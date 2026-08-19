//! Pulizia periodica delle sessioni di upload scadute in `.keeppix-tmp/`
//! (Task 2, Fase 5): `UploadSessionRepo::delete_expired` esisteva ma nessun
//! percorso di produzione la chiamava — un upload abbandonato a metà
//! sarebbe rimasto sul disco per sempre, oltre l'`expires_at` pensato per
//! liberarlo.

use chrono::Utc;
use keeppix_db::{Db, JobRepo, UploadSessionRepo};
use keeppix_domain::{JobKind, JobPriority};
use serde_json::json;

use crate::JobError;

/// # Errors
/// Database, o I/O sul temporaneo (in quel caso la riga resta, come
/// `cleanup_trash::run`).
pub async fn run(db: &Db) -> Result<(), JobError> {
    UploadSessionRepo::new(db)
        .delete_expired(Utc::now())
        .await?;
    Ok(())
}

/// Accoda un giro, deduplicato fra pending/running. All'avvio e poi
/// periodicamente dal binario: non dal job stesso, perché il `dedup_key`
/// collide con il job ancora `running`.
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
