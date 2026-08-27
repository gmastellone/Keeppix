//! `JobKind::WriteSidecar`: flushes pending overrides out to the files.
//! "DB first, file later": the user has already seen the change (the
//! `UPDATE` on `asset_overrides` is synchronous), this job is only the
//! propagation — asynchronous, background priority, retryable — to the
//! `.xmp` sidecar next to the file.
//!
//! Unlike `DeriveRaw` (one job per asset, carried in the payload), this job
//! carries no asset in its payload: each run rereads
//! `OverrideRepo::pending_sidecars` and processes a batch. This way an
//! `apply_batch` over 500 assets produces **one** job (the enqueue is
//! deduplicated by `keeppix-db`), not 500.

use std::path::{Path, PathBuf};

use keeppix_db::{AssetRepo, Db, DbError, FolderRepo, JobRepo, OverrideRepo};
use keeppix_domain::{AssetId, JobKind, JobPriority, Pick, Rating};
use keeppix_media::SidecarData;

use crate::JobError;

/// Assets processed per run. Enough to quickly drain a small library; on a
/// larger one the job re-enqueues itself (see [`run`]) instead of keeping
/// the worker busy for minutes in one go.
const BATCH_LIMIT: i64 = 200;

/// # Errors
/// Database, or one or more sidecars not writable (read-only folder, full
/// disk, a RAW whose file has disappeared, ...). In that case the job fails
/// with a retry — assets written successfully stay marked as such, so a
/// retry only processes the ones left behind, not the whole batch again.
pub async fn run(db: &Db) -> Result<(), JobError> {
    let pending = OverrideRepo::new(db).pending_sidecars(BATCH_LIMIT).await?;
    if pending.is_empty() {
        return Ok(());
    }

    let mut failures = Vec::new();
    for asset_id in &pending {
        if let Err(e) = write_one(db, *asset_id).await {
            failures.push(format!("{asset_id}: {e}"));
        }
    }

    let pending_count = i64::try_from(pending.len()).unwrap_or(i64::MAX);
    if pending_count >= BATCH_LIMIT {
        // The batch was full: there could still be more work. Re-enqueueing
        // instead of continuing here keeps every run bounded in time even
        // on a library with thousands of pending overrides.
        JobRepo::new(db)
            .enqueue(
                JobKind::WriteSidecar,
                serde_json::json!({}),
                JobPriority::Background,
                Some("write_sidecar"),
            )
            .await?;
    }

    if failures.is_empty() {
        Ok(())
    } else {
        Err(JobError::Worker(failures.join("; ")))
    }
}

async fn write_one(db: &Db, asset_id: AssetId) -> Result<(), JobError> {
    let assets = AssetRepo::new(db);
    let asset = match assets.get_for_scan(asset_id).await {
        Ok(asset) => asset,
        // Deleted between enqueueing the job and running it: its override
        // row is already gone with it (ON DELETE CASCADE on
        // asset_overrides), so it will not reappear on the next round.
        Err(DbError::NotFound) => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    let folder_path = FolderRepo::new(db)
        .absolute_path_for_scan(asset.folder_id)
        .await?;
    let sidecar_path = sidecar_path_for(&folder_path.join(asset.filename.as_str()));

    let overrides = OverrideRepo::new(db);
    let source = overrides.sidecar_source(asset_id).await?;
    let data = SidecarData {
        rating: source.owner_rating.map(Rating::value),
        description: source.effective.description,
        title: source.effective.title,
        tags: Vec::new(),
        gps: source.effective.location,
        taken_at: source.effective.taken_at,
        label: pick_label(source.owner_pick),
    };

    keeppix_media::write_sidecar(&sidecar_path, &data).map_err(|e| map_sidecar_error(&e))?;
    overrides.mark_sidecar_written(asset_id).await?;
    Ok(())
}

/// `permission-denied: ` is a stable marker, not free text: it's what
/// `keeppix_db::ProblemsRepo::composed` uses to recognize the
/// "XMP sidecar not writable" condition in `jobs.last_error` without having
/// to parse the message of an `io::Error`, which could word itself
/// differently depending on the platform.
fn map_sidecar_error(e: &keeppix_media::XmpError) -> JobError {
    if let keeppix_media::XmpError::Io(io_err) = e
        && io_err.kind() == std::io::ErrorKind::PermissionDenied
    {
        return JobError::Worker(format!("permission-denied: {e}"));
    }
    JobError::Worker(e.to_string())
}

/// `IMG_1234.ARW` → `IMG_1234.ARW.xmp`, next to the file.
fn sidecar_path_for(asset_path: &Path) -> PathBuf {
    let mut name = asset_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_owned();
    name.push_str(".xmp");
    asset_path.with_file_name(name)
}

/// `xmp:Label` carries the pick/reject only if the user has voted:
/// `Pick::None` writes nothing, so an asset never voted on doesn't end up
/// with an empty label in the sidecar.
fn pick_label(pick: Pick) -> Option<String> {
    (pick != Pick::None).then(|| pick.as_str().to_owned())
}
