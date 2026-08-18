//! Gestione utenti. Nessun SQL: solo `UserRepo` / `SessionRepo`.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum_extra::extract::CookieJar;
use keeppix_db::{SessionRepo, UserRepo};
use keeppix_domain::{
    NewUser, Password, SessionToken, SystemRole, UserId, Username, hash_password, verify_password,
};
use serde::Deserialize;

use crate::extract::{AdminAuth, Auth, SESSION_COOKIE};
use crate::json::Json;
use crate::problem::Problem;
use crate::routes::auth::UserView;
use crate::state::AppState;

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreateUserRequest {
    pub username: String,
    pub display_name: String,
    pub password: String,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default = "default_role")]
    pub role: String,
}

fn default_role() -> String {
    "user".to_owned()
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct PatchUserRequest {
    pub display_name: Option<String>,
    pub locale: Option<String>,
    pub role: Option<String>,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

/// # Errors
/// `403` se non admin.
#[utoipa::path(
    get,
    path = "/api/v1/users",
    tag = "users",
    operation_id = "users_list",
    security(("session_cookie" = [])),
    responses(
        (status = 200, description = "Elenco utenti", body = [UserView]),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Solo admin", body = Problem)
    )
)]
pub async fn list(
    State(state): State<AppState>,
    AdminAuth(ctx): AdminAuth,
) -> Result<Json<Vec<UserView>>, Problem> {
    let users = UserRepo::new(&state.db).list(&ctx).await?;
    Ok(Json(users.iter().map(UserView::from).collect()))
}

/// # Errors
/// `403` se non admin; `409` username/email già in uso; `422` dati non validi.
#[utoipa::path(
    post,
    path = "/api/v1/users",
    tag = "users",
    operation_id = "users_create",
    security(("session_cookie" = [])),
    request_body = CreateUserRequest,
    responses(
        (status = 201, description = "Utente creato", body = UserView),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Solo admin", body = Problem),
        (status = 409, description = "Username o email già in uso", body = Problem),
        (status = 422, description = "Dati non validi", body = Problem)
    )
)]
pub async fn create(
    State(state): State<AppState>,
    AdminAuth(ctx): AdminAuth,
    Json(body): Json<CreateUserRequest>,
) -> Result<(StatusCode, Json<UserView>), Problem> {
    let username = Username::parse(&body.username).map_err(|e| {
        Problem::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid-username",
            "Invalid username",
        )
        .with_detail(e.to_string())
    })?;
    let password = Password::parse(&body.password).map_err(|e| {
        Problem::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid-password",
            "Invalid password",
        )
        .with_detail(e.to_string())
    })?;
    let role = match body.role.as_str() {
        "admin" => SystemRole::Admin,
        "user" => SystemRole::User,
        _ => {
            return Err(Problem::new(
                StatusCode::UNPROCESSABLE_ENTITY,
                "invalid-role",
                "Role must be admin or user",
            ));
        }
    };
    let password_hash = hash_password(&password).map_err(|_| Problem::internal())?;
    let user = UserRepo::new(&state.db)
        .create(
            &ctx,
            NewUser {
                username,
                email: body.email.filter(|e| !e.is_empty()),
                display_name: body.display_name,
                password_hash: password_hash.as_str().to_owned(),
                role,
            },
        )
        .await?;
    Ok((StatusCode::CREATED, Json(UserView::from(&user))))
}

/// # Errors
/// Admin o sé stessi; altrimenti `403`.
#[utoipa::path(
    patch,
    path = "/api/v1/users/{id}",
    tag = "users",
    operation_id = "users_patch",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Id utente")),
    request_body = PatchUserRequest,
    responses(
        (status = 200, description = "Utente aggiornato", body = UserView),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Non consentito", body = Problem)
    )
)]
pub async fn patch(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Path(id): Path<UserId>,
    Json(body): Json<PatchUserRequest>,
) -> Result<Json<UserView>, Problem> {
    let user = UserRepo::new(&state.db)
        .update_profile(
            &ctx,
            id,
            body.display_name.as_deref(),
            body.locale.as_deref(),
            parse_optional_role(body.role.as_deref())?,
        )
        .await?;
    Ok(Json(UserView::from(&user)))
}

