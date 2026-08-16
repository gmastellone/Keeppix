use std::path::PathBuf;
use std::time::Duration;

use keeppix_db::Db;
use keeppix_domain::{Job, JobKind};

use crate::JobError;
use crate::derive as derive_job;
use crate::discover;
use crate::hash as hash_job;
use crate::metadata;
use crate::raw as raw_job;
use crate::xmp as xmp_job;

/// Handler unico della pipeline 1b.
#[derive(Clone)]
pub struct IngestHandler {
    pub db: Db,
    pub data_dir: PathBuf,
    pub stability_wait: Duration,
}

impl crate::JobHandler for IngestHandler {
    fn ram_hint_bytes(&self, job: &Job) -> u64 {
        match job.kind {
            JobKind::DeriveAsset => DEFAULT_RAM_HINT,
            // Il demosaic gira in un processo a parte con il proprio rlimit,
            // ma il gate deve comunque evitare di farne partire troppi in
            // parallelo sulla stessa macchina.
            JobKind::DeriveRaw => 512 * 1024 * 1024,
            // WriteSidecar scrive un piccolo file di testo per asset:
            // leggero come gli altri job di manutenzione, copre il default.
            _ => 8 * 1024 * 1024,
        }
    }

    async fn handle(&self, job: &Job) -> Result<(), JobError> {
        match job.kind {
            JobKind::DiscoverLibrary => {
                let id = discover::library_id_from_payload(&job.payload)?;
                // `stability_wait` sul handler è la soglia di età (settled_after),
                // non un sonno: in produzione è `PRODUCTION_SETTLED_AFTER`.
                discover::run(&self.db, id, self.stability_wait).await
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
            JobKind::ReapStale => {
                keeppix_db::JobRepo::new(&self.db)
                    .reap_stale(Duration::from_secs(600))
                    .await?;
                Ok(())
            }
        }
    }
}

/// Un handler per tipo di job. I tipi concreti arrivano dai task 6–10.
pub trait JobHandler: Send + Sync {
    fn ram_hint_bytes(&self, job: &Job) -> u64;

    fn handle(&self, job: &Job) -> impl std::future::Future<Output = Result<(), JobError>> + Send;
}

/// Stima di default: 64 MiB, abbastanza per un JPEG 20 MP decodificato.
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
