//! Bulk move between folders ("Move to folder") — on top of
//! [`keeppix_db::AssetRepo::move_to_folder`], already implemented but never
//! exposed by a route until now. Same pattern as
//! `routes::flags::batch_set`: a sequential loop per asset, outcome in
//! [`BulkOutcome`] (partial success allowed) — not the `operation_id`
//! wrapper used by `routes::rename`, which serves `WebSocket` progress for
//! a tracked batch: a folder move has no preview or undo in the functional
//! spec, unlike formula-based rename.

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
/// `400` if the batch exceeds [`crate::batch::MAX_BATCH_ASSETS`]; `401` if
/// not authenticated.
#[utoipa::path(
    post,
    path = "/api/v1/assets/batch/move",
    tag = "timeline",
    operation_id = "assets_batch_move",
    summary = "Move multiple assets to a folder",
    security(("session_cookie" = [])),
    request_body = BatchMoveRequest,
    responses(
        (status = 200, description = "Per-asset outcome (partial success allowed)", body = BulkOutcome),
        (status = 400, description = "Batch too large", body = Problem),
        (status = 401, description = "Not authenticated", body = Problem)
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
