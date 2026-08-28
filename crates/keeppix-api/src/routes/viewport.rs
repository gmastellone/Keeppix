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

/// Promotes the `derive:{hash}` jobs of visible buckets to Visible priority.
///
/// # Errors
/// `401` if not authenticated.
#[utoipa::path(
    post,
    path = "/api/v1/viewport",
    tag = "timeline",
    operation_id = "viewport_promote",
    summary = "Promote viewport-visible derive jobs",
    security(("session_cookie" = [])),
    request_body = ViewportRequest,
    responses(
        (status = 204, description = "Visible jobs promoted"),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 500, description = "Database error", body = Problem)
    )
)]
pub async fn promote(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Json(body): Json<ViewportRequest>,
) -> Result<StatusCode, Problem> {
    // A view change is a view change even if there are no jobs to
    // promote — the automatic analysis pause must see it regardless, not
    // only when the body carries valid hashes.
    if let Some(hook) = &state.on_viewport_activity {
        hook();
    }
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
