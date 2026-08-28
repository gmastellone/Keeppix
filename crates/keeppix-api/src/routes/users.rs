//! User management. No SQL: only `UserRepo` / `SessionRepo`.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum_extra::extract::CookieJar;
use keeppix_db::{HomeRepo, SessionRepo, UserRepo};
use keeppix_domain::{
    GeoPoint, NewUser, Password, SessionToken, SystemRole, UserId, Username, hash_password,
    verify_password,
};
use serde::{Deserialize, Serialize};

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

#[derive(Deserialize, utoipa::ToSchema)]
pub struct SetHomeRequest {
    pub lat: f64,
    pub lon: f64,
    #[serde(default = "default_home_radius")]
    pub radius_m: i32,
}

const fn default_home_radius() -> i32 {
    200
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct HomeView {
    pub lat: f64,
    pub lon: f64,
    pub radius_m: i32,
}

/// # Errors
/// `403` if not admin.
#[utoipa::path(
    get,
    path = "/api/v1/users",
    tag = "users",
    operation_id = "users_list",
    summary = "List users",
    security(("session_cookie" = [])),
    responses(
        (status = 200, description = "List of users", body = [UserView]),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "Admin only", body = Problem)
    )
)]
pub async fn list(
    State(state): State<AppState>,
    AdminAuth(ctx): AdminAuth,
) -> Result<Json<Vec<UserView>>, Problem> {
    let users = UserRepo::new(&state.db).list(&ctx).await?;
    Ok(Json(
        users
            .iter()
            .map(|u| UserView::new(u, &state.server_name))
            .collect(),
    ))
}

/// # Errors
/// `403` if not admin; `409` if username/email already in use; `422` if
/// invalid data.
#[utoipa::path(
    post,
    path = "/api/v1/users",
    tag = "users",
    operation_id = "users_create",
    summary = "Create a user",
    security(("session_cookie" = [])),
    request_body = CreateUserRequest,
    responses(
        (status = 201, description = "User created", body = UserView),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "Admin only", body = Problem),
        (status = 409, description = "Username or email already in use", body = Problem),
        (status = 422, description = "Invalid data", body = Problem)
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
    let password = Password::parse_owned(body.password).map_err(|e| {
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
    Ok((
        StatusCode::CREATED,
        Json(UserView::new(&user, &state.server_name)),
    ))
}

/// # Errors
/// Admin or self; otherwise `403`.
#[utoipa::path(
    patch,
    path = "/api/v1/users/{id}",
    tag = "users",
    operation_id = "users_patch",
    summary = "Update a user",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "User id")),
    request_body = PatchUserRequest,
    responses(
        (status = 200, description = "User updated", body = UserView),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "Not allowed", body = Problem)
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
    state.sessions.clear();
    Ok(Json(UserView::new(&user, &state.server_name)))
}

/// # Errors
/// `403` if not admin.
#[utoipa::path(
    post,
    path = "/api/v1/users/{id}/disable",
    tag = "users",
    operation_id = "users_disable",
    summary = "Disable a user",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "User id")),
    responses(
        (status = 204, description = "User disabled; sessions terminated"),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "Admin only", body = Problem),
        (status = 404, description = "User not found", body = Problem)
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
/// `403` if not admin.
#[utoipa::path(
    post,
    path = "/api/v1/users/{id}/enable",
    tag = "users",
    operation_id = "users_enable",
    summary = "Enable a user",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "User id")),
    responses(
        (status = 204, description = "User re-enabled"),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "Admin only", body = Problem),
        (status = 404, description = "User not found", body = Problem)
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

/// Password change: requires the current password; revokes the other
/// sessions.
///
/// # Errors
/// `403` if the current password is wrong.
#[utoipa::path(
    post,
    path = "/api/v1/users/me/password",
    tag = "users",
    operation_id = "users_change_password",
    summary = "Change the current user's password",
    security(("session_cookie" = [])),
    request_body = ChangePasswordRequest,
    responses(
        (status = 204, description = "Password updated"),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "Current password wrong", body = Problem),
        (status = 422, description = "Invalid new password", body = Problem)
    )
)]
pub async fn change_password(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    jar: CookieJar,
    Json(body): Json<ChangePasswordRequest>,
) -> Result<StatusCode, Problem> {
    let user_id = ctx.user_id().ok_or_else(Problem::unauthenticated)?;
    let current = Password::parse_owned(body.current_password).map_err(|_| Problem::forbidden())?;
    let new_password = Password::parse_owned(body.new_password).map_err(|e| {
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

/// Sets or updates the "home" point used for the geofence on public links.
///
/// # Errors
/// `401` if not authenticated; `409` if `radius_m` is not positive.
#[utoipa::path(
    put,
    path = "/api/v1/users/me/home",
    tag = "users",
    operation_id = "users_set_home",
    summary = "Set the current user's home location",
    security(("session_cookie" = [])),
    request_body = SetHomeRequest,
    responses(
        (status = 200, description = "Home updated", body = HomeView),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 409, description = "Invalid radius", body = Problem)
    )
)]
pub async fn set_home(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Json(body): Json<SetHomeRequest>,
) -> Result<Json<HomeView>, Problem> {
    let home = HomeRepo::new(&state.db)
        .set(
            &ctx,
            GeoPoint {
                lat: body.lat,
                lon: body.lon,
            },
            body.radius_m,
        )
        .await?;
    Ok(Json(HomeView {
        lat: home.point.lat,
        lon: home.point.lon,
        radius_m: home.radius_m,
    }))
}

/// Removes home: no geofence until it is set again.
///
/// # Errors
/// `401` if not authenticated.
#[utoipa::path(
    delete,
    path = "/api/v1/users/me/home",
    tag = "users",
    operation_id = "users_delete_home",
    summary = "Clear the current user's home location",
    security(("session_cookie" = [])),
    responses(
        (status = 204, description = "Home removed"),
        (status = 401, description = "Not authenticated", body = Problem)
    )
)]
pub async fn delete_home(
    State(state): State<AppState>,
    Auth(ctx): Auth,
) -> Result<StatusCode, Problem> {
    HomeRepo::new(&state.db).delete(&ctx).await?;
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
