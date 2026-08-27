use std::collections::HashMap;
use std::time::Duration;

use chrono::{TimeDelta, Utc};
use keeppix_db::{AssetRepo, Db, FolderRepo, JobRepo, LibraryRepo, OperationsRepo};
use keeppix_domain::{
    AssetId, AssetKind, AssetName, FolderId, JobKind, JobPriority, LibraryId, LibraryStatus,
    NewAsset, OperationId,
};
use keeppix_media::{Freshness, WalkedFile, freshness, iter_entries};
use uuid::Uuid;

use crate::{JobError, PRODUCTION_BATCH_SIZE, PRODUCTION_STABILITY_WAIT, maintenance};

/// Scans the tree. Never opens files: only `stat` and batched inserts.
///
/// `settled_after`: minimum `mtime` age for a file to count as settled.
/// More recent files are `InFlight`: they don't block the cycle; a recheck
/// is enqueued with `run_after = now() + PRODUCTION_STABILITY_WAIT`.
///
/// # Errors
/// `MassDisappearance` if more than 20% of known files have disappeared;
/// I/O or database errors.
pub async fn run(db: &Db, library_id: LibraryId, settled_after: Duration) -> Result<(), JobError> {
    run_with_operation(db, library_id, settled_after, None).await
}

/// Like [`run`], but tracks an operation: progress and cancellation
/// (`operations`, read by the WebSocket poll) instead of a silent scan.
/// This is generic long-running-operation infrastructure hung off the one
/// real long-op already present — library scanning.
///
/// **Ruling: cancelling midway produces a partial success, not a
/// rollback.** Assets already written in this pass stay; the operation
/// closes as `Cancelled` listing them, it does not attempt to undo them.
///
/// # Errors
/// Same as [`run`].
pub async fn run_with_operation(
    db: &Db,
    library_id: LibraryId,
    settled_after: Duration,
    operation_id: Option<OperationId>,
) -> Result<(), JobError> {
    let libraries = LibraryRepo::new(db);
    let library = libraries.load_for_scan(library_id).await?;
    let assets = AssetRepo::new(db);
    let jobs = JobRepo::new(db);
    let ops = OperationsRepo::new(db);

    let existing = assets.count_in_library(library_id).await?;
    if let Some(op_id) = operation_id {
        // Honest, not made up: a first import doesn't know how many files
        // it will find until it has found them, so this stays `None` — the
        // frontend draws an indeterminate progress bar instead of a fake
        // one (same choice as `FailureReason::Unknown`).
        ops.set_total(op_id, (existing > 0).then_some(existing))
            .await?;
    }
    let root = &library.root_path;

    if !root.is_dir() {
        if existing > 0 {
            libraries
                .set_status_for_scan(library_id, LibraryStatus::Offline)
                .await?;
        }
        finish_done_if_tracked(&ops, operation_id).await?;
        return Ok(());
    }

    let mut batch: Vec<WalkedFile> = Vec::with_capacity(PRODUCTION_BATCH_SIZE);
    let mut present: i64 = 0;
    let mut had_inflight = false;
    let mut cancelled = false;
    // Covers the whole scan, not just the current batch: files in the same
    // folder (the common case) don't pay for `ensure_path` a second time.
    let mut folder_cache: HashMap<Vec<String>, FolderId> = HashMap::new();

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
                    cancelled =
                        flush_batch(db, library_id, &mut batch, &mut folder_cache, operation_id)
                            .await?;
                    if cancelled {
                        break;
                    }
                }
            }
        }
    }

    if cancelled {
        finish_cancelled_if_tracked(&ops, operation_id).await?;
        return Ok(());
    }

    if existing > 0 && present == 0 {
        libraries
            .set_status_for_scan(library_id, LibraryStatus::Offline)
            .await?;
        finish_done_if_tracked(&ops, operation_id).await?;
        return Ok(());
    }

    if existing > 0 && present.saturating_mul(5) < existing.saturating_mul(4) {
        // An error, not a completion: the operation stays `running` — the
        // job retry will resume it with the same `operation_id`.
        return Err(JobError::MassDisappearance);
    }

    libraries
        .set_status_for_scan(library_id, LibraryStatus::Active)
        .await?;

    if flush_batch(db, library_id, &mut batch, &mut folder_cache, operation_id).await? {
        finish_cancelled_if_tracked(&ops, operation_id).await?;
        return Ok(());
    }

    if had_inflight {
        let run_after = Utc::now()
            + TimeDelta::from_std(PRODUCTION_STABILITY_WAIT)
                .unwrap_or_else(|_| TimeDelta::seconds(5));
        let mut payload = serde_json::json!({ "library_id": library_id.to_string() });
        if let Some(op_id) = operation_id {
            payload["operation_id"] = serde_json::Value::String(op_id.to_string());
        }
        jobs.enqueue_after(
            JobKind::DiscoverLibrary,
            payload,
            JobPriority::Background,
            Some(&format!("discover-retry:{library_id}")),
            run_after,
        )
        .await?;
    }

    libraries.mark_scanned(library_id).await?;
    maintenance::schedule_vacuum_analyze(db).await?;
    // With `had_inflight` there's still work coming with this same
    // `operation_id`: closing it `Done` now would lie about progress.
    if !had_inflight {
        finish_done_if_tracked(&ops, operation_id).await?;
    }
    Ok(())
}

