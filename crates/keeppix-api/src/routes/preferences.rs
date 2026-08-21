//! User presentation preferences (`GET`/`PATCH /users/me/preferences`).

use axum::extract::State;
use keeppix_db::{PreferencesPatchError, PreferencesRepo, UserPreferences};
use serde::Serialize;

use crate::extract::Auth;
use crate::json::Json;
use crate::problem::Problem;
use crate::state::AppState;

#[derive(Serialize, utoipa::ToSchema)]
pub struct UserPreferencesView {
    pub theme: String,
    pub grid_density: GridDensityView,
    pub notifications: NotificationPreferencesView,
    pub language: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct GridDensityView {
    pub desktop: u8,
    pub mobile: u8,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct NotificationPreferencesView {
    pub digest: bool,
    pub condivisioni: bool,
    pub problemi: bool,
}

impl From<UserPreferences> for UserPreferencesView {
    fn from(prefs: UserPreferences) -> Self {
        Self {
            theme: prefs.theme,
            grid_density: GridDensityView {
                desktop: prefs.grid_density.desktop,
                mobile: prefs.grid_density.mobile,
            },
            notifications: NotificationPreferencesView {
                digest: prefs.notifications.digest,
                condivisioni: prefs.notifications.condivisioni,
                problemi: prefs.notifications.problemi,
            },
            language: prefs.language,
        }
    }
}

fn patch_error(err: PreferencesPatchError) -> Problem {
    match err {
        PreferencesPatchError::UnknownField(field) => {
            Problem::bad_request("unknown-field", "Unknown preference field").with_detail(field)
        }
        PreferencesPatchError::InvalidValue(msg) => {
            Problem::bad_request("invalid-preference", "Invalid preference value").with_detail(msg)
        }
    }
}

/// # Errors
/// `401` if not authenticated.
#[utoipa::path(
    get,
    path = "/api/v1/users/me/preferences",
    tag = "users",
    operation_id = "users_preferences_get",
    summary = "Get the current user's preferences",
    security(("session_cookie" = [])),
    responses(
        (status = 200, description = "Preferences document", body = UserPreferencesView),
        (status = 401, description = "Not authenticated", body = Problem)
    )
)]
pub async fn get(
    State(state): State<AppState>,
    Auth(ctx): Auth,
) -> Result<Json<UserPreferencesView>, Problem> {
    let prefs = PreferencesRepo::new(&state.db).get(&ctx).await?;
    Ok(Json(UserPreferencesView::from(prefs)))
}

/// Partial merge: only the fields sent in the body are updated.
///
/// # Errors
/// `401` if not authenticated; `400` for unknown fields or invalid values.
#[utoipa::path(
    patch,
    path = "/api/v1/users/me/preferences",
    tag = "users",
    operation_id = "users_preferences_patch",
    summary = "Update the current user's preferences",
    security(("session_cookie" = [])),
    request_body(content = serde_json::Value, description = "Partial preferences patch"),
    responses(
        (status = 200, description = "Updated preferences", body = UserPreferencesView),
        (status = 400, description = "Unknown field or invalid value", body = Problem),
        (status = 401, description = "Not authenticated", body = Problem)
    )
)]
pub async fn patch(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<UserPreferencesView>, Problem> {
    let repo = PreferencesRepo::new(&state.db);
    let mut prefs = repo.get(&ctx).await?;
    prefs.apply_patch(&body).map_err(patch_error)?;
    repo.set(&ctx, &prefs).await?;
    Ok(Json(UserPreferencesView::from(prefs)))
}
