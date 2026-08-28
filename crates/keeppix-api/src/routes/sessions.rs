//! Active profile sessions: listing, single revocation, revoking all
//! others. No SQL: only `SessionRepo`, this module only handles the HTTP
//! translation.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum_extra::extract::CookieJar;
use chrono::{DateTime, Utc};
use keeppix_db::SessionRepo;
use keeppix_domain::{SessionId, SessionToken};
use serde::Serialize;

use crate::extract::{Auth, SESSION_COOKIE};
use crate::json::Json;
use crate::problem::Problem;
use crate::state::AppState;

#[derive(Serialize, utoipa::ToSchema)]
pub struct SessionView {
    pub id: String,
    pub device_label: Option<String>,
    pub last_seen_at: DateTime<Utc>,
    pub current: bool,
}

/// The family (== `SessionId`) of the session cookie already validated by
/// `Auth`. Deliberately built with the same schema as `refresh`/`logout`
/// (`crate::routes::auth`): it reads the cookie by hand instead of going
/// through a second extractor, because here the token itself is needed,
/// not just the context derived from it.
async fn current_family(state: &AppState, jar: &CookieJar) -> Result<SessionId, Problem> {
    let cookie = jar
        .get(SESSION_COOKIE)
        .ok_or_else(Problem::unauthenticated)?;
    let token = SessionToken::from_string(cookie.value().to_owned());
    SessionRepo::new(&state.db)
        .family_of(&token)
        .await
        .map_err(crate::extract::session_problem)?
        .ok_or_else(Problem::unauthenticated)
}

/// # Errors
/// `401` if not authenticated.
#[utoipa::path(
    get,
    path = "/api/v1/users/me/sessions",
    tag = "auth",
    operation_id = "sessions_list",
    summary = "List the current user's active sessions",
    security(("session_cookie" = [])),
    responses(
        (status = 200, description = "Active sessions, the caller's marked `current`", body = [SessionView]),
        (status = 401, description = "Not authenticated", body = Problem)
    )
)]
pub async fn list(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    jar: CookieJar,
) -> Result<Json<Vec<SessionView>>, Problem> {
    let current = current_family(&state, &jar).await?;
    let sessions = SessionRepo::new(&state.db)
        .list_active(&ctx, current)
        .await?;
    Ok(Json(
        sessions
            .into_iter()
            .map(|s| SessionView {
                id: s.id.to_string(),
                device_label: s.device_label,
                last_seen_at: s.last_seen_at,
                current: s.current,
            })
            .collect(),
    ))
}

/// Revokes a session **different** from the caller's: to log out of your
/// own device, use `POST /auth/logout`, which also clears the cookie. An
/// id belonging to another user responds `403`, never `404` (existence
/// oracle).
///
/// # Errors
/// `400` if `id` is the current session; `401` if not authenticated; `403`
/// if `id` does not belong to the caller.
#[utoipa::path(
    delete,
    path = "/api/v1/users/me/sessions/{id}",
    tag = "auth",
    operation_id = "sessions_revoke",
    summary = "Revoke one of the current user's other sessions",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Session id (SessionView.id)")),
    responses(
        (status = 204, description = "Session revoked"),
        (status = 400, description = "`id` is the current session: use /auth/logout instead", body = Problem),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "`id` does not belong to the caller", body = Problem)
    )
)]
pub async fn revoke(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    jar: CookieJar,
    Path(id): Path<SessionId>,
) -> Result<StatusCode, Problem> {
    let current = current_family(&state, &jar).await?;
    if id == current {
        return Err(Problem::bad_request(
            "session-is-current",
            "Cannot revoke the current session; use POST /auth/logout instead",
        ));
    }
    SessionRepo::new(&state.db).revoke_family(&ctx, id).await?;
    // Like `change_password`/`disable`: the in-process cache is indexed by
    // token, and the family just revoked is not the one this request
    // knows — a targeted `drop_token` isn't possible, so the whole cache
    // is cleared to avoid leaving the revoked token valid for up to 30s.
    state.sessions.clear();
    Ok(StatusCode::NO_CONTENT)
}

/// Revokes every family of the user except the caller's — "Log out of all
/// other devices".
///
/// # Errors
/// `401` if not authenticated.
#[utoipa::path(
    post,
    path = "/api/v1/users/me/sessions/revoke-others",
    tag = "auth",
    operation_id = "sessions_revoke_others",
    summary = "Revoke every session except the caller's",
    security(("session_cookie" = [])),
    responses(
        (status = 204, description = "The other sessions have been revoked"),
        (status = 401, description = "Not authenticated", body = Problem)
    )
)]
pub async fn revoke_others(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    jar: CookieJar,
) -> Result<StatusCode, Problem> {
    let cookie = jar
        .get(SESSION_COOKIE)
        .ok_or_else(Problem::unauthenticated)?;
    let token = SessionToken::from_string(cookie.value().to_owned());
    let user_id = ctx.user_id().ok_or_else(Problem::unauthenticated)?;

    SessionRepo::new(&state.db)
        .revoke_other_families(user_id, &token)
        .await?;
    state.sessions.clear();
    Ok(StatusCode::NO_CONTENT)
}
