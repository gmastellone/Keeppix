use std::time::Duration;

use chrono::{TimeDelta, Utc};
use keeppix_db::{AssetRepo, Db, FolderRepo, JobRepo, LibraryRepo};
use keeppix_domain::{
    AssetKind, AssetName, JobKind, JobPriority, LibraryId, LibraryStatus, NewAsset,
};
use keeppix_media::{Freshness, WalkedFile, freshness, iter_entries};
use uuid::Uuid;

use crate::{JobError, PRODUCTION_BATCH_SIZE, PRODUCTION_STABILITY_WAIT};

/// Scansiona l'albero. Non apre i file: solo `stat` e insert a lotti.
///
/// `settled_after`: età minima di `mtime` perché un file conti come fermo.
/// I file più recenti sono `InFlight`: non bloccano il ciclo; si accoda un
/// ricontrollo con `run_after = now() + PRODUCTION_STABILITY_WAIT`.
///
/// # Errors
/// `MassDisappearance` se è sparito più del 20% dei file noti; errori di I/O
/// o di database.
pub async fn run(db: &Db, library_id: LibraryId, settled_after: Duration) -> Result<(), JobError> {
    let libraries = LibraryRepo::new(db);
    let library = libraries.load_for_scan(library_id).await?;
    let assets = AssetRepo::new(db);
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

    let mut batch: Vec<WalkedFile> = Vec::with_capacity(PRODUCTION_BATCH_SIZE);
    let mut present: i64 = 0;
    let mut had_inflight = false;

    for walked in iter_entries(root, &library.exclude_patterns) {
        match freshness(&walked.path, settled_after)
            .map_err(|e| JobError::Worker(format!("stat {}: {e}", walked.path.display())))?
        {
            Freshness::InFlight => {
                present = present.saturating_add(1);
                had_inflight = true;
            }
            Freshness::Settled(meta) => {
                present = present.saturating_add(1);
                let mut file = walked;
                file.size_bytes = i64::try_from(meta.len()).unwrap_or(file.size_bytes);
                if let Ok(mtime) = meta.modified() {
                    file.mtime = chrono::DateTime::<chrono::Utc>::from(mtime);
                }
                batch.push(file);
                if batch.len() >= PRODUCTION_BATCH_SIZE {
                    flush_batch(db, library_id, &mut batch).await?;
                }
            }
        }
    }

    if existing > 0 && present == 0 {
        libraries
            .set_status_for_scan(library_id, LibraryStatus::Offline)
            .await?;
        return Ok(());
    }

    if existing > 0 && present.saturating_mul(5) < existing.saturating_mul(4) {
        return Err(JobError::MassDisappearance);
    }

    libraries
        .set_status_for_scan(library_id, LibraryStatus::Active)
        .await?;

    flush_batch(db, library_id, &mut batch).await?;

    if had_inflight {
        let run_after = Utc::now()
            + TimeDelta::from_std(PRODUCTION_STABILITY_WAIT)
                .unwrap_or_else(|_| TimeDelta::seconds(5));
        jobs.enqueue_after(
            JobKind::DiscoverLibrary,
            serde_json::json!({ "library_id": library_id.to_string() }),
            JobPriority::Background,
            Some(&format!("discover-retry:{library_id}")),
            run_after,
        )
        .await?;
    }

    libraries.mark_scanned(library_id).await?;
    Ok(())
}

async fn flush_batch(
    db: &Db,
    library_id: LibraryId,
    batch: &mut Vec<WalkedFile>,
) -> Result<(), JobError> {
    if batch.is_empty() {
        return Ok(());
    }
    let assets = AssetRepo::new(db);
    let folders = FolderRepo::new(db);
    let jobs = JobRepo::new(db);
    for file in batch.drain(..) {
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