async fn finish_done_if_tracked(
    ops: &OperationsRepo<'_>,
    operation_id: Option<OperationId>,
) -> Result<(), JobError> {
    if let Some(op_id) = operation_id {
        ops.finish_done(op_id).await?;
    }
    Ok(())
}

async fn finish_cancelled_if_tracked(
    ops: &OperationsRepo<'_>,
    operation_id: Option<OperationId>,
) -> Result<(), JobError> {
    if let Some(op_id) = operation_id {
        ops.finish_cancelled(op_id).await?;
    }
    Ok(())
}

/// Writes the batch in a handful of multi-row statements instead of one
/// query per file (per-file overhead — queueing plus database round-trips —
/// is the largest share of import time, and the only part made of
/// groupable work). A single `INSERT` upserts all assets in the batch, a
/// single `INSERT` enqueues metadata jobs for the ones that are actually
/// new/changed, a single `UPDATE` advances the tracked operation.
///
/// The `assets_change_log` trigger still writes one `change_log` row per
/// asset (entity-per-entity, as required by mobile sync): what's reduced
/// here is the number of **network round-trips**, not the number of rows in
/// the log.
///
/// The cancellation check happens once per batch (up to
/// `PRODUCTION_BATCH_SIZE` files), not once per file as before: coarser,
/// but the whole point of batching is avoiding a query per file, and this
/// check is no exception.
///
/// Returns `true` if the tracked operation was cancelled before writing
/// this batch: the caller must stop immediately, without continuing with
/// more files.
async fn flush_batch(
    db: &Db,
    library_id: LibraryId,
    batch: &mut Vec<WalkedFile>,
    folder_cache: &mut HashMap<Vec<String>, FolderId>,
    operation_id: Option<OperationId>,
) -> Result<bool, JobError> {
    if batch.is_empty() {
        return Ok(false);
    }
    let ops = OperationsRepo::new(db);
    if let Some(op_id) = operation_id
        && ops.is_cancel_requested(op_id).await?
    {
        batch.clear();
        return Ok(true);
    }

    let folders = FolderRepo::new(db);
    let mut new_assets = Vec::with_capacity(batch.len());
    for file in batch.drain(..) {
        let folder_id =
            resolve_folder(&folders, library_id, &file.relative_dir, folder_cache).await?;
        let filename = AssetName::parse(&file.filename)
            .map_err(|e| JobError::Worker(format!("filename: {e}")))?;
        new_assets.push(NewAsset {
            folder_id,
            filename,
            size_bytes: file.size_bytes,
            mtime: file.mtime,
            inode: file.inode,
            kind: AssetKind::Unknown,
        });
    }

    let assets = AssetRepo::new(db);
    let changed = assets.batch_upsert_discovered(&new_assets).await?;
    if changed.is_empty() {
        return Ok(false);
    }

    let jobs = JobRepo::new(db);
    let metadata_jobs: Vec<(serde_json::Value, String)> = changed
        .iter()
        .map(|asset| {
            (
                serde_json::json!({ "asset_id": asset.id.to_string() }),
                format!("meta:{}", asset.id),
            )
        })
        .collect();
    jobs.enqueue_many(
        JobKind::ExtractMetadata,
        &metadata_jobs,
        JobPriority::Background,
    )
    .await?;

    if let Some(op_id) = operation_id {
        let ids: Vec<AssetId> = changed.iter().map(|asset| asset.id).collect();
        ops.record_success_many(op_id, &ids).await?;
    }

    Ok(false)
}

/// Folder for `relative` under `library_id`, with a cache that covers the
/// whole scan (not just the current batch): files in the same folder — the
/// common case, an import is usually a handful of folders with hundreds of
/// files each — don't pay for `FolderRepo::ensure_path` a second time.
async fn resolve_folder(
    folders: &FolderRepo<'_>,
    library_id: LibraryId,
    relative: &[String],
    cache: &mut HashMap<Vec<String>, FolderId>,
) -> Result<FolderId, JobError> {
    if let Some(id) = cache.get(relative) {
        return Ok(*id);
    }
    let refs: Vec<&str> = relative.iter().map(String::as_str).collect();
    let folder = folders.ensure_path(library_id, &refs).await?;
    cache.insert(relative.to_vec(), folder.id);
    Ok(folder.id)
}

/// # Errors
/// `JobError::Worker` if `library_id` is missing or not a UUID.
pub fn library_id_from_payload(payload: &serde_json::Value) -> Result<LibraryId, JobError> {
    let raw = payload
        .get("library_id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| JobError::Worker("payload.library_id missing".to_owned()))?;
    let uuid =
        Uuid::parse_str(raw).map_err(|e| JobError::Worker(format!("payload.library_id: {e}")))?;
    Ok(LibraryId::from_uuid(uuid))
}

/// `None` when the payload carries no `operation_id` — the normal case for
/// a rescan triggered by the watcher, with no user waiting on it.
#[must_use]
pub fn operation_id_from_payload(payload: &serde_json::Value) -> Option<OperationId> {
    payload
        .get("operation_id")
        .and_then(serde_json::Value::as_str)
        .and_then(|raw| Uuid::parse_str(raw).ok())
        .map(OperationId::from_uuid)
}
