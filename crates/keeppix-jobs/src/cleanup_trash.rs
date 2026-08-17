//! Potatura periodica del cestino. `TrashRepo::cleanup_expired` esisteva
//! ed era coperta da test, ma nessun percorso di produzione la chiamava:
//! le foto cancellate restavano su disco per sempre.

use chrono::{Duration, Utc};
use keeppix_db::{Db, JobRepo, TrashRepo};
use keeppix_domain::{JobKind, JobPriority};
use serde_json::json;

use crate::JobError;

/// # Errors
/// Database, o I/O sul file nel cestino (in quel caso la riga resta).
pub async fn run(db: &Db, retention_days: i64) -> Result<(), JobError> {
    let days = retention_days.max(1);
    let before = Utc::now() - Duration::days(days);
    TrashRepo::new(db).cleanup_expired(before).await?;
    Ok(())
}

/// Accoda un giro, deduplicato fra pending/running. All'avvio e poi ogni
/// 24 h dal binario: non dal job stesso, perché il `dedup_key` collide con
/// il job ancora `running`.
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
