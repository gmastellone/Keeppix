use std::time::Duration;

use keeppix_db::{AssetRepo, Db, FolderRepo, JobRepo, LibraryRepo};
use keeppix_domain::{
    AssetKind, AssetName, JobKind, JobPriority, LibraryId, LibraryStatus, NewAsset,
};
use keeppix_media::{iter_entries, restat_if_stable};
use uuid::Uuid;

use crate::JobError;

/// Scansiona l'albero. Non apre i file: solo `stat` e insert.
///
/// # Errors
/// `MassDisappearance` se è sparito più del 20% dei file noti; errori di I/O
/// o di database.
pub async fn run(db: &Db, library_id: LibraryId, stability_wait: Duration) -> Result<(), JobError> {
    let libraries = LibraryRepo::new(db);
    let library = libraries.load_for_scan(library_id).await?;
    let assets = AssetRepo::new(db);
    let folders = FolderRepo::new(db);
    let jobs = JobRepo::new(db);

    let existing = assets.count_in_library(library_id).await?;
    let root = &library.root_path;

    if !root.is_dir() {
        if existing > 0 {
            libraries
                .set_status_for_scan(library_id, LibraryStatus::Offline)
                .await?;
        }
        return Ok(());
    }

    let mut seen = Vec::new();
    for walked in iter_entries(root, &library.exclude_patterns) {
        let Some(meta) = restat_if_stable(&walked.path, stability_wait)
            .map_err(|e| JobError::Worker(format!("stat {}: {e}", walked.path.display())))?
        else {
            continue;
        };
        let mut file = walked;
        file.size_bytes = i64::try_from(meta.len()).unwrap_or(file.size_bytes);
        if let Ok(mtime) = meta.modified() {
            file.mtime = chrono::DateTime::<chrono::Utc>::from(mtime);
        }
        seen.push(file);
    }

    if existing > 0 && seen.is_empty() {
        libraries
            .set_status_for_scan(library_id, LibraryStatus::Offline)
            .await?;
        return Ok(());
    }

    let seen_n = i64::try_from(seen.len()).unwrap_or(i64::MAX);
    if existing > 0 && seen_n.saturating_mul(5) < existing.saturating_mul(4) {
        return Err(JobError::MassDisappearance);
    }

    libraries
        .set_status_for_scan(library_id, LibraryStatus::Active)
        .await?;

    for file in seen {
        let relative: Vec<&str> = file.relative_dir.iter().map(String::as_str).collect();
        let folder = folders.ensure_path(library_id, &relative).await?;
        let filename = AssetName::parse(&file.filename)
            .map_err(|e| JobError::Worker(format!("filename: {e}")))?;
        let asset = assets
            .upsert_discovered(NewAsset {
                folder_id: folder.id,
                filename,
                size_bytes: file.size_bytes,
                mtime: file.mtime,
                inode: file.inode,
                kind: AssetKind::Unknown,
            })
            .await?;
        jobs.enqueue(
            JobKind::ExtractMetadata,
            serde_json::json!({ "asset_id": asset.id.to_string() }),
            JobPriority::Background,
            Some(&format!("meta:{}", asset.id)),
        )
        .await?;
    }

    libraries.mark_scanned(library_id).await?;
    Ok(())
}

/// # Errors
/// `JobError::Worker` se manca `library_id` o non è un UUID.
pub fn library_id_from_payload(payload: &serde_json::Value) -> Result<LibraryId, JobError> {
    let raw = payload
        .get("library_id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| JobError::Worker("payload.library_id missing".to_owned()))?;
    let uuid =
        Uuid::parse_str(raw).map_err(|e| JobError::Worker(format!("payload.library_id: {e}")))?;
    Ok(LibraryId::from_uuid(uuid))
}
