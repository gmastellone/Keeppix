//! `JobKind::DetectFaces`: rilevamento SCRFD + impronta `ArcFace` +
//! raggruppamento incrementale a lotto (Fase 8 Task 4/5).
//!
//! Stesso pattern di `embed.rs`: `keeppix-db` elenca i pending (escluso
//! culling, escluse le librerie con `faces_enabled = false`),
//! `keeppix-media` fa l'inferenza, poi si persiste. Gli originali non
//! vengono mai decodificati — rilevamento sulla miniatura 240px, impronta
//! sulla preview 2048px (spec §2.1 emendamento: i 240px della miniatura
//! possono non bastare a distinguere due persone, ma bastano a trovare
//! *dove* sono i volti).
//!
//! La sessione ONNX vive per l'intera finestra di analisi, stesso motivo di
//! `embed.rs`. Ogni finestra apre un'`Operation` `FaceDetection`: il poll WS
//! esistente emette `operation.progress` senza eventi nuovi.
//!
//! **Raggruppamento incrementale** (Task 5, spec §4.1): per ogni volto con
//! impronta calcolata, si cerca il centroide più vicino fra le persone
//! esistenti. Soglie non ancora calibrate su dati reali (nessun peso
//! SCRFD/ArcFace scaricabile in questa sandbox — vedi ledger di fase):
//! `ASSIGN_SIMILARITY`/`PROPOSE_SIMILARITY` sono punti di partenza
//! ragionevoli per una similarità coseno `ArcFace`, non numeri misurati. Una
//! persona con almeno una separazione registrata (spec §4.3) non riceve mai
//! un'assegnazione certa: va sempre in proposta, anche sopra
//! `ASSIGN_SIMILARITY` — vedi il Ruling nel ledger.

use std::path::Path;

use keeppix_db::{
    Db, FaceRepo, NewDetectedFace, OperationsRepo, PendingFaceScan, PersonRepo, UserRepo,
};
use keeppix_domain::{AssetId, FaceId, JobKind, JobPriority, OperationId, OperationKind};
use keeppix_media::face::{FaceModels, MODEL_VERSION, first_complete_model_dir};
use keeppix_media::{decode_to_rgb8, derivative_paths};
use serde_json::json;

use crate::JobError;

/// Dimensione di lotto di default — stesso valore e stessa ragione di
/// `embed::DEFAULT_BATCH_SIZE`: il gate di pausa si valuta fra un lotto e
/// l'altro.
pub const DEFAULT_BATCH_SIZE: i64 = 16;

/// Un volto sotto questa frazione del lato corto del riquadro (relativo
/// all'immagine) resta **non riconosciuto di proposito** (spec Task 4 punto
/// 4): niente impronta calcolata, quindi non diventa mai candidato al
/// raggruppamento. Un'attribuzione sbagliata costa più di un volto mancato.
const MIN_FACE_SIZE_REL: f32 = 0.03;

/// Similarità coseno oltre la quale un volto è assegnato con certezza al
/// centroide più vicino.
const ASSIGN_SIMILARITY: f32 = 0.62;
/// Similarità coseno oltre la quale, senza raggiungere la certezza, il volto
/// va comunque proposto in coda di revisione invece di aprire una persona
/// nuova.
const PROPOSE_SIMILARITY: f32 = 0.45;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DetectOutcome {
    pub assets_scanned: u32,
    pub faces_found: u32,
}

/// Accoda un lotto ad alta priorità dopo l'ingest — stesso motivo di
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

/// Backfill a priorità `Background`. No-op se i pesi mancano — stesso
/// motivo di `embed::schedule_backfill`.
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

/// Quanti volti orfani (impronta calcolata, mai raggruppati) si ritenta a
/// ogni finestra. Basta un lotto piccolo: se ne accumulano solo quando
/// `group_face` fallisce a metà di una passata precedente (un volto già
/// inserito, il confronto pgvector che segue no) — un evento raro, non un
/// volume paragonabile alla coda principale.
const STRAGGLER_BATCH_LIMIT: i64 = 200;

/// Processa i pending a lotti di `limit`, tenendo la sessione ONNX viva
/// finché `continue_window` resta vera e ci sono foto — stesso contratto di
/// `embed::run`.
///
/// # Errors
/// Modello assente, fallimento inferenza, o errore database.
pub async fn run(
    db: &Db,
    data_dir: &Path,
    limit: i64,
    mut continue_window: impl FnMut() -> bool,
) -> Result<DetectOutcome, JobError> {
    let faces = FaceRepo::new(db);

    // Recupero: un volto con impronta calcolata ma senza persona né proposta
    // è lo scarto di un raggruppamento incrementale fallito a metà in una
    // passata precedente — `insert_detected` è riuscito, `group_face` subito
    // dopo no. L'asset che lo conteneva è già segnato `scanned`
    // (`process_pending` lo marca comunque, per non bloccare la coda su un
    // asset irraggiungibile), quindi `list_pending_scan` non lo rivede mai
    // più: senza questo passo resterebbe orfano per sempre. Non serve il
    // modello ONNX qui: solo il confronto pgvector già pronto.
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

/// Rileva e raggruppa i volti di un asset. Qualsiasi fallimento (miniatura
/// assente, decodifica fallita, inferenza fallita) è recuperabile dal
/// chiamante: l'asset viene comunque segnato come analizzato, per non
/// bloccare la coda su un file irraggiungibile — stesso spirito di
/// `embed::ThumbLoadError::Missing`.
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

    // La preview serve solo se almeno un volto è abbastanza grande da
    // meritare un'impronta — evita di decodificarla per niente.
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

/// Raggruppamento incrementale di un volto appena rilevato (Task 5, spec
/// §4.1): cerca il centroide più vicino, decide certo/proposto/nuova
/// persona.
///
/// Ruling (ledger di fase): una persona con almeno una separazione
/// registrata non riceve mai un'assegnazione automatica **certa** — sempre
/// proposta, anche sopra `ASSIGN_SIMILARITY`. Implementare un margine fra il
/// primo e il secondo centroide più vicino richiederebbe un secondo
/// confronto pgvector per ogni volto; questa regola è più semplice e non
/// causa mai un'assegnazione automatica silenziosa sbagliata.
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

/// Ritenta il raggruppamento dei volti orfani — vedi il commento in
/// [`run`]. Non richiede `FaceModels`: l'impronta esiste già, serve solo il
/// confronto pgvector di [`group_face`]. Un fallimento su un singolo volto
/// non interrompe gli altri: resta orfano un altro giro, non blocca la
/// finestra.
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
