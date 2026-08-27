//! Periodic cleanup of expired upload sessions in `.keeppix-tmp/`:
//! `UploadSessionRepo::delete_expired` already existed but no production
//! code path called it — an upload abandoned halfway would have stayed on
//! disk forever, past the `expires_at` meant to free it.

use chrono::Utc;
use keeppix_db::{Db, JobRepo, UploadSessionRepo};
use keeppix_domain::{JobKind, JobPriority};
use serde_json::json;

use crate::JobError;

/// # Errors
/// Database, or I/O on the temp file (in which case the row stays, same as
/// `cleanup_trash::run`).
pub async fn run(db: &Db) -> Result<(), JobError> {
    UploadSessionRepo::new(db)
        .delete_expired(Utc::now())
        .await?;
    Ok(())
}

/// Enqueue a run, deduplicated across pending/running. Triggered on startup
/// and then periodically from the binary — not from the job itself, since
/// the `dedup_key` would collide with the job still `running`.
///
/// # Errors
/// Database.
pub async fn schedule(db: &Db) -> Result<(), JobError> {
    JobRepo::new(db)
        .enqueue_after(
            JobKind::TmpCleanup,
            json!({}),
            JobPriority::Background,
            Some("tmp_cleanup"),
            Utc::now(),
        )
        .await?;
    Ok(())
}
