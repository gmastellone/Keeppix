//! Rinomina con formula in blocco (Fase 9 Task 10): anteprima, applicazione
//! e annullamento su [`keeppix_db::RenameRepo`] — sincrona dentro la
//! richiesta come `metadata::apply_batch`, non un job. `apply`/`undo` sono
//! chiamati con `track_operation = true`: questa rotta è il primo chiamante
//! reale di `OperationKind::BulkRename` (finora solo i test lo esercitavano),
//! quindi `operation_id` è sempre presente nella risposta, non opzionale —
//! il frontend lo usa per seguire l'avanzamento sul `WebSocket` degli stessi
//! eventi `operation.progress` di `LibraryScan`/`AiAnalysis`/`FaceDetection`.

use axum::extract::{Path, State};
use keeppix_db::RenameRepo;
use keeppix_domain::{AssetId, BatchId, FolderId, OperationId};
use serde::{Deserialize, Serialize};

use crate::bulk::BulkOutcome;
use crate::extract::Auth;
use crate::json::Json;
use crate::problem::Problem;
use crate::state::AppState;

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct RenameBatchRequest {
    #[schema(value_type = Vec<String>)]
    pub asset_ids: Vec<AssetId>,
    /// Formula con segnaposto (spec §62): `{data}`, `{fotocamera}`,
    /// `{obiettivo}`, `{luogo}`, `{titolo}`, `{n[:D]}`.
    pub schema: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct RenamePreviewItemView {
    #[schema(value_type = String)]
    pub asset_id: AssetId,
    #[schema(value_type = String)]
    pub folder_id: FolderId,
    pub current_name: String,
    pub new_name: String,
    pub collides: bool,
}

impl From<keeppix_db::RenamePreviewItem> for RenamePreviewItemView {
    fn from(item: keeppix_db::RenamePreviewItem) -> Self {
        Self {
            asset_id: item.asset_id,
            folder_id: item.folder_id,
            current_name: item.current_name,
            new_name: item.new_name,
            collides: item.collides,
        }
    }
}

/// Nidificato, non appiattito su `BulkOutcome` (sicurezza `utoipa`: un
/// `#[serde(flatten)]` su uno schema generato perde i nomi dei campi nel
/// documento `OpenAPI`).
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct RenameOperationOutcome {
    #[schema(value_type = String)]
    pub operation_id: OperationId,
    pub outcome: BulkOutcome,
}

/// # Errors
/// `400` se il lotto supera [`crate::batch::MAX_BATCH_ASSETS`]; `401` se non
/// autenticato; `403` se anche un solo asset non è visibile o modificabile.
#[utoipa::path(
    post,
    path = "/api/v1/assets/batch/rename/preview",
    tag = "rename",
    operation_id = "rename_preview",
    summary = "Preview a bulk rename",
    security(("session_cookie" = [])),
    request_body = RenameBatchRequest,
    responses(
        (status = 200, description = "Nomi calcolati, nulla scritto su disco o database", body = Vec<RenamePreviewItemView>),
        (status = 400, description = "batch troppo grande", body = Problem),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Un asset non visibile o non modificabile", body = Problem)
    )
)]
pub async fn preview(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Json(body): Json<RenameBatchRequest>,
) -> Result<Json<Vec<RenamePreviewItemView>>, Problem> {
    crate::batch::reject_oversized_batch(&body.asset_ids)?;
    let items = RenameRepo::new(&state.db)
        .preview(&ctx, &body.asset_ids, &body.schema)
        .await?;
    Ok(Json(items.into_iter().map(Into::into).collect()))
}

/// # Errors
/// `400` se il lotto supera [`crate::batch::MAX_BATCH_ASSETS`]; `401` se non
/// autenticato; `403` se anche un solo asset non è visibile o modificabile
/// (le collisioni, invece, finiscono in `outcome.failed` senza bloccare il
/// resto del lotto).
#[utoipa::path(
    post,
    path = "/api/v1/assets/batch/rename",
    tag = "rename",
    operation_id = "rename_apply_batch",
    summary = "Apply a bulk rename",
    security(("session_cookie" = [])),
    request_body = RenameBatchRequest,
    responses(
        (status = 200, description = "Esito per asset; batch_id annullabile sui riusciti", body = RenameOperationOutcome),
        (status = 400, description = "batch troppo grande", body = Problem),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Un asset non visibile o non modificabile", body = Problem)
    )
)]
pub async fn apply_batch(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Json(body): Json<RenameBatchRequest>,
) -> Result<Json<RenameOperationOutcome>, Problem> {
    crate::batch::reject_oversized_batch(&body.asset_ids)?;
    let outcome = RenameRepo::new(&state.db)
        .apply(&ctx, &body.asset_ids, &body.schema, true)
        .await?;
    let operation_id = outcome.operation_id.ok_or_else(|| {
        Problem::internal().with_detail("rename apply did not track an operation")
    })?;
    let succeeded = outcome.renamed.iter().map(|asset| asset.id).collect();
    Ok(Json(RenameOperationOutcome {
        operation_id,
        outcome: BulkOutcome::from_partition(succeeded, &outcome.failed, outcome.batch_id),
    }))
}

/// # Errors
/// `401` se non autenticato; `403` se il batch non appartiene al chiamante
/// (non admin). Un secondo annullamento sullo stesso batch è un no-op, non
/// un errore — la risposta torna con `outcome.succeeded` vuoto.
#[utoipa::path(
    post,
    path = "/api/v1/assets/batch/rename/{batch_id}/undo",
    tag = "rename",
    operation_id = "rename_undo_batch",
    summary = "Undo a bulk rename batch",
    security(("session_cookie" = [])),
    params(("batch_id" = String, Path, description = "Id del batch restituito da apply")),
    responses(
        (status = 200, description = "Esito per asset ripristinato", body = RenameOperationOutcome),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Batch non del chiamante", body = Problem)
    )
)]
pub async fn undo_batch(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Path(batch_id): Path<BatchId>,
) -> Result<Json<RenameOperationOutcome>, Problem> {
    let outcome = RenameRepo::new(&state.db)
        .undo(&ctx, batch_id, true)
        .await?;
    let operation_id = outcome
        .operation_id
        .ok_or_else(|| Problem::internal().with_detail("rename undo did not track an operation"))?;
    let succeeded = outcome.restored.iter().map(|asset| asset.id).collect();
    Ok(Json(RenameOperationOutcome {
        operation_id,
        outcome: BulkOutcome::from_partition(succeeded, &outcome.failed, None),
    }))
}
