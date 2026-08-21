//! `JobKind::EmbedAssets`: embeddings CLIP dalle miniature THUMB 240px.
//!
//! Orchestrazione: `keeppix-db` elenca i pending (escluso culling),
//! `keeppix-media` fa l'inferenza a lotto, poi si persiste. Gli originali
//! non vengono mai decodificati.

use std::path::Path;

use keeppix_db::{Db, EmbeddingRepo, PendingEmbedding};
use keeppix_media::{
    MODEL_VERSION, MobileClip, decode_to_rgb8, derivative_paths, first_complete_model_dir,
};

use crate::JobError;

/// Dimensione di lotto di default (ammortizza il carico del modello).
pub const DEFAULT_BATCH_SIZE: i64 = 16;

/// Esito di un giro di embedding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmbedOutcome {
    pub embedded: u32,
    pub skipped_missing_thumb: u32,
}

/// # Errors
/// Modello assente, fallimento inferenza, o errore database.
pub async fn run(db: &Db, data_dir: &Path, limit: i64) -> Result<EmbedOutcome, JobError> {
    let pending = EmbeddingRepo::new(db)
        .list_pending(MODEL_VERSION, limit)
        .await?;
    if pending.is_empty() {
        return Ok(EmbedOutcome {
            embedded: 0,
            skipped_missing_thumb: 0,
        });
    }

    let model_dir = first_complete_model_dir().ok_or_else(|| {
        JobError::Worker(format!(
            "MobileCLIP model missing (expected {MODEL_VERSION} under models/)"
        ))
    })?;

    // Carica il modello una volta per lotto; Drop a fine funzione libera la RAM.
    let mut clip = MobileClip::load(&model_dir).map_err(JobError::Worker)?;
    embed_pending(db, data_dir, &mut clip, &pending).await
}

async fn embed_pending(
    db: &Db,
    data_dir: &Path,
    clip: &mut MobileClip,
    pending: &[PendingEmbedding],
) -> Result<EmbedOutcome, JobError> {
    let mut prepared: Vec<(PendingEmbedding, Vec<f32>)> = Vec::new();
    let mut skipped_missing_thumb = 0_u32;

    for item in pending {
        match load_thumb_nchw(data_dir, &item.content_hash, clip) {
            Ok(nchw) => prepared.push((item.clone(), nchw)),
            Err(ThumbLoadError::Missing) => {
                skipped_missing_thumb = skipped_missing_thumb.saturating_add(1);
            }
            Err(ThumbLoadError::Other(msg)) => {
                tracing::warn!(
                    asset_id = %item.asset_id.as_uuid(),
                    error = %msg,
                    "skip asset: thumb unreadable for embedding"
                );
                skipped_missing_thumb = skipped_missing_thumb.saturating_add(1);
            }
        }
    }

    if prepared.is_empty() {
        return Ok(EmbedOutcome {
            embedded: 0,
            skipped_missing_thumb,
        });
    }

    let batch = prepared.len();
    let mut stacked = Vec::with_capacity(batch * prepared[0].1.len());
    for (_, nchw) in &prepared {
        stacked.extend_from_slice(nchw);
    }

    let embeddings = clip
        .embed_images_nchw_batch(&stacked, batch)
        .map_err(JobError::Worker)?;

    let repo = EmbeddingRepo::new(db);
    let mut embedded = 0_u32;
    for ((item, _), emb) in prepared.into_iter().zip(embeddings) {
        repo.upsert(item.asset_id, &emb, MODEL_VERSION).await?;
        embedded = embedded.saturating_add(1);
    }

    Ok(EmbedOutcome {
        embedded,
        skipped_missing_thumb,
    })
}

enum ThumbLoadError {
    Missing,
    Other(String),
}

fn load_thumb_nchw(
    data_dir: &Path,
    hash: &[u8; 32],
    clip: &MobileClip,
) -> Result<Vec<f32>, ThumbLoadError> {
    let (thumb, _) = derivative_paths(data_dir, hash);
    if !thumb.is_file() {
        return Err(ThumbLoadError::Missing);
    }
    let bytes = std::fs::read(&thumb).map_err(|e| ThumbLoadError::Other(e.to_string()))?;
    let (rgb, width, height) =
        decode_to_rgb8(&bytes).map_err(|e| ThumbLoadError::Other(e.to_string()))?;
    clip.rgb_to_nchw(&rgb, width, height)
        .map_err(ThumbLoadError::Other)
}

/// # Errors
/// `JobError::Worker` se `limit` non è un intero positivo quando presente.
pub fn limit_from_payload(payload: &serde_json::Value) -> Result<i64, JobError> {
    match payload.get("limit") {
        None | Some(serde_json::Value::Null) => Ok(DEFAULT_BATCH_SIZE),
        Some(v) => v
            .as_i64()
            .filter(|&n| n > 0)
            .ok_or_else(|| JobError::Worker("payload.limit must be a positive integer".to_owned())),
    }
}
