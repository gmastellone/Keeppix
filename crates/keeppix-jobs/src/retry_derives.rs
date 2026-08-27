//! Retries derives that left the asset in `error` without failing the job
//! (`DeriveRaw` returns `Ok` after `set_error`). Rescanning doesn't touch
//! them: it skips unchanged assets.

use chrono::Utc;
use keeppix_db::{AssetRepo, Db, JobRepo};
use keeppix_domain::{JobKind, JobPriority};
use serde_json::json;

use crate::JobError;
use crate::hash::enqueue_derive;

pub const MAX_DERIVE_ATTEMPTS: i64 = 3;

/// # Errors
/// Database.
pub async fn run(db: &Db) -> Result<(), JobError> {
    let jobs = JobRepo::new(db);
    for (hash, kind) in AssetRepo::new(db).error_hashes_for_retry().await? {
        let mut hex = String::with_capacity(64);
        for b in hash {
            use std::fmt::Write as _;
            let _ = write!(hex, "{b:02x}");
        }
        let prefix = match kind {
            keeppix_domain::AssetKind::RawImage => "derive_raw",
            _ => "derive",
        };
        let key = format!("{prefix}:{hex}");
        if jobs.count_for_dedup_key(&key).await? >= MAX_DERIVE_ATTEMPTS {
            continue;
        }
        enqueue_derive(db, hash, kind).await?;
    }
    Ok(())
}

/// Enqueue a run, deduplicated across pending/running. Triggered on startup
/// and then every 15 minutes from the binary — not from the job itself,
/// since the `dedup_key` would collide with the job still `running`.
///
/// # Errors
/// Database.
pub async fn schedule(db: &Db) -> Result<(), JobError> {
    JobRepo::new(db)
        .enqueue_after(
            JobKind::RetryErrorAssets,
            json!({}),
            JobPriority::Background,
            Some("retry_error_assets"),
            Utc::now(),
        )
        .await?;
    Ok(())
}
