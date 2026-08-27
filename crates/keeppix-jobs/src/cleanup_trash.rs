//! Periodic trash pruning. `TrashRepo::cleanup_expired` already existed
//! and was covered by tests, but no production code path called it:
//! deleted photos stayed on disk forever.

use chrono::{Duration, Utc};
use keeppix_db::{Db, JobRepo, TrashRepo};
use keeppix_domain::{JobKind, JobPriority};
use serde_json::json;

use crate::JobError;

/// # Errors
/// Database, or I/O on the file in trash (in which case the row stays).
pub async fn run(db: &Db, retention_days: i64) -> Result<(), JobError> {
    let days = retention_days.max(1);
    let before = Utc::now() - Duration::days(days);
    TrashRepo::new(db).cleanup_expired(before).await?;
    Ok(())
}

/// Enqueue a run, deduplicated across pending/running. Triggered on startup
/// and then every 24h from the binary — not from the job itself, since the
/// `dedup_key` would collide with the job still `running`.
///
/// # Errors
/// Database.
pub async fn schedule(db: &Db) -> Result<(), JobError> {
    JobRepo::new(db)
        .enqueue_after(
            JobKind::CleanupTrash,
            json!({}),
            JobPriority::Background,
            Some("cleanup_trash"),
            Utc::now(),
        )
        .await?;
    Ok(())
}
