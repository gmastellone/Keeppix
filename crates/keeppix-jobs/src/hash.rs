use keeppix_db::{AssetRepo, Db, FolderRepo, JobRepo};
use keeppix_domain::{AssetId, AssetKind, JobKind, JobPriority};
use keeppix_media::hash_file;

use crate::JobError;

/// # Errors
/// I/O o database.
pub async fn run(db: &Db, asset_id: AssetId) -> Result<(), JobError> {
    let assets = AssetRepo::new(db);
    let asset = assets.get_for_scan(asset_id).await?;
    let path = FolderRepo::new(db)
        .absolute_path_for_scan(asset.folder_id)
        .await?
        .join(asset.filename.as_str());

    let meta = std::fs::metadata(&path).map_err(|e| JobError::Worker(e.to_string()))?;
    let size = i64::try_from(meta.len()).unwrap_or(asset.size_bytes);
    let mtime_ok = meta.modified().ok().is_some_and(|m| {
        chrono::DateTime::<chrono::Utc>::from(m).timestamp() == asset.mtime.timestamp()
    });
    let inode_ok = inode_of(&meta) == asset.inode;
    if let Some(hash) = asset.content_hash
        && size == asset.size_bytes
        && mtime_ok
        && inode_ok
    {
        enqueue_derive(db, hash, asset.kind).await?;
        return Ok(());
    }

    let hash = hash_file(&path).map_err(|e| JobError::Worker(e.to_string()))?;
    assets.set_hash(asset_id, hash).await?;
    crate::moves::after_hash(db, asset_id, hash, size).await?;
    enqueue_derive(db, hash, asset.kind).await?;
    Ok(())
}

/// I RAW hanno una pipeline di derivazione diversa (preview incorporata
/// prima del demosaic, `JobKind::DeriveRaw`); tutti gli altri kind restano
/// su `DeriveAsset`.
pub(crate) async fn enqueue_derive(
    db: &Db,
    hash: [u8; 32],
    kind: AssetKind,
) -> Result<(), JobError> {
    let mut hex = String::with_capacity(64);
    for b in hash {
        use std::fmt::Write as _;
        let _ = write!(hex, "{b:02x}");
    }
    let (job_kind, dedup_prefix) = match kind {
        AssetKind::RawImage => (JobKind::DeriveRaw, "derive_raw"),
        _ => (JobKind::DeriveAsset, "derive"),
    };
    JobRepo::new(db)
        .enqueue(
            job_kind,
            serde_json::json!({ "content_hash": hex }),
            JobPriority::Background,
            Some(&format!("{dedup_prefix}:{hex}")),
        )
        .await?;
    Ok(())
}

fn inode_of(meta: &std::fs::Metadata) -> Option<i64> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        i64::try_from(meta.ino()).ok()
    }
    #[cfg(not(unix))]
    {
        let _ = meta;
        None
    }
}
