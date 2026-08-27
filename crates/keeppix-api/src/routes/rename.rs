//! Rinomina con formula in blocco (Fase 9 Task 10): anteprima, applicazione
//! e annullamento su [`keeppix_db::RenameRepo`]. `undo` resta sincrona
//! dentro la richiesta (come `metadata::apply_batch`, `track_operation =
//! true` — debito dichiarato in `keeppix_db::rename`, non toccato qui).
//!
//! **`apply_batch` non lo è più dal 27 agosto**: gira in background via
//! `JobKind::BulkRename` (`keeppix-jobs::rename_batch`), stessa forma di
//! `LibraryScan` — questa rotta fa solo i controlli fallibili a monte
//! (batch/permesso/visibilità), crea l'`Operation` e accoda il job,
//! rispondendo `202` con `operation_id` subito. Progettazione originale:
//! sincrona perché "veloce, nessuna inferenza" — un lotto da migliaia di
//! foto su storage lento restava comunque un blocco di minuti senza modo di
//! annullarlo, il motivo del cambio.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use keeppix_db::{AssetRepo, JobRepo, OperationsRepo, PermissionRepo, RenameRepo};
use keeppix_domain::{
    AssetId, BatchId, FolderId, JobKind, JobPriority, OperationId, OperationKind,
};
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
/// documento `OpenAPI`). Usato solo da `undo_batch`, ancora sincrona —
/// `apply_batch` risponde con [`RenameAccepted`].
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct RenameOperationOutcome {
    #[schema(value_type = String)]
    pub operation_id: OperationId,
    pub outcome: BulkOutcome,
}

/// Risposta di `apply_batch` (dal 27 agosto, `202`): l'esito non è ancora
/// noto quando si risponde — il chiamante segue `operation_id` sul
/// `WebSocket` (`operation.progress`), stesso pattern di `ScanAccepted`
/// (`routes/libraries.rs`).
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct RenameAccepted {
    #[schema(value_type = String)]
    pub operation_id: OperationId,
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

// I controlli fallibili restano sincroni e a monte, esattamente come prima
// quando vivevano dentro RenameRepo::compute — solo spostati un livello più
// su, perché ora sono l'unica cosa che questa richiesta fa davvero: il
// lavoro vero e proprio (compute di nuovo, per davvero, con nomi freschi
// non quelli di questo momento) gira dentro keeppix-jobs::rename_batch, non
// qui.
/// # Errors
/// `400` se il lotto supera [`crate::batch::MAX_BATCH_ASSETS`]; `401` se non
/// autenticato; `403` se anche un solo asset non è visibile o modificabile.
#[utoipa::path(
    post,
    path = "/api/v1/assets/batch/rename",
    tag = "rename",
    operation_id = "rename_apply_batch",
    summary = "Start a bulk rename",
    security(("session_cookie" = [])),
    request_body = RenameBatchRequest,
    responses(
        (status = 202, description = "Accodata — segui operation_id su WebSocket (operation.progress)", body = RenameAccepted),
        (status = 400, description = "batch troppo grande", body = Problem),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Un asset non visibile o non modificabile", body = Problem)
    )
)]
pub async fn apply_batch(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Json(body): Json<RenameBatchRequest>,
) -> Result<(StatusCode, Json<RenameAccepted>), Problem> {
    crate::batch::reject_oversized_batch(&body.asset_ids)?;
    let actor_id = ctx.user_id().ok_or_else(Problem::unauthenticated)?;
    AssetRepo::new(&state.db)
        .assert_visible(&ctx, &body.asset_ids)
        .await?;
    PermissionRepo::new(&state.db)
        .assert_can_edit_assets(&ctx, &body.asset_ids)
        .await?;

    let operation = OperationsRepo::new(&state.db)
        .create(&ctx, OperationKind::BulkRename)
        .await?;

    JobRepo::new(&state.db)
        .enqueue(
            JobKind::BulkRename,
            serde_json::json!({
                "operation_id": operation.id.to_string(),
                "actor_id": actor_id.to_string(),
                "asset_ids": body.asset_ids.iter().map(AssetId::to_string).collect::<Vec<_>>(),
                "schema": body.schema,
            }),
            JobPriority::Background,
            None,
        )
        .await?;

    Ok((
        StatusCode::ACCEPTED,
        Json(RenameAccepted {
            operation_id: operation.id,
        }),
    ))
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
