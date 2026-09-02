use std::path::{Path, PathBuf};

use axum::extract::rejection::PathRejection;
use axum::extract::{Path as AxumPath, State};
use axum::http::StatusCode;
use keeppix_db::{JobRepo, MapRegion, NewMapRegion, RegionAcquisition, RegionRepo, RegionStatus};
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

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct CatalogEntryView {
    pub id: String,
    pub label: String,
    pub approx_size_bytes: i64,
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

/// Lists the searchable catalog of 35 countries the region search box
/// (`docs/ui/documento-funzionale-ui.md`, "B — Ricerca di regione") picks
/// from — a fixed, small enough list that the frontend filters it
/// client-side rather than this endpoint taking a query parameter.
///
/// # Errors
/// `401` without a session.
#[utoipa::path(
    get,
    path = "/api/v1/map/regions/catalog",
    tag = "map",
    operation_id = "map_regions_catalog",
    summary = "List the searchable map region catalog",
    security(("session_cookie" = [])),
    responses(
        (status = 200, description = "Downloadable region catalog", body = [CatalogEntryView]),
        (status = 401, description = "Not authenticated", body = Problem)
    )
)]
pub async fn catalog(Auth(_ctx): Auth) -> Json<Vec<CatalogEntryView>> {
    Json(
        keeppix_jobs::map_catalog::CATALOG
            .iter()
            .map(|entry| CatalogEntryView {
                id: entry.id.to_owned(),
                label: entry.label.to_owned(),
                approx_size_bytes: entry.approx_size_bytes,
            })
            .collect(),
    )
}

/// Downloads a catalog entry by id — the search-box counterpart to
/// [`download`], which instead takes a hand-typed URL/checksum.
///
/// # Errors
/// `403` for non-admin, `404` for an unknown catalog id, `409` if already
/// downloading.
#[utoipa::path(
    post,
    path = "/api/v1/map/regions/catalog/{id}",
    tag = "map",
    operation_id = "map_regions_download_from_catalog",
    summary = "Download a map region from the catalog",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Catalog id")),
    responses(
        (status = 202, description = "Extraction queued", body = RegionView),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "Admin only", body = Problem),
        (status = 404, description = "Unknown catalog id", body = Problem),
        (status = 409, description = "Download already active", body = Problem)
    )
)]
pub async fn download_from_catalog(
    State(state): State<AppState>,
    AdminAuth(ctx): AdminAuth,
    path: Result<AxumPath<String>, PathRejection>,
) -> Result<(StatusCode, Json<RegionView>), Problem> {
    let id = region_path(path)?;
    keeppix_jobs::map_extract::enqueue_extraction(&state.db, &ctx, &id)
        .await
        .map_err(map_extract_error)?;
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
    retire_region_job(&state, &id, region.download_generation, region.acquisition).await?;
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
        retire_region_job(&state, &id, region.download_generation, region.acquisition).await?;
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

fn map_extract_error(error: keeppix_jobs::map_extract::MapExtractError) -> Problem {
    match error {
        keeppix_jobs::map_extract::MapExtractError::UnknownCatalogId(id) => {
            Problem::new(StatusCode::NOT_FOUND, "unknown-region-catalog-id", "Unknown region catalog id")
                .with_detail(id)
        }
        keeppix_jobs::map_extract::MapExtractError::Db(error) => Problem::from(error),
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
    acquisition: RegionAcquisition,
) -> Result<(), Problem> {
    // Must match whichever flow's own dedup-key prefix actually created
    // the job: `keeppix_jobs::regions::enqueue_download` vs
    // `keeppix_jobs::map_extract::enqueue_extraction`. Retiring the wrong
    // key silently no-ops, leaving the real job to keep running to
    // completion before its `mark_available`/`mark_available_with_actuals`
    // finds the row already reassigned and discards its own result.
    let prefix = match acquisition {
        RegionAcquisition::Download => "map-region",
        RegionAcquisition::Extract => "map-region-extract",
    };
    let dedup_key = format!("{prefix}:{id}:{generation}");
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
