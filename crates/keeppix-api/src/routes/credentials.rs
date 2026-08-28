//! App passwords: dedicated credentials for non-interactive clients
//! (`WebDAV`). See `keeppix_db::AppPasswordRepo` for the logic; this module
//! only handles the HTTP translation.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use keeppix_db::AppPasswordRepo;
use keeppix_domain::AppPasswordId;
use serde::{Deserialize, Serialize};

use crate::extract::Auth;
use crate::json::Json;
use crate::problem::Problem;
use crate::state::AppState;

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreateAppPasswordRequest {
    pub label: String,
}

/// Creation response: **the only one** where `secret` appears. No other
/// route ever returns it again — after this response, only the hash
/// remains.
#[derive(Serialize, utoipa::ToSchema)]
pub struct AppPasswordCreatedView {
    pub id: String,
    pub label: String,
    pub secret: String,
    pub created_at: DateTime<Utc>,
}

/// Public summary: never `secret`, never the hash.
#[derive(Serialize, utoipa::ToSchema)]
pub struct AppPasswordView {
    pub id: String,
    pub label: String,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
}

/// Creates an app password for the authenticated user. The plaintext
/// secret appears in this response and will never be retrievable again.
///
/// # Errors
/// `401` if not authenticated.
#[utoipa::path(
    post,
    path = "/api/v1/users/me/app-passwords",
    tag = "users",
    operation_id = "app_passwords_create",
    summary = "Create an app password",
    security(("session_cookie" = [])),
    request_body = CreateAppPasswordRequest,
    responses(
        (status = 201, description = "App password created; `secret` appears here only once", body = AppPasswordCreatedView),
        (status = 401, description = "Not authenticated", body = Problem)
    )
)]
pub async fn create(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Json(body): Json<CreateAppPasswordRequest>,
) -> Result<(StatusCode, Json<AppPasswordCreatedView>), Problem> {
    let (summary, secret) = AppPasswordRepo::new(&state.db)
        .create(&ctx, body.label)
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(AppPasswordCreatedView {
            id: summary.id.to_string(),
            label: summary.label,
            secret: secret.expose().to_owned(),
            created_at: summary.created_at,
        }),
    ))
}

/// List of the authenticated user's non-revoked app passwords. Never the
/// secret: only `id`, `label`, and the two dates.
///
/// # Errors
/// `401` if not authenticated.
#[utoipa::path(
    get,
    path = "/api/v1/users/me/app-passwords",
    tag = "users",
    operation_id = "app_passwords_list",
    summary = "List app passwords",
    security(("session_cookie" = [])),
    responses(
        (status = 200, description = "List of non-revoked app passwords", body = [AppPasswordView]),
        (status = 401, description = "Not authenticated", body = Problem)
    )
)]
pub async fn list(
    State(state): State<AppState>,
    Auth(ctx): Auth,
) -> Result<Json<Vec<AppPasswordView>>, Problem> {
    let summaries = AppPasswordRepo::new(&state.db).list(&ctx).await?;
    Ok(Json(
        summaries
            .into_iter()
            .map(|s| AppPasswordView {
                id: s.id.to_string(),
                label: s.label,
                created_at: s.created_at,
                last_used_at: s.last_used_at,
            })
            .collect(),
    ))
}

/// Immediate revocation: only your own, or an admin's. The id of another
/// user's app password responds `403`, **never** `404` — otherwise the
/// route becomes an existence oracle.
///
/// # Errors
/// `403` if the id belongs to another user (or does not exist, for a
/// non-admin).
#[utoipa::path(
    delete,
    path = "/api/v1/users/me/app-passwords/{id}",
    tag = "users",
    operation_id = "app_passwords_revoke",
    summary = "Revoke an app password",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "App password id")),
    responses(
        (status = 204, description = "App password revoked"),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "Not allowed", body = Problem)
    )
)]
pub async fn revoke(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Path(id): Path<AppPasswordId>,
) -> Result<StatusCode, Problem> {
    AppPasswordRepo::new(&state.db).revoke(&ctx, id).await?;
    Ok(StatusCode::NO_CONTENT)
}
