use std::path::{Path, PathBuf};

use axum::extract::rejection::PathRejection;
use axum::extract::{Path as AxumPath, State};
use axum::http::StatusCode;
use keeppix_db::{JobRepo, MapRegion, NewMapRegion, RegionRepo, RegionStatus};
use serde::{Deserialize, Serialize};

use crate::{AdminAuth, AppState, Auth, Json, Problem};

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct RegionView {
    pub id: String,
    pub label: String,
    pub size_bytes: i64,
    pub version: String,
    pub downloaded_at: Option<String>,
    pub status: String,
    pub downloaded_bytes: i64,
    pub last_error: Option<String>,
    pub source_url: String,
    pub checksum_sha256: String,
}

impl From<MapRegion> for RegionView {
    fn from(region: MapRegion) -> Self {
        Self {
            id: region.id,
            label: region.label,
            size_bytes: region.size_bytes,
            version: region.version,
            downloaded_at: region.downloaded_at.map(|value| value.to_rfc3339()),
            status: region.status.as_str().to_owned(),
            downloaded_bytes: region.downloaded_bytes,
            last_error: region.last_error,
            source_url: region.source_url,
            checksum_sha256: region.checksum_sha256,
        }
    }
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct DownloadRegionRequest {
    pub id: String,
    pub label: String,
    pub size_bytes: i64,
    pub version: String,
    pub source_url: String,
    pub checksum_sha256: String,
}

/// # Errors
/// `401` without a session.
#[utoipa::path(
    get,
    path = "/api/v1/map/regions",
    tag = "map",
    operation_id = "map_regions_list",
    summary = "List map regions",
    security(("session_cookie" = [])),
    responses(
        (status = 200, description = "PMTiles region status", body = [RegionView]),
        (status = 401, description = "Not authenticated", body = Problem)
    )
)]
pub async fn list(
    State(state): State<AppState>,
    Auth(ctx): Auth,
) -> Result<Json<Vec<RegionView>>, Problem> {
    let regions = RegionRepo::new(&state.db).list(&ctx).await?;
    Ok(Json(regions.into_iter().map(RegionView::from).collect()))
}

/// # Errors
/// `403` for non-admin, `409` if already downloading, `422` for metadata or
/// a URL outside the allowlist.
#[utoipa::path(
    post,
    path = "/api/v1/map/regions",
    tag = "map",
    operation_id = "map_regions_download",
    summary = "Download a map region",
    security(("session_cookie" = [])),
    request_body = DownloadRegionRequest,
    responses(
        (status = 202, description = "Download queued", body = RegionView),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "Admin only", body = Problem),
        (status = 409, description = "Download already active", body = Problem),
        (status = 422, description = "Invalid source or metadata", body = Problem)
    )
)]
pub async fn download(
    State(state): State<AppState>,
    AdminAuth(ctx): AdminAuth,
    Json(body): Json<DownloadRegionRequest>,
) -> Result<(StatusCode, Json<RegionView>), Problem> {
    let id = body.id.clone();
    let region = NewMapRegion {
        id: body.id,
        label: body.label,
        size_bytes: body.size_bytes,
        version: body.version,
        source_url: body.source_url,
        checksum_sha256: body.checksum_sha256,
    };
    keeppix_jobs::regions::enqueue_download(&state.db, &ctx, region)
        .await
        .map_err(region_error)?;
    let region = RegionRepo::new(&state.db).find(&ctx, &id).await?;
    Ok((StatusCode::ACCEPTED, Json(RegionView::from(region))))
}

