//! `JobKind::DetectFaces`: `YuNet` detection + `SFace` embedding +
//! incremental batch grouping.
//!
//! Same pattern as `embed.rs`: `keeppix-db` lists the pending items
//! (excluding culled assets, excluding libraries with `faces_enabled =
//! false`), `keeppix-media` runs inference, then results are persisted.
//! Originals are never decoded — detection runs on the 240px thumbnail,
//! embedding on the 2048px preview (the 240px thumbnail may not carry
//! enough detail to tell two people apart, but it is enough to find
//! *where* the faces are).
//!
//! The ONNX session lives for the whole scan window, for the same reason as
//! in `embed.rs`. Each window opens a `FaceDetection` `Operation`: the
//! existing WS poll emits `operation.progress` with no new events needed.
//!
//! **Incremental grouping**: for each face with a computed embedding, the
//! nearest centroid among existing people is looked up. The thresholds are
//! not yet calibrated against real data (no `YuNet`/`SFace` weights were
//! downloadable in the sandbox this was built in):
//! `ASSIGN_SIMILARITY`/`PROPOSE_SIMILARITY` are reasonable starting points
//! for an `SFace` cosine similarity, not measured numbers. A person with at
//! least one recorded separation never receives a certain assignment: it
//! always goes to the proposal queue, even above `ASSIGN_SIMILARITY`.

use std::path::Path;

use keeppix_db::{
    Db, FaceRepo, NewDetectedFace, OperationsRepo, PendingFaceScan, PersonRepo, UserRepo,
};
use keeppix_domain::{AssetId, FaceId, JobKind, JobPriority, OperationId, OperationKind};
use keeppix_media::face::{FaceModels, MODEL_VERSION, first_complete_model_dir};
use keeppix_media::{decode_to_rgb8, derivative_paths};
use serde_json::json;

use crate::JobError;

/// Default batch size — same value and same reason as
/// `embed::DEFAULT_BATCH_SIZE`: the pause gate is evaluated between batches.
pub const DEFAULT_BATCH_SIZE: i64 = 16;

/// A face below this fraction of the bounding box's short side (relative to
/// the image) stays **deliberately unrecognized**: no embedding is
/// computed, so it never becomes a grouping candidate. A wrong attribution
/// costs more than a missed face.
const MIN_FACE_SIZE_REL: f32 = 0.03;

/// Cosine similarity above which a face is assigned with certainty to the
/// nearest centroid.
const ASSIGN_SIMILARITY: f32 = 0.62;
/// Cosine similarity above which, without reaching certainty, the face is
/// still proposed in the review queue instead of opening a new person.
const PROPOSE_SIMILARITY: f32 = 0.45;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DetectOutcome {
    pub assets_scanned: u32,
    pub faces_found: u32,
}

/// Enqueue a high-priority batch after ingest — same reason as
/// `embed::enqueue_after_ingest`.
///
/// # Errors
/// Database.
pub async fn enqueue_after_ingest(db: &Db) -> Result<(), JobError> {
    keeppix_db::JobRepo::new(db)
        .enqueue(
            JobKind::DetectFaces,
            json!({ "limit": DEFAULT_BATCH_SIZE }),
            JobPriority::High,
            Some("detect_faces:ingest"),
        )
        .await?;
    Ok(())
}

/// Backfill at `Background` priority. No-op if the weights are missing —
/// same reason as `embed::schedule_backfill`.
///
/// # Errors
/// Database.
pub async fn schedule_backfill(db: &Db) -> Result<(), JobError> {
    if first_complete_model_dir().is_none() {
        return Ok(());
    }
    keeppix_db::JobRepo::new(db)
        .enqueue_after(
            JobKind::DetectFaces,
            json!({ "limit": DEFAULT_BATCH_SIZE }),
            JobPriority::Background,
            Some("detect_faces:backfill"),
            chrono::Utc::now(),
        )
        .await?;
    Ok(())
}

/// How many orphaned faces (embedding computed, never grouped) are retried
/// each window. A small batch is enough: they only accumulate when
/// `group_face` fails partway through an earlier pass (the face insert
/// succeeds, the following pgvector comparison doesn't) — a rare event, not
/// a volume comparable to the main queue.
const STRAGGLER_BATCH_LIMIT: i64 = 200;

