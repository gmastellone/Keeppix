//! Spostamento in blocco fra cartelle (Fase 11 Task 7, §13.3 campo 8,
//! "Sposta in cartella") — su [`keeppix_db::AssetRepo::move_to_folder`],
//! già scritta dalla Fase 9 Task 1 ma mai esposta da una rotta finora.
//! Stesso pattern di `routes::flags::batch_set`: un ciclo sequenziale per
//! asset, esito in [`BulkOutcome`] (riuscita parziale ammessa, spec §3) —
//! non l'involucro con `operation_id` di `routes::rename`, che serve al
//! progresso sul `WebSocket` di un batch tracciato: uno spostamento di
//! cartella non ha un'anteprima né un annullamento nel documento
//! funzionale (§13.3), a differenza della rinomina con formula.

use axum::extract::State;
use keeppix_db::AssetRepo;
use keeppix_domain::{AssetId, FolderId};
use serde::Deserialize;

use crate::bulk::BulkOutcome;
use crate::extract::Auth;
use crate::json::Json;
use crate::problem::Problem;
use crate::state::AppState;

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct BatchMoveRequest {
    #[schema(value_type = Vec<String>)]
    pub asset_ids: Vec<AssetId>,
    #[schema(value_type = String)]
    pub folder_id: FolderId,
}

/// # Errors
/// `400` se il lotto supera [`crate::batch::MAX_BATCH_ASSETS`]; `401` se non
/// autenticato.
#[utoipa::path(
    post,
    path = "/api/v1/assets/batch/move",
    tag = "timeline",
    operation_id = "assets_batch_move",
    summary = "Move multiple assets to a folder",
    security(("session_cookie" = [])),
    request_body = BatchMoveRequest,
    responses(
        (status = 200, description = "Esito per asset (riuscita parziale ammessa)", body = BulkOutcome),
        (status = 400, description = "batch troppo grande", body = Problem),
        (status = 401, description = "Non autenticato", body = Problem)
    )
)]
pub async fn batch_move(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Json(body): Json<BatchMoveRequest>,
) -> Result<Json<BulkOutcome>, Problem> {
    crate::batch::reject_oversized_batch(&body.asset_ids)?;
    let repo = AssetRepo::new(&state.db);
    let mut succeeded = Vec::new();
    let mut failed = Vec::new();
    for asset_id in &body.asset_ids {
        match repo.move_to_folder(&ctx, *asset_id, body.folder_id).await {
            Ok(_) => succeeded.push(*asset_id),
            Err(error) => failed.push((*asset_id, error)),
        }
    }
    Ok(Json(BulkOutcome::from_partition(succeeded, &failed, None)))
}
