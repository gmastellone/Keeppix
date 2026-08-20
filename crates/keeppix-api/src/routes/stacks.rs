//! Stack RAW+JPEG (spec §5): membri e scelta del primario.

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
/// `401` se non autenticato; `403` se l'asset non è visibile al chiamante.
#[utoipa::path(
    get,
    path = "/api/v1/assets/{id}/stack",
    tag = "trash",
    operation_id = "assets_stack_get",
    summary = "Get an asset stack",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Id dell'asset")),
    responses(
        (status = 200, description = "Membri dello stack", body = StackView),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Asset non visibile", body = Problem),
        (status = 500, description = "Errore del database", body = Problem)
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
/// `401` se non autenticato; `403` se l'asset non è visibile; `409` se
/// l'asset non è in uno stack.
#[utoipa::path(
    post,
    path = "/api/v1/assets/{id}/stack/primary",
    tag = "trash",
    operation_id = "assets_stack_set_primary",
    summary = "Set the primary asset in a stack",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Id dell'asset da promuovere")),
    responses(
        (status = 204, description = "Primario aggiornato"),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Asset non visibile", body = Problem),
        (status = 409, description = "Asset non in uno stack", body = Problem),
        (status = 500, description = "Errore del database", body = Problem)
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