/// # Errors
/// `403` se non admin.
#[utoipa::path(
    post,
    path = "/api/v1/users/{id}/disable",
    tag = "users",
    operation_id = "users_disable",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Id utente")),
    responses(
        (status = 204, description = "Utente disabilitato; sessioni terminate"),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Solo admin", body = Problem),
        (status = 404, description = "Utente assente", body = Problem)
    )
)]
pub async fn disable(
    State(state): State<AppState>,
    AdminAuth(ctx): AdminAuth,
    Path(id): Path<UserId>,
) -> Result<StatusCode, Problem> {
    UserRepo::new(&state.db).disable(&ctx, id).await?;
    SessionRepo::new(&state.db).revoke_all_for_user(id).await?;
    state.sessions.clear();
    Ok(StatusCode::NO_CONTENT)
}

/// # Errors
/// `403` se non admin.
#[utoipa::path(
    post,
    path = "/api/v1/users/{id}/enable",
    tag = "users",
    operation_id = "users_enable",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Id utente")),
    responses(
        (status = 204, description = "Utente riabilitato"),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Solo admin", body = Problem),
        (status = 404, description = "Utente assente", body = Problem)
    )
)]
pub async fn enable(
    State(state): State<AppState>,
    AdminAuth(ctx): AdminAuth,
    Path(id): Path<UserId>,
) -> Result<StatusCode, Problem> {
    UserRepo::new(&state.db).enable(&ctx, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Cambio password: richiede la password attuale; revoca le altre sessioni.
///
/// # Errors
/// `403` se la password attuale è sbagliata.
#[utoipa::path(
    post,
    path = "/api/v1/users/me/password",
    tag = "users",
    operation_id = "users_change_password",
    security(("session_cookie" = [])),
    request_body = ChangePasswordRequest,
    responses(
        (status = 204, description = "Password aggiornata"),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Password attuale errata", body = Problem),
        (status = 422, description = "Password nuova non valida", body = Problem)
    )
)]
pub async fn change_password(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    jar: CookieJar,
    Json(body): Json<ChangePasswordRequest>,
) -> Result<StatusCode, Problem> {
    let user_id = ctx.user_id().ok_or_else(Problem::unauthenticated)?;
    let current = Password::parse(&body.current_password).map_err(|_| Problem::forbidden())?;
    let new_password = Password::parse(&body.new_password).map_err(|e| {
        Problem::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid-password",
            "Invalid password",
        )
        .with_detail(e.to_string())
    })?;

    let user = UserRepo::new(&state.db).find_by_id(&ctx, user_id).await?;
    let Some((_, hash)) = UserRepo::new(&state.db)
        .find_by_username(&user.username)
        .await?
    else {
        return Err(Problem::forbidden());
    };
    if !verify_password(&current, &hash) {
        return Err(Problem::forbidden());
    }

    let new_hash = hash_password(&new_password).map_err(|_| Problem::internal())?;
    UserRepo::new(&state.db)
        .set_password_hash(&ctx, user_id, new_hash.as_str())
        .await?;

    if let Some(raw) = jar.get(SESSION_COOKIE).map(|c| c.value().to_owned()) {
        let token = SessionToken::from_string(raw);
        SessionRepo::new(&state.db)
            .revoke_other_families(user_id, &token)
            .await?;
    }
    state.sessions.clear();

    Ok(StatusCode::NO_CONTENT)
}

fn parse_optional_role(raw: Option<&str>) -> Result<Option<SystemRole>, Problem> {
    match raw {
        None => Ok(None),
        Some("admin") => Ok(Some(SystemRole::Admin)),
        Some("user") => Ok(Some(SystemRole::User)),
        Some(_) => Err(Problem::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid-role",
            "Role must be admin or user",
        )),
    }
}
