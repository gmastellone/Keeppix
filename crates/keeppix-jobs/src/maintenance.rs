//! Completes the maintenance scheduler (Fase 6 Task 8).
//! Reuses the same `schedule()` pattern as `cleanup_trash` / `tmp_cleanup`.
//! Heavy work is `JobPriority::Background`, so `EnergyProfile::Interactive`
//! never claims it (see `EnergyProfile::max_priority`).

use std::path::Path;
use std::time::Duration;

use chrono::{Duration as ChronoDuration, Utc};
use keeppix_db::{AssetRepo, BackupRepo, Db, FolderRepo, IdempotencyRepo, JobRepo, SessionRepo};
use keeppix_domain::{JobKind, JobPriority};
use keeppix_media::hash_file;
use serde_json::json;

use crate::JobError;

/// # Errors
/// Database.
pub async fn purge_sessions(db: &Db) -> Result<(), JobError> {
    let n = SessionRepo::new(db).purge_expired().await?;
    tracing::info!(purged = n, "expired sessions purged");
    Ok(())
}

/// # Errors
/// Database.
pub async fn cleanup_done_jobs(db: &Db) -> Result<(), JobError> {
    let before = Utc::now() - ChronoDuration::days(7);
    let n = JobRepo::new(db).delete_done_older_than(before).await?;
    tracing::info!(deleted = n, "done jobs older than 7d cleaned");
    Ok(())
}

/// # Errors
/// Database.
pub async fn cleanup_idempotency(db: &Db) -> Result<(), JobError> {
    let before = Utc::now() - ChronoDuration::hours(24);
    let n = IdempotencyRepo::new(db).purge_older_than(before).await?;
    tracing::info!(deleted = n, "idempotency keys older than 24h cleaned");
    Ok(())
}

/// Reap HLS/transcode cache entries whose mtime is older than 90 days.
///
/// # Errors
/// I/O on the cache tree.
pub fn cleanup_transcode_cache(data_dir: &Path) -> Result<(), JobError> {
    let root = data_dir.join("video-cache");
    if !root.exists() {
        return Ok(());
    }
    let cutoff = std::time::SystemTime::now() - Duration::from_secs(90 * 24 * 60 * 60);
    let mut removed = 0u64;
    cleanup_tree(&root, cutoff, &mut removed)?;
    tracing::info!(removed, "transcode cache entries older than 90d cleaned");
    Ok(())
}

fn cleanup_tree(
    dir: &Path,
    cutoff: std::time::SystemTime,
    removed: &mut u64,
) -> Result<(), JobError> {
    for entry in std::fs::read_dir(dir).map_err(|e| JobError::Worker(e.to_string()))? {
        let entry = entry.map_err(|e| JobError::Worker(e.to_string()))?;
        let path = entry.path();
        let meta = entry
            .metadata()
            .map_err(|e| JobError::Worker(e.to_string()))?;
        if meta.is_dir() {
            cleanup_tree(&path, cutoff, removed)?;
            if std::fs::read_dir(&path)
                .map(|mut i| i.next().is_none())
                .unwrap_or(false)
            {
                let _ = std::fs::remove_dir(&path);
            }
        } else {
            let mtime = meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            if mtime < cutoff && std::fs::remove_file(&path).is_ok() {
                *removed += 1;
            }
        }
    }
    Ok(())
}

/// # Errors
/// Database.
pub async fn vacuum_analyze(db: &Db) -> Result<(), JobError> {
    BackupRepo::new(db).vacuum_analyze().await?;
    tracing::info!("VACUUM ANALYZE completed");
    Ok(())
}

/// Integrity scrub: re-hash ~5% of ready assets. Reports mismatches; never
/// auto-deletes.
///
/// # Errors
/// Database or I/O.
pub async fn integrity_scrub(db: &Db) -> Result<(), JobError> {
    let sample = BackupRepo::new(db).sample_assets_for_scrub(5).await?;
    let assets = AssetRepo::new(db);
    let folders = FolderRepo::new(db);
    let mut mismatches = 0u64;
    for (id, expected_hash, filename) in sample {
        let asset_id = keeppix_domain::AssetId::from_uuid(id);
        let asset = match assets.get_for_scan(asset_id).await {
            Ok(a) => a,
            Err(e) => {
                tracing::warn!(error = %e, asset = %id, "scrub skip: cannot load asset");
                continue;
            }
        };
        let path = match folders.absolute_path_for_scan(asset.folder_id).await {
            Ok(p) => p.join(&filename),
            Err(e) => {
                tracing::warn!(error = %e, asset = %id, "scrub skip: cannot resolve path");
                continue;
            }
        };
        if !path.exists() {
            tracing::warn!(asset = %id, path = %path.display(), "integrity: file missing");
            mismatches += 1;
            continue;
        }
        match hash_file(&path) {
            Ok(actual) => {
                if actual.as_slice() != expected_hash.as_slice() {
                    tracing::error!(
                        asset = %id,
                        path = %path.display(),
                        "integrity: content hash mismatch (report only, not deleted)"
                    );
                    mismatches += 1;
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, asset = %id, "integrity: hash failed");
                mismatches += 1;
            }
        }
    }
    tracing::info!(mismatches, "integrity scrub finished");
    Ok(())
}

macro_rules! schedule_one {
    ($name:ident, $kind:expr, $dedup:expr) => {
        /// # Errors
        /// Database.
        pub async fn $name(db: &Db) -> Result<(), JobError> {
            JobRepo::new(db)
                .enqueue_after(
                    $kind,
                    json!({}),
                    JobPriority::Background,
                    Some($dedup),
                    Utc::now(),
                )
                .await?;
            Ok(())
        }
    };
}

schedule_one!(
    schedule_purge_sessions,
    JobKind::PurgeSessions,
    "purge_sessions"
);
schedule_one!(
    schedule_cleanup_done_jobs,
    JobKind::CleanupDoneJobs,
    "cleanup_done_jobs"
);
schedule_one!(
    schedule_cleanup_transcode_cache,
    JobKind::CleanupTranscodeCache,
    "cleanup_transcode_cache"
);
schedule_one!(
    schedule_cleanup_idempotency,
    JobKind::CleanupIdempotency,
    "cleanup_idempotency"
);
schedule_one!(
    schedule_vacuum_analyze,
    JobKind::VacuumAnalyze,
    "vacuum_analyze"
);
schedule_one!(
    schedule_integrity_scrub,
    JobKind::IntegrityScrub,
    "integrity_scrub"
);

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::EnergyProfile;

    #[test]
    fn maintenance_jobs_are_background_so_interactive_will_not_claim_them() {
        assert!(
            EnergyProfile::Interactive.max_priority() < JobPriority::Background,
            "Interactive must not run Background maintenance"
        );
        assert_eq!(
            EnergyProfile::Background.max_priority(),
            JobPriority::Background
        );
        assert_eq!(EnergyProfile::Night.max_priority(), JobPriority::Background);
    }
}