/// Processes the pending items in batches of `limit`, keeping the ONNX
/// session alive as long as `continue_window` stays true and there are
/// photos left — same contract as `embed::run`.
///
/// # Errors
/// Model missing, inference failure, or database error.
pub async fn run(
    db: &Db,
    data_dir: &Path,
    limit: i64,
    mut continue_window: impl FnMut() -> bool,
) -> Result<DetectOutcome, JobError> {
    let faces = FaceRepo::new(db);

    // Recovery: a face with a computed embedding but no person and no
    // proposal is the leftover of an incremental grouping that failed
    // halfway through an earlier pass — `insert_detected` succeeded,
    // `group_face` right after it did not. The asset that contained it is
    // already marked `scanned` (`process_pending` marks it regardless, so
    // the queue doesn't stall on an unreachable asset), so `list_pending_scan`
    // never sees it again: without this step it would stay orphaned forever.
    // The ONNX model isn't needed here, only the pgvector comparison that's
    // already available.
    let stragglers = regroup_stragglers(db, &faces, STRAGGLER_BATCH_LIMIT).await?;
    if stragglers > 0 {
        tracing::info!(
            stragglers,
            "detect_faces: regrouped stragglers from an earlier incomplete pass"
        );
    }

    let has_pending = !faces.list_pending_scan(MODEL_VERSION, 1).await?.is_empty();
    if !has_pending {
        return Ok(DetectOutcome::default());
    }

    let model_dir = first_complete_model_dir().ok_or_else(|| {
        JobError::Worker(format!(
            "face models missing (expected {MODEL_VERSION} under models/)"
        ))
    })?;

    let ops = OperationsRepo::new(db);
    let op_id = open_face_detection_operation(db, &ops, &faces).await?;

    let mut models = FaceModels::load(&model_dir).map_err(JobError::Worker)?;

    let mut total = DetectOutcome::default();
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

        let pending = faces.list_pending_scan(MODEL_VERSION, limit).await?;
        if pending.is_empty() {
            break;
        }

        let (outcome, scanned_ids) = process_pending(db, data_dir, &mut models, &pending).await?;
        total.assets_scanned = total.assets_scanned.saturating_add(outcome.assets_scanned);
        total.faces_found = total.faces_found.saturating_add(outcome.faces_found);

        if let Some(id) = op_id
            && !scanned_ids.is_empty()
        {
            ops.record_success_many(id, &scanned_ids).await?;
        }
        batches = batches.saturating_add(1);

        let full_batch = i64::try_from(pending.len()).unwrap_or(i64::MAX) >= limit;
        if !full_batch {
            break;
        }
        if !continue_window() {
            stopped_for_pause = true;
            break;
        }
    }

    drop(models);
    tracing::info!(
        batches,
        assets_scanned = total.assets_scanned,
        faces_found = total.faces_found,
        stopped_for_pause,
        stopped_for_cancel,
        "detect_faces window"
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

async fn open_face_detection_operation(
    db: &Db,
    ops: &OperationsRepo<'_>,
    faces: &FaceRepo<'_>,
) -> Result<Option<OperationId>, JobError> {
    let Some(owner) = UserRepo::new(db).first_admin_id().await? else {
        tracing::warn!("detect_faces window: no admin to own FaceDetection operation");
        return Ok(None);
    };
    let op = ops
        .create_for_owner(owner, OperationKind::FaceDetection)
        .await?;
    let pending_total = faces.count_pending_scan(MODEL_VERSION).await?;
    ops.set_total(op.id, Some(pending_total)).await?;
    ops.set_phase(op.id, "detecting").await?;
    Ok(Some(op.id))
}

async fn maybe_requeue_backfill(db: &Db) -> Result<(), JobError> {
    let more = FaceRepo::new(db)
        .list_pending_scan(MODEL_VERSION, 1)
        .await?;
    if more.is_empty() {
        return Ok(());
    }
    schedule_backfill(db).await
}

async fn process_pending(
    db: &Db,
    data_dir: &Path,
    models: &mut FaceModels,
    pending: &[PendingFaceScan],
) -> Result<(DetectOutcome, Vec<AssetId>), JobError> {
    let face_repo = FaceRepo::new(db);
    let person_repo = PersonRepo::new(db);
    let mut outcome = DetectOutcome::default();
    let mut scanned = Vec::with_capacity(pending.len());

    for item in pending {
        let faces_found = scan_one_asset(&face_repo, &person_repo, data_dir, models, item)
            .await
            .unwrap_or_else(|err| {
                tracing::warn!(
                    asset_id = %item.asset_id.as_uuid(),
                    error = %err,
                    "skip asset: face detection failed"
                );
                0
            });
        outcome.faces_found = outcome.faces_found.saturating_add(faces_found);
        face_repo.mark_scanned(item.asset_id, MODEL_VERSION).await?;
        scanned.push(item.asset_id);
        outcome.assets_scanned = outcome.assets_scanned.saturating_add(1);
    }

    Ok((outcome, scanned))
}

/// Detects and groups the faces of an asset. Any failure (missing
/// thumbnail, decode failure, inference failure) is recoverable by the
/// caller: the asset is still marked as scanned, so the queue doesn't stall
/// on an unreachable file — same spirit as `embed::ThumbLoadError::Missing`.
async fn scan_one_asset(
    face_repo: &FaceRepo<'_>,
    person_repo: &PersonRepo<'_>,
    data_dir: &Path,
    models: &mut FaceModels,
    item: &PendingFaceScan,
) -> Result<u32, String> {
    let (thumb_path, preview_path) = derivative_paths(data_dir, &item.content_hash);
    let thumb_bytes = std::fs::read(&thumb_path).map_err(|e| format!("read thumb: {e}"))?;
    let (thumb_rgb, tw, th) =
        decode_to_rgb8(&thumb_bytes).map_err(|e| format!("decode thumb: {e}"))?;

    let detections = models.detect(&thumb_rgb, tw, th)?;
    if detections.is_empty() {
        return Ok(0);
    }

    // The preview is only needed if at least one face is large enough to
    // deserve an embedding — this avoids decoding it for nothing.
    let needs_preview = detections
        .iter()
        .any(|d| d.bbox.w.min(d.bbox.h) >= MIN_FACE_SIZE_REL);
    let preview = if needs_preview {
        std::fs::read(&preview_path)
            .ok()
            .and_then(|bytes| decode_to_rgb8(&bytes).ok())
    } else {
        None
    };

    let mut found = 0_u32;
    for det in &detections {
        let too_small = det.bbox.w.min(det.bbox.h) < MIN_FACE_SIZE_REL;
        let embedding = if too_small {
            None
        } else if let Some((preview_rgb, pw, ph)) = &preview {
            models.embed_face(preview_rgb, *pw, *ph, det.landmarks).ok()
        } else {
            None
        };

        let face = face_repo
            .insert_detected(NewDetectedFace {
                asset_id: item.asset_id,
                bbox: det.bbox,
                landmarks: Some(json!(det.landmarks)),
                embedding: embedding.clone(),
                detect_score: det.score,
                quality: Some(det.bbox.w.min(det.bbox.h)),
                model_version: MODEL_VERSION.to_owned(),
            })
            .await
            .map_err(|e| e.to_string())?;
        found += 1;

        if let Some(embedding) = embedding {
            group_face(face_repo, person_repo, face.id, &embedding)
                .await
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(found)
}

/// Incremental grouping of a newly detected face: looks up the nearest
/// centroid, decides certain/proposed/new person.
///
/// Ruling: a person with at least one recorded separation never receives an
/// automatic **certain** assignment — always a proposal, even above
/// `ASSIGN_SIMILARITY`. Implementing a margin between the nearest and
/// second-nearest centroid would require a second pgvector comparison per
/// face; this rule is simpler and never causes a silent, wrong automatic
/// assignment.
async fn group_face(
    face_repo: &FaceRepo<'_>,
    person_repo: &PersonRepo<'_>,
    face_id: FaceId,
    embedding: &[f32],
) -> Result<(), keeppix_db::DbError> {
    match person_repo.nearest_centroid(embedding).await? {
        None => {
            let person = person_repo.create(None).await?;
            face_repo.auto_assign(face_id, person.id).await?;
        }
        Some((person_id, similarity)) => {
            let separated = person_repo.has_any_separation(person_id).await?;
            if separated {
                face_repo.propose(face_id, person_id, similarity).await?;
            } else if similarity >= ASSIGN_SIMILARITY {
                face_repo.auto_assign(face_id, person_id).await?;
            } else if similarity >= PROPOSE_SIMILARITY {
                face_repo.propose(face_id, person_id, similarity).await?;
            } else {
                let person = person_repo.create(None).await?;
                face_repo.auto_assign(face_id, person.id).await?;
            }
        }
    }
    Ok(())
}

/// Retries grouping the orphaned faces — see the comment in [`run`]. Does
/// not need `FaceModels`: the embedding already exists, only the pgvector
/// comparison from [`group_face`] is needed. A failure on a single face
/// does not stop the others: it stays orphaned for one more round, it does
/// not block the window.
async fn regroup_stragglers(db: &Db, faces: &FaceRepo<'_>, limit: i64) -> Result<u32, JobError> {
    let person_repo = PersonRepo::new(db);
    let stragglers = faces
        .list_unassigned_with_embedding(MODEL_VERSION, limit)
        .await?;
    let mut regrouped = 0_u32;
    for face in stragglers {
        let Some(embedding) = faces.embedding_of(face.id).await? else {
            continue;
        };
        match group_face(faces, &person_repo, face.id, &embedding).await {
            Ok(()) => regrouped = regrouped.saturating_add(1),
            Err(err) => tracing::warn!(
                face_id = %face.id.as_uuid(),
                error = %err,
                "straggler regroup failed, will retry next window"
            ),
        }
    }
    Ok(regrouped)
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