/// # Errors
/// `403` for non-admin, `404` if not found, `409` if not downloading.
#[utoipa::path(
    post,
    path = "/api/v1/map/regions/{id}/cancel",
    tag = "map",
    operation_id = "map_regions_cancel",
    summary = "Cancel a map region download",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Region id")),
    responses(
        (status = 204, description = "Download cancelled"),
        (status = 400, description = "Invalid region path", body = Problem),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "Admin only", body = Problem),
        (status = 404, description = "Region not found", body = Problem),
        (status = 409, description = "Region not downloading", body = Problem)
    )
)]
pub async fn cancel(
    State(state): State<AppState>,
    AdminAuth(ctx): AdminAuth,
    path: Result<AxumPath<String>, PathRejection>,
) -> Result<StatusCode, Problem> {
    let id = region_path(path)?;
    let repo = RegionRepo::new(&state.db);
    let region = repo.find(&ctx, &id).await?;
    repo.request_cancel(&ctx, &id).await?;
    remove_region_files(&state.data_dir, &region.file_path).await?;
    retire_region_job(&state, &id, region.download_generation).await?;
    repo.finish_cancel(&id, region.download_generation).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// # Errors
/// `403` for non-admin, `404` if not found, `500` if the files cannot be
/// removed.
#[utoipa::path(
    delete,
    path = "/api/v1/map/regions/{id}",
    tag = "map",
    operation_id = "map_regions_delete",
    summary = "Delete a map region",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Region id")),
    responses(
        (status = 204, description = "Region deleted"),
        (status = 400, description = "Invalid region path", body = Problem),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "Admin only", body = Problem),
        (status = 404, description = "Region not found", body = Problem)
    )
)]
pub async fn delete(
    State(state): State<AppState>,
    AdminAuth(ctx): AdminAuth,
    path: Result<AxumPath<String>, PathRejection>,
) -> Result<StatusCode, Problem> {
    let id = region_path(path)?;
    let repo = RegionRepo::new(&state.db);
    let region = repo.find(&ctx, &id).await?;
    if region.status == RegionStatus::Downloading {
        repo.request_cancel(&ctx, &id).await?;
    }
    remove_region_files(&state.data_dir, &region.file_path).await?;
    if region.status == RegionStatus::Downloading {
        retire_region_job(&state, &id, region.download_generation).await?;
    }
    repo.delete(&ctx, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}

fn region_error(error: keeppix_jobs::regions::RegionError) -> Problem {
    match error {
        keeppix_jobs::regions::RegionError::SourceNotAllowed => Problem::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "region-source-not-allowed",
            "Region source URL is not allowed",
        ),
        keeppix_jobs::regions::RegionError::InvalidRegion => Problem::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid-region",
            "Invalid region metadata",
        ),
        keeppix_jobs::regions::RegionError::Db(error) => Problem::from(error),
    }
}

fn region_path(path: Result<AxumPath<String>, PathRejection>) -> Result<String, Problem> {
    let AxumPath(id) = path.map_err(|rejection| {
        Problem::bad_request("invalid-region-path", "Invalid region path")
            .with_detail(rejection.body_text())
    })?;
    Ok(id)
}

async fn remove_if_present(path: PathBuf) -> Result<(), Problem> {
    match tokio::fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => {
            tracing::error!(%error, "cannot remove PMTiles region file");
            Err(Problem::internal())
        }
    }
}

async fn remove_region_files(data_dir: &Path, file_path: &str) -> Result<(), Problem> {
    remove_if_present(partial_path(data_dir, file_path)).await?;
    remove_if_present(final_path(data_dir, file_path)).await
}

async fn retire_region_job(
    state: &AppState,
    id: &str,
    generation: uuid::Uuid,
) -> Result<(), Problem> {
    let dedup_key = format!("map-region:{id}:{generation}");
    JobRepo::new(&state.db)
        .retire_active(&dedup_key, "Download cancelled")
        .await?;
    Ok(())
}

fn final_path(data_dir: &Path, file_path: &str) -> PathBuf {
    data_dir.join(file_path)
}

fn partial_path(data_dir: &Path, file_path: &str) -> PathBuf {
    let mut partial = final_path(data_dir, file_path).into_os_string();
    partial.push(".part");
    PathBuf::from(partial)
}
