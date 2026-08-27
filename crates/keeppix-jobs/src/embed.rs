//! `JobKind::EmbedAssets`: CLIP embeddings from the 240px THUMB thumbnails.
//!
//! Orchestration: `keeppix-db` lists the pending items (excluding culled
//! assets), `keeppix-media` runs inference in batches, then results are
//! persisted. Originals are never decoded.
//!
//! The ONNX session lives for the **whole analysis window**: it loads once,
//! processes batches of `DEFAULT_BATCH_SIZE` (16) checking the pause gate
//! **between** batches, and gets dropped when the pause kicks in or the
//! queue empties out. Raising `DEFAULT_BATCH_SIZE` is not a substitute: the
//! gate is evaluated between small jobs/batches, otherwise the CPU would
//! stay pinned after the user resumes browsing.
//!
//! Each window opens an `AiAnalysis` `Operation`: the existing WS poll
//! already emits `operation.progress` with no new events needed.

use std::path::Path;

use chrono::Utc;
use keeppix_db::{
    AssetTagRepo, Db, EmbeddingRepo, JobRepo, OperationsRepo, PendingEmbedding, UserRepo,
};
use keeppix_domain::{AssetId, JobKind, JobPriority, OperationId, OperationKind};
use keeppix_media::{
    current_rss_bytes, decode_to_rgb8, derivative_paths, measure_rss_peak_during,
    openclip_xlmr::{self, MODEL_VERSION, OpenClipXlmr},
};
use serde_json::json;

use crate::JobError;

/// Default batch size. Deliberately small: the pause gate is evaluated
/// between batches (~0.7 s at 45 ms/photo), not after tens of seconds of
/// saturated CPU.
pub const DEFAULT_BATCH_SIZE: i64 = 16;

/// Hard RSS ceiling during analysis. Exceeding it is a warning, not a batch
/// abort: the model was already chosen to stay under this ceiling.
pub const RSS_HARD_CEILING_BYTES: u64 = 1024 * 1024 * 1024;

/// Outcome of an embedding pass (one window: zero or more batches).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmbedOutcome {
    pub embedded: u32,
    pub skipped_missing_thumb: u32,
}

/// Enqueue a high-priority batch after ingest (new photos). Deduplicated
/// while pending/running: many consecutive derives share a single job.
///
/// # Errors
/// Database.
pub async fn enqueue_after_ingest(db: &Db) -> Result<(), JobError> {
    JobRepo::new(db)
        .enqueue(
            JobKind::EmbedAssets,
            json!({ "limit": DEFAULT_BATCH_SIZE }),
            JobPriority::High,
            Some("embed_assets:ingest"),
        )
        .await?;
    Ok(())
}

/// Enqueue the backfill at `Background` priority (it stops on its own when
/// the gallery is in use — `EnergyProfile` plus the viewport pause). No-op
/// if the CLIP weights aren't on disk: otherwise the queue fills with jobs
/// that fail and go into retry.
///
/// # Errors
/// Database.
pub async fn schedule_backfill(db: &Db) -> Result<(), JobError> {
    if openclip_xlmr::first_complete_model_dir().is_none() {
        return Ok(());
    }
    JobRepo::new(db)
        .enqueue_after(
            JobKind::EmbedAssets,
            json!({ "limit": DEFAULT_BATCH_SIZE }),
            JobPriority::Background,
            Some("embed_assets:backfill"),
            Utc::now(),
        )
        .await?;
    Ok(())
}

/// Processes the pending items in batches of `limit`, keeping the ONNX
/// session alive as long as `continue_window` stays true and there are
/// photos left. `continue_window` is evaluated **between** batches (not
/// during inference). On pause or an empty queue the session is dropped;
/// the backfill is only re-enqueued if items remain pending after a pause.
///
/// # Errors
/// Model missing, inference failure, or database error.
pub async fn run(
    db: &Db,
    data_dir: &Path,
    limit: i64,
    mut continue_window: impl FnMut() -> bool,
) -> Result<EmbedOutcome, JobError> {
    let embeddings = EmbeddingRepo::new(db);
    let has_pending = !embeddings.list_pending(MODEL_VERSION, 1).await?.is_empty();
    if !has_pending {
        return Ok(EmbedOutcome {
            embedded: 0,
            skipped_missing_thumb: 0,
        });
    }

    let model_dir = openclip_xlmr::first_complete_model_dir().ok_or_else(|| {
        JobError::Worker(format!(
            "OpenCLIP model missing (expected {MODEL_VERSION} under models/)"
        ))
    })?;

    let ops = OperationsRepo::new(db);
    let op_id = open_ai_analysis_operation(db, &ops, &embeddings).await?;

    // Just one load per window. Dropped at end of scope. VmRSS after drop
    // typically stays roughly equal to after_load: the ORT allocator holds
    // onto pages for the life of the process — it doesn't grow between
    // batches, stays under the 1 GiB ceiling, and that's accepted.
    let rss_before = current_rss_bytes();
    let (load_result, rss_after_load) = measure_rss_peak_during(|| OpenClipXlmr::load(&model_dir));
    let mut clip = load_result.map_err(JobError::Worker)?;

    let mut total = EmbedOutcome {
        embedded: 0,
        skipped_missing_thumb: 0,
    };
    let mut rss_peak_infer = rss_after_load;
    let mut stopped_for_pause = false;
    let mut stopped_for_cancel = false;
    let mut batches = 0_u32;

    loop {
        if let Some(id) = op_id
            && ops.is_cancel_requested(id).await?
        {
            stopped_for_cancel = true;
            break;
        }

        let pending = embeddings.list_pending(MODEL_VERSION, limit).await?;
        if pending.is_empty() {
            break;
        }

        let (outcome, peak, embedded_ids) =
            embed_pending(db, data_dir, &mut clip, &pending, rss_after_load).await?;
        total.embedded = total.embedded.saturating_add(outcome.embedded);
        total.skipped_missing_thumb = total
            .skipped_missing_thumb
            .saturating_add(outcome.skipped_missing_thumb);
        if let Some(p) = peak {
            rss_peak_infer = Some(rss_peak_infer.map_or(p, |cur| cur.max(p)));
        }
        if let Some(id) = op_id
            && !embedded_ids.is_empty()
        {
            ops.record_success_many(id, &embedded_ids).await?;
        }
        batches = batches.saturating_add(1);

        let full_batch = i64::try_from(pending.len()).unwrap_or(i64::MAX) >= limit;
        if !full_batch {
            break;
        }
        // Gate between batches: the user resumed browsing → drop and re-enqueue.
        if !continue_window() {
            stopped_for_pause = true;
            break;
        }
    }

    drop(clip);
    let rss_after_drop = current_rss_bytes();
    tracing::info!(
        batches,
        embedded = total.embedded,
        stopped_for_pause,
        stopped_for_cancel,
        rss_before_bytes = rss_before,
        rss_after_load_bytes = rss_after_load,
        rss_peak_infer_bytes = rss_peak_infer,
        rss_after_drop_bytes = rss_after_drop,
        "embed window rss"
    );

    if let Some(id) = op_id {
        if stopped_for_cancel {
            ops.finish_cancelled(id).await?;
        } else {
            ops.finish_done(id).await?;
        }
    }

    if stopped_for_pause || stopped_for_cancel {
        maybe_requeue_backfill(db).await?;
    }
    Ok(total)
}

