use axum::extract::State;
use keeppix_db::ProblemsRepo;
use serde::Serialize;

use crate::extract::Auth;
use crate::json::Json;
use crate::problem::Problem;
use crate::routes::timeline::hex_bytes;
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

#[derive(Serialize, utoipa::ToSchema)]
pub struct DuplicateGroupView {
    content_hash: String,
    count: i64,
    size_bytes: i64,
    reclaimable_bytes: i64,
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

/// # Errors
/// `401` se non autenticato.
#[utoipa::path(
    get,
    path = "/api/v1/duplicates",
    tag = "library",
    operation_id = "duplicates_list",
    security(("session_cookie" = [])),
    responses(
        (status = 200, description = "Gruppi con lo stesso content_hash", body = [DuplicateGroupView]),
        (status = 401, description = "Non autenticato", body = Problem)
    )
)]
pub async fn duplicates(
    State(state): State<AppState>,
    Auth(ctx): Auth,
) -> Result<Json<Vec<DuplicateGroupView>>, Problem> {
    let groups = ProblemsRepo::new(&state.db).duplicates(&ctx).await?;
    Ok(Json(
        groups
            .into_iter()
            .map(|g| DuplicateGroupView {
                content_hash: hex_bytes(&g.content_hash),
                count: g.count,
                size_bytes: g.size_bytes,
                reclaimable_bytes: g.reclaimable_bytes(),
            })
            .collect(),
    ))
}
