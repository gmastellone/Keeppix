//! App-password: credenziali dedicate per client non interattivi (`WebDAV`,
//! Fase 5 Task 5+, spec fase-5-webdav-upload.md §2.1). Vedi
//! `keeppix_db::AppPasswordRepo` per la logica; qui solo la traduzione HTTP.

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

/// Risposta alla creazione: **l'unica** in cui `secret` compare. Nessun'altra
/// rotta lo restituisce di nuovo — dopo questa risposta, solo l'hash resta.
#[derive(Serialize, utoipa::ToSchema)]
pub struct AppPasswordCreatedView {
    pub id: String,
    pub label: String,
    pub secret: String,
    pub created_at: DateTime<Utc>,
}

/// Riepilogo pubblico: mai `secret`, mai l'hash.
#[derive(Serialize, utoipa::ToSchema)]
pub struct AppPasswordView {
    pub id: String,
    pub label: String,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
}

/// Crea un'app-password per l'utente autenticato. Il segreto in chiaro
/// appare in questa risposta e non sarà mai più recuperabile.
///
/// # Errors
/// `401` se non autenticato.
#[utoipa::path(
    post,
    path = "/api/v1/users/me/app-passwords",
    tag = "users",
    operation_id = "app_passwords_create",
    summary = "Create an app password",
    security(("session_cookie" = [])),
    request_body = CreateAppPasswordRequest,
    responses(
        (status = 201, description = "App-password creata; `secret` appare qui una sola volta", body = AppPasswordCreatedView),
        (status = 401, description = "Non autenticato", body = Problem)
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

/// Elenco delle app-password non revocate dell'utente autenticato. Mai il
/// segreto: solo `id`, `label` e le due date.
///
/// # Errors
/// `401` se non autenticato.
#[utoipa::path(
    get,
    path = "/api/v1/users/me/app-passwords",
    tag = "users",
    operation_id = "app_passwords_list",
    summary = "List app passwords",
    security(("session_cookie" = [])),
    responses(
        (status = 200, description = "Elenco delle app-password non revocate", body = [AppPasswordView]),
        (status = 401, description = "Non autenticato", body = Problem)
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

/// Revoca immediata: solo la propria, o un admin. L'id di un'app-password
/// altrui risponde `403`, **mai** `404` — altrimenti la rotta diventa un
/// oracolo di esistenza.
///
/// # Errors
/// `403` se l'id appartiene a un altro utente (o non esiste, per un
/// non-admin).
#[utoipa::path(
    delete,
    path = "/api/v1/users/me/app-passwords/{id}",
    tag = "users",
    operation_id = "app_passwords_revoke",
    summary = "Revoke an app password",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Id dell'app-password")),
    responses(
        (status = 204, description = "App-password revocata"),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Non consentito", body = Problem)
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