async fn open_ai_analysis_operation(
    db: &Db,
    ops: &OperationsRepo<'_>,
    embeddings: &EmbeddingRepo<'_>,
) -> Result<Option<OperationId>, JobError> {
    let Some(owner) = UserRepo::new(db).first_admin_id().await? else {
        tracing::warn!("embed window: no admin to own AiAnalysis operation");
        return Ok(None);
    };
    let op = ops
        .create_for_owner(owner, OperationKind::AiAnalysis)
        .await?;
    let pending_total = embeddings.count_pending(MODEL_VERSION).await?;
    ops.set_total(op.id, Some(pending_total)).await?;
    ops.set_phase(op.id, "embedding").await?;
    Ok(Some(op.id))
}

async fn maybe_requeue_backfill(db: &Db) -> Result<(), JobError> {
    let more = EmbeddingRepo::new(db)
        .list_pending(MODEL_VERSION, 1)
        .await?;
    if more.is_empty() {
        return Ok(());
    }
    schedule_backfill(db).await
}

async fn embed_pending(
    db: &Db,
    data_dir: &Path,
    clip: &mut OpenClipXlmr,
    pending: &[PendingEmbedding],
    rss_after_load: Option<u64>,
) -> Result<(EmbedOutcome, Option<u64>, Vec<AssetId>), JobError> {
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
        return Ok((
            EmbedOutcome {
                embedded: 0,
                skipped_missing_thumb,
            },
            rss_after_load,
            Vec::new(),
        ));
    }

    // `OpenClipXlmr` doesn't (yet) have an ONNX-level batch method the way
    // `MobileClip::embed_images_nchw_batch` did — one `session.run()` per
    // image, not a single call for the whole batch. This is still correct
    // (each image is independent, no cross-talk between examples in a
    // ViT/BatchNorm-free model), just more ort calls — the visual tower is
    // still ~3x faster per image than MobileCLIP2 (confirmed on real
    // benchmarks), so the batch's net cost stays lower even without
    // ONNX-level batching.
    let (infer_result, rss_peak_infer) = measure_rss_peak_during(|| {
        prepared
            .iter()
            .map(|(_, nchw)| clip.embed_image_nchw(nchw))
            .collect::<Result<Vec<Vec<f32>>, String>>()
    });
    let embeddings = infer_result.map_err(JobError::Worker)?;

    if let Some(peak) = rss_peak_infer
        && peak > RSS_HARD_CEILING_BYTES
    {
        tracing::warn!(
            peak_bytes = peak,
            ceiling_bytes = RSS_HARD_CEILING_BYTES,
            "embed RSS exceeded hard ceiling"
        );
    }

    let repo = EmbeddingRepo::new(db);
    let mut embedded = 0_u32;
    let mut embedded_ids = Vec::with_capacity(prepared.len());
    for ((item, _), emb) in prepared.into_iter().zip(embeddings) {
        repo.upsert(item.asset_id, &emb, MODEL_VERSION).await?;
        embedded_ids.push(item.asset_id);
        embedded = embedded.saturating_add(1);
    }

    if !embedded_ids.is_empty() {
        AssetTagRepo::new(db)
            .propose_for_assets(&embedded_ids)
            .await?;
    }

    Ok((
        EmbedOutcome {
            embedded,
            skipped_missing_thumb,
        },
        rss_peak_infer,
        embedded_ids,
    ))
}

enum ThumbLoadError {
    Missing,
    Other(String),
}

fn load_thumb_nchw(
    data_dir: &Path,
    hash: &[u8; 32],
    clip: &OpenClipXlmr,
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
/// `JobError::Worker` if `limit` is present but not a positive integer.
pub fn limit_from_payload(payload: &serde_json::Value) -> Result<i64, JobError> {
    match payload.get("limit") {
        None | Some(serde_json::Value::Null) => Ok(DEFAULT_BATCH_SIZE),
        Some(v) => v
            .as_i64()
            .filter(|&n| n > 0)
            .ok_or_else(|| JobError::Worker("payload.limit must be a positive integer".to_owned())),
    }
}
