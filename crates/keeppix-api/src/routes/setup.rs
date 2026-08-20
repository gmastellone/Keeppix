use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::IntoResponse;
use axum_extra::extract::CookieJar;
use keeppix_db::{SessionRepo, UserRepo};
use keeppix_domain::{NewUser, Password, SystemRole, Username, hash_password};
use serde::{Deserialize, Serialize};

use crate::cookie::session_cookie;
use crate::json::Json;
use crate::problem::Problem;
use crate::routes::auth::UserView;
use crate::state::AppState;

#[derive(Serialize, utoipa::ToSchema)]
pub struct SetupStatus {
    initialised: bool,
}

/// # Errors
/// `Problem` se il conteggio degli utenti fallisce.
#[utoipa::path(
    get,
    path = "/api/v1/setup/status",
    tag = "setup",
    operation_id = "setup_status",
    summary = "Tell whether the instance already has a bootstrap admin",
    responses(
        (status = 200, description = "Stato di inizializzazione dell'istanza", body = SetupStatus),
        (status = 500, description = "Il conteggio degli utenti è fallito", body = Problem)
    )
)]
pub async fn status(State(state): State<AppState>) -> Result<Json<SetupStatus>, Problem> {
    let count = UserRepo::new(&state.db).count().await?;
    Ok(Json(SetupStatus {
        initialised: count > 0,
    }))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct SetupRequest {
    username: String,
    display_name: String,
    email: Option<String>,
    password: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct SetupResponse {
    user: UserView,
}

/// Crea il primo amministratore e apre subito una sessione.
///
/// # Errors
/// `409 already-initialised` se l'istanza è già configurata;
/// `422 invalid-username` / `422 invalid-password` sui dati non validi.
#[utoipa::path(
    post,
    path = "/api/v1/setup",
    tag = "setup",
    operation_id = "setup_create",
    summary = "Create the bootstrap admin account and open a session",
    request_body = SetupRequest,
    responses(
        (status = 201, description = "Amministratore creato e sessione aperta", body = SetupResponse),
        (status = 400, description = "Corpo JSON sintatticamente non valido", body = Problem),
        (status = 409, description = "Istanza già inizializzata", body = Problem),
        (status = 415, description = "Content-Type diverso da application/json", body = Problem),
        (status = 422, description = "Username o password non validi, o corpo JSON di forma inattesa", body = Problem),
        (status = 500, description = "Hashing della password o scrittura fallita", body = Problem)
    )
)]
pub async fn create(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Json(req): Json<SetupRequest>,
) -> Result<impl IntoResponse, Problem> {
    let username = Username::parse(&req.username).map_err(|e| {
        Problem::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid-username",
            "Invalid username",
        )
        .with_detail(e.to_string())
    })?;
    let password = Password::parse_owned(req.password).map_err(|e| {
        Problem::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid-password",
            "Invalid password",
        )
        .with_detail(e.to_string())
    })?;
    let hash = hash_password(&password).map_err(|_| Problem::internal())?;

    let users = UserRepo::new(&state.db);
    let user = users
        .create_bootstrap_admin(NewUser {
            username,
            email: req.email,
            display_name: req.display_name,
            password_hash: hash.as_str().to_owned(),
            role: SystemRole::Admin,
        })
        .await
        .map_err(|e| match e {
            keeppix_db::DbError::Conflict(_) => Problem::new(
                StatusCode::CONFLICT,
                "already-initialised",
                "Instance is already initialised",
            ),
            other => Problem::from(other),
        })?;

    let token = SessionRepo::new(&state.db)
        .create(user.id, state.session_ttl, user_agent(&headers))
        .await?;

    let jar = jar.add(session_cookie(&token, state.session_ttl));

    Ok((
        StatusCode::CREATED,
        jar,
        Json(SetupResponse {
            user: UserView::from(&user),
        }),
    ))
}

pub(crate) fn user_agent(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
}
