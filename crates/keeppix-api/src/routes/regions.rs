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
            status: match region.status {
                RegionStatus::Available => "available",
                RegionStatus::Downloading => "downloading",
                RegionStatus::Error => "error",
            }
            .to_owned(),
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
/// `401` senza sessione.
#[utoipa::path(
    get,
    path = "/api/v1/map/regions",
    tag = "map",
    operation_id = "map_regions_list",
    security(("session_cookie" = [])),
    responses(
        (status = 200, description = "Stato regioni PMTiles", body = [RegionView]),
        (status = 401, description = "Non autenticato", body = Problem)
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
/// `403` per non-admin, `409` se già in download, `422` per metadati o URL
/// fuori allowlist.
#[utoipa::path(
    post,
    path = "/api/v1/map/regions",
    tag = "map",
    operation_id = "map_regions_download",
    security(("session_cookie" = [])),
    request_body = DownloadRegionRequest,
    responses(
        (status = 202, description = "Download accodato", body = RegionView),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Solo admin", body = Problem),
        (status = 409, description = "Download già attivo", body = Problem),
        (status = 422, description = "Sorgente o metadati non validi", body = Problem)
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
/// `403` per non-admin, `404` se assente, `409` se non in download.
#[utoipa::path(
    post,
    path = "/api/v1/map/regions/{id}/cancel",
    tag = "map",
    operation_id = "map_regions_cancel",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Id regione")),
    responses(
        (status = 204, description = "Download annullato"),
        (status = 400, description = "Percorso regione non valido", body = Problem),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Solo admin", body = Problem),
        (status = 404, description = "Regione assente", body = Problem),
        (status = 409, description = "Regione non in download", body = Problem)
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
/// `403` per non-admin, `404` se assente, `500` se i file non sono rimovibili.
#[utoipa::path(
    delete,
    path = "/api/v1/map/regions/{id}",
    tag = "map",
    operation_id = "map_regions_delete",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Id regione")),
    responses(
        (status = 204, description = "Regione eliminata"),
        (status = 400, description = "Percorso regione non valido", body = Problem),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Solo admin", body = Problem),
        (status = 404, description = "Regione assente", body = Problem)
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
