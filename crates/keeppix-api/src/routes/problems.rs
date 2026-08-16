use axum::extract::State;
use keeppix_db::ProblemsRepo;
use serde::Serialize;

use crate::extract::Auth;
use crate::json::Json;
use crate::problem::Problem;
use crate::state::AppState;

#[derive(Serialize, utoipa::ToSchema)]
pub struct OfflineLibraryView {
    id: String,
    name: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct FailedJobView {
    id: i64,
    kind: String,
    last_error: Option<String>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct ErrorAssetView {
    id: String,
    filename: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct ProblemsView {
    offline_libraries: Vec<OfflineLibraryView>,
    failed_jobs: Vec<FailedJobView>,
    error_assets: Vec<ErrorAssetView>,
}

/// # Errors
/// `401` se non autenticato.
#[utoipa::path(
    get,
    path = "/api/v1/problems",
    tag = "library",
    operation_id = "problems_list",
    security(("session_cookie" = [])),
    responses(
        (status = 200, description = "Problemi visibili", body = ProblemsView),
        (status = 401, description = "Non autenticato", body = Problem)
    )
)]
pub async fn list(
    State(state): State<AppState>,
    Auth(ctx): Auth,
) -> Result<Json<ProblemsView>, Problem> {
    let set = ProblemsRepo::new(&state.db).list(&ctx).await?;
    Ok(Json(ProblemsView {
        offline_libraries: set
            .offline_libraries
            .into_iter()
            .map(|l| OfflineLibraryView {
                id: l.id.to_string(),
                name: l.name,
            })
            .collect(),
        failed_jobs: set
            .failed_jobs
            .into_iter()
            .map(|j| FailedJobView {
                id: j.id,
                kind: j.kind,
                last_error: j.last_error,
            })
            .collect(),
        error_assets: set
            .error_assets
            .into_iter()
            .map(|a| ErrorAssetView {
                id: a.id.to_string(),
                filename: a.filename,
            })
            .collect(),
    }))
}
