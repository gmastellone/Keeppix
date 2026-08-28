//! RAW+JPEG stacks: members and choosing the primary.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use keeppix_db::StackRepo;
use keeppix_domain::AssetId;
use serde::Serialize;

use crate::extract::Auth;
use crate::json::Json;
use crate::problem::Problem;
use crate::routes::timeline::AssetView;
use crate::state::AppState;

#[derive(Serialize, utoipa::ToSchema)]
pub struct StackMemberView {
    #[serde(flatten)]
    pub asset: AssetView,
    pub is_primary: bool,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct StackView {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_asset_id: Option<String>,
    pub members: Vec<StackMemberView>,
}

/// # Errors
/// `401` if not authenticated; `403` if the asset is not visible to the
/// caller.
#[utoipa::path(
    get,
    path = "/api/v1/assets/{id}/stack",
    tag = "trash",
    operation_id = "assets_stack_get",
    summary = "Get an asset stack",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Asset id")),
    responses(
        (status = 200, description = "Stack members", body = StackView),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "Asset not visible", body = Problem),
        (status = 500, description = "Database error", body = Problem)
    )
)]
pub async fn get_members(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Path(id): Path<AssetId>,
) -> Result<Json<StackView>, Problem> {
    let details = StackRepo::new(&state.db).members(&ctx, id).await?;
    Ok(Json(match details {
        None => StackView {
            stack_id: None,
            primary_asset_id: None,
            members: vec![],
        },
        Some(stack) => StackView {
            stack_id: Some(stack.stack_id.to_string()),
            primary_asset_id: Some(stack.primary_asset_id.to_string()),
            members: stack
                .members
                .iter()
                .map(|m| StackMemberView {
                    asset: AssetView::from_asset(&m.asset),
                    is_primary: m.is_primary,
                })
                .collect(),
        },
    }))
}

/// # Errors
/// `401` if not authenticated; `403` if the asset is not visible; `409` if
/// the asset is not in a stack.
#[utoipa::path(
    post,
    path = "/api/v1/assets/{id}/stack/primary",
    tag = "trash",
    operation_id = "assets_stack_set_primary",
    summary = "Set the primary asset in a stack",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Id of the asset to promote")),
    responses(
        (status = 204, description = "Primary updated"),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "Asset not visible", body = Problem),
        (status = 409, description = "Asset not in a stack", body = Problem),
        (status = 500, description = "Database error", body = Problem)
    )
)]
pub async fn set_primary(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Path(id): Path<AssetId>,
) -> Result<StatusCode, Problem> {
    StackRepo::new(&state.db).set_primary(&ctx, id).await?;
    Ok(StatusCode::NO_CONTENT)
}
