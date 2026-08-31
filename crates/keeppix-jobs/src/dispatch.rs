use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use keeppix_db::Db;
use keeppix_domain::{Job, JobKind};

use crate::JobError;
use crate::derive as derive_job;
use crate::discover;
use crate::hash as hash_job;
use crate::metadata;
use crate::profile::{ActivityTracker, DEFAULT_ANALYSIS_IDLE_MS};
use crate::raw as raw_job;
use crate::xmp as xmp_job;

/// Single handler for the ingest pipeline.
#[derive(Clone)]
pub struct IngestHandler {
    pub db: Db,
    pub data_dir: PathBuf,
    pub stability_wait: Duration,
    pub trash_retention_days: i64,
    pub database_url: String,
    pub config_path: Option<PathBuf>,
    /// Same tracker as the pool: between embed batches it decides whether
    /// the analysis window stays open (viewport idle).
    pub activity: Arc<ActivityTracker>,
}

impl crate::JobHandler for IngestHandler {
    fn ram_hint_bytes(&self, job: &Job) -> u64 {
        match job.kind {
            JobKind::DeriveAsset => DEFAULT_RAM_HINT,
            // DeriveRaw: out-of-process demosaic. EmbedAssets: OpenCLIP
            // XLM-R IT/EN, visual+text ≈ 530-550 MB RSS per batch, measured
            // on real CI benchmarks. DetectFaces: YuNet+SFace, same order of
            // magnitude as a second ort stack. Same gate ceiling.
            JobKind::DeriveRaw | JobKind::EmbedAssets | JobKind::DetectFaces => 512 * 1024 * 1024,
            JobKind::TranscodeVideo => 1024 * 1024 * 1024,
            JobKind::BackupDump | JobKind::RestoreProof | JobKind::VacuumAnalyze => {
                256 * 1024 * 1024
            }
            // WriteSidecar writes a small text file per asset: as light as
            // the other maintenance jobs, covered by the default.
            _ => 8 * 1024 * 1024,
        }
    }

    async fn handle(&self, job: &Job) -> Result<(), JobError> {
        match job.kind {
            JobKind::DiscoverLibrary => {
                let id = discover::library_id_from_payload(&job.payload)?;
                let operation_id = discover::operation_id_from_payload(&job.payload);
                // `stability_wait` on the handler is the age threshold
                // (settled_after), not a sleep: in production it's
                // `PRODUCTION_SETTLED_AFTER`.
                discover::run_with_operation(&self.db, id, self.stability_wait, operation_id).await
            }
            JobKind::ExtractMetadata => {
                let id = metadata::asset_id_from_payload(&job.payload)?;
                metadata::run(&self.db, id).await
            }
            JobKind::HashAsset => {
                let id = metadata::asset_id_from_payload(&job.payload)?;
                hash_job::run(&self.db, id).await
            }
            JobKind::DeriveAsset => {
                let hash = derive_job::hash_from_payload(&job.payload)?;
                derive_job::run(&self.db, &self.data_dir, hash).await
            }
            JobKind::DeriveRaw => {
                let hash = derive_job::hash_from_payload(&job.payload)?;
                raw_job::run(&self.db, &self.data_dir, hash).await
            }
            JobKind::WriteSidecar => xmp_job::run(&self.db).await,
            JobKind::CleanupTrash => {
                crate::cleanup_trash::run(&self.db, self.trash_retention_days).await
            }
            JobKind::RetryErrorAssets => crate::retry_derives::run(&self.db).await,
            JobKind::TmpCleanup => crate::tmp_cleanup::run(&self.db).await,
            JobKind::TranscodeVideo => {
                let id = crate::transcode::asset_id_from_payload(&job.payload)?;
                let save = crate::transcode::save_bandwidth_from_payload(&job.payload)?;
                crate::transcode::run(&self.db, &self.data_dir, id, save).await
            }
            JobKind::DownloadMapRegion => crate::regions::run(&self.db, &self.data_dir, job).await,
            JobKind::ReapStale => {
                crate::regions::repair_interrupted_downloads(&self.db).await?;
                Ok(())
            }
            JobKind::BackupDump => {
                let ctx = crate::backup::BackupContext {
                    database_url: self.database_url.clone(),
                    data_dir: self.data_dir.clone(),
                    config_path: self.config_path.clone(),
                };
                crate::backup::run(&self.db, &ctx).await
            }
            JobKind::RestoreProof => {
                crate::backup::run_restore_proof(&self.db, &self.database_url).await
            }
            JobKind::PurgeSessions => crate::maintenance::purge_sessions(&self.db).await,
            JobKind::CleanupDoneJobs => crate::maintenance::cleanup_done_jobs(&self.db).await,
            JobKind::CleanupTranscodeCache => {
                crate::maintenance::cleanup_transcode_cache(&self.data_dir)
            }
            JobKind::CleanupIdempotency => crate::maintenance::cleanup_idempotency(&self.db).await,
            JobKind::VacuumAnalyze => crate::maintenance::vacuum_analyze(&self.db).await,
            JobKind::IntegrityScrub => crate::maintenance::integrity_scrub(&self.db).await,
            JobKind::EmbedAssets => {
                let limit = crate::embed::limit_from_payload(&job.payload)?;
                let activity = Arc::clone(&self.activity);
                crate::embed::run(&self.db, &self.data_dir, limit, move || {
                    activity.analysis_should_run(Utc::now(), DEFAULT_ANALYSIS_IDLE_MS)
                })
                .await
                .map(|_| ())
            }
            JobKind::DetectFaces => {
                let limit = crate::detect_faces::limit_from_payload(&job.payload)?;
                let activity = Arc::clone(&self.activity);
                crate::detect_faces::run(&self.db, &self.data_dir, limit, move || {
                    activity.analysis_should_run(Utc::now(), DEFAULT_ANALYSIS_IDLE_MS)
                })
                .await
                .map(|_| ())
            }
            JobKind::BulkRename => crate::rename_batch::run(&self.db, &job.payload).await,
            JobKind::RenameUndo => crate::rename_undo::run(&self.db, &job.payload).await,
        }
    }
}

/// One handler per job type.
pub trait JobHandler: Send + Sync {
    fn ram_hint_bytes(&self, job: &Job) -> u64;

    fn handle(&self, job: &Job) -> impl std::future::Future<Output = Result<(), JobError>> + Send;
}

/// Default estimate: 64 MiB, enough for a decoded 20 MP JPEG.
pub const DEFAULT_RAM_HINT: u64 = 64 * 1024 * 1024;

#[must_use]
pub fn ram_hint_for_image(width: Option<i32>, height: Option<i32>) -> u64 {
    match (width, height) {
        (Some(w), Some(h)) if w > 0 && h > 0 => {
            u64::try_from(w)
                .unwrap_or(0)
                .saturating_mul(u64::try_from(h).unwrap_or(0))
                * 3
        }
        _ => DEFAULT_RAM_HINT,
    }
}
