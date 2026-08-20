//! Sessioni attive del profilo (spec fase-10 §8.1): elenco, revoca singola,
//! revoca di tutte le altre. Nessun SQL: solo `SessionRepo`, qui solo la
//! traduzione HTTP.

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

/// La famiglia (== `SessionId`) del cookie di sessione già validato da
/// `Auth`. Fatta apposta con lo stesso schema di `refresh`/`logout`
/// (`crate::routes::auth`): legge il cookie a mano invece di passare da un
/// secondo extractor, perché qui serve il token stesso, non solo il
/// contesto che ne deriva.
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
/// `401` se non autenticato.
#[utoipa::path(
    get,
    path = "/api/v1/users/me/sessions",
    tag = "auth",
    operation_id = "sessions_list",
    summary = "List the current user's active sessions",
    security(("session_cookie" = [])),
    responses(
        (status = 200, description = "Sessioni attive, la chiamante marcata `current`", body = [SessionView]),
        (status = 401, description = "Non autenticato", body = Problem)
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

/// Revoca una sessione **diversa** dalla chiamante: per uscire dal proprio
/// dispositivo esiste `POST /auth/logout`, che pulisce anche il cookie. Un id
/// che appartiene a un altro utente risponde `403`, mai `404` (oracolo di
/// esistenza).
///
/// # Errors
/// `400` se `id` è la sessione corrente; `401` se non autenticato; `403` se
/// `id` non appartiene al chiamante.
#[utoipa::path(
    delete,
    path = "/api/v1/users/me/sessions/{id}",
    tag = "auth",
    operation_id = "sessions_revoke",
    summary = "Revoke one of the current user's other sessions",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Id di sessione (SessionView.id)")),
    responses(
        (status = 204, description = "Sessione revocata"),
        (status = 400, description = "`id` è la sessione corrente: usare /auth/logout", body = Problem),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "`id` non appartiene al chiamante", body = Problem)
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
    // Come `change_password`/`disable`: la cache in-process è indicizzata per
    // token, e la famiglia appena revocata non è quella che questa richiesta
    // conosce — non c'è un `drop_token` puntuale possibile, quindi si
    // svuota tutta la cache per non lasciare il token revocato valido fino
    // a 30s.
    state.sessions.clear();
    Ok(StatusCode::NO_CONTENT)
}

/// Revoca tutte le famiglie dell'utente tranne quella chiamante — "Esci da
/// tutti gli altri dispositivi".
///
/// # Errors
/// `401` se non autenticato.
#[utoipa::path(
    post,
    path = "/api/v1/users/me/sessions/revoke-others",
    tag = "auth",
    operation_id = "sessions_revoke_others",
    summary = "Revoke every session except the caller's",
    security(("session_cookie" = [])),
    responses(
        (status = 204, description = "Le altre sessioni sono state revocate"),
        (status = 401, description = "Non autenticato", body = Problem)
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
