use axum::extract::State;
use axum::http::StatusCode;
use keeppix_db::JobRepo;
use keeppix_domain::JobPriority;
use serde::Deserialize;

use crate::extract::Auth;
use crate::json::Json;
use crate::problem::Problem;
use crate::state::AppState;

#[derive(Deserialize, utoipa::ToSchema)]
pub struct ViewportRequest {
    hashes: Vec<String>,
}

/// Promuove i job `derive:{hash}` dei bucket visibili a priorità Visible.
///
/// # Errors
/// `401` se non autenticato.
#[utoipa::path(
    post,
    path = "/api/v1/viewport",
    tag = "timeline",
    operation_id = "viewport_promote",
    summary = "Promote viewport-visible derive jobs",
    security(("session_cookie" = [])),
    request_body = ViewportRequest,
    responses(
        (status = 204, description = "Job visibili promossi"),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 500, description = "Errore del database", body = Problem)
    )
)]
pub async fn promote(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Json(body): Json<ViewportRequest>,
) -> Result<StatusCode, Problem> {
    let keys: Vec<String> = body
        .hashes
        .into_iter()
        .filter(|h| h.len() == 64 && h.bytes().all(|b| b.is_ascii_hexdigit()))
        .take(200)
        .map(|h| format!("derive:{}", h.to_ascii_lowercase()))
        .collect();
    if !keys.is_empty() {
        JobRepo::new(&state.db)
            .promote(&ctx, &keys, JobPriority::Visible)
            .await?;
    }
    Ok(StatusCode::NO_CONTENT)
}
