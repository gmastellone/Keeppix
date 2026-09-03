use axum::extract::rejection::{PathRejection, QueryRejection};
use axum::extract::{Path as AxumPath, Query, State};
use axum::http::{HeaderMap, header};
use axum::response::Response;
use keeppix_db::{GeoRepo, MapBounds, MapCluster, MapScope, RegionRepo};
use keeppix_domain::{AlbumId, FolderId, LibraryId};
use serde::{Deserialize, Serialize};

use crate::{AppState, Auth, Json, Problem};

const MAX_ZOOM: i32 = 30;

#[derive(Deserialize)]
pub struct ClustersQuery {
    bbox: String,
    zoom: i32,
    scope: String,
    scope_id: uuid::Uuid,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct MapClusterView {
    pub lat: f64,
    pub lon: f64,
    pub count: i64,
    pub cover_asset_id: String,
    pub clustered: bool,
    /// Id of `cover_asset_id`'s folder: opens "Open folder" from the
    /// popover without a second request.
    pub folder_id: String,
    /// Human-readable place label, from reverse geocoding.
    /// `None` until the cover asset has an assigned place.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub place_label: Option<String>,
}

impl From<MapCluster> for MapClusterView {
    fn from(cluster: MapCluster) -> Self {
        Self {
            lat: cluster.lat,
            lon: cluster.lon,
            count: cluster.count,
            cover_asset_id: cluster.cover_asset_id.to_string(),
            clustered: cluster.clustered,
            folder_id: cluster.folder_id.to_string(),
            place_label: cluster.place_label,
        }
    }
}

/// # Errors
/// `400` for a malformed bbox, zoom, or scope; `401` without a session;
/// `403` for an unknown or not-visible scope.
#[utoipa::path(
    get,
    path = "/api/v1/map/clusters",
    tag = "map",
    operation_id = "map_clusters",
    summary = "Get map clusters",
    security(("session_cookie" = [])),
    params(
        ("bbox" = String, Query, description = "west,south,east,north in WGS84 degrees"),
        ("zoom" = i32, Query, description = "Zoom level, 0..=30"),
        ("scope" = String, Query, description = "library, album, folder, or search"),
        ("scope_id" = String, Query, description = "UUID of the scope")
    ),
    responses(
        (status = 200, description = "Aggregated cells or individual points", body = [MapClusterView]),
        (status = 400, description = "Invalid parameters", body = Problem),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "Scope not visible", body = Problem),
        (status = 503, description = "Database unavailable", body = Problem),
        (status = 500, description = "Database error", body = Problem)
    )
)]
pub async fn clusters(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    query: Result<Query<ClustersQuery>, QueryRejection>,
) -> Result<Json<Vec<MapClusterView>>, Problem> {
    let Query(query) = query.map_err(|rejection| {
        Problem::bad_request("invalid-query", "Invalid query parameters")
            .with_detail(rejection.body_text())
    })?;
    let bounds = parse_bounds(&query.bbox)?;
    let zoom = u8::try_from(query.zoom)
        .ok()
        .filter(|zoom| i32::from(*zoom) <= MAX_ZOOM)
        .ok_or_else(|| Problem::bad_request("invalid-zoom", "Invalid map zoom"))?;
    let map_scope = match query.scope.as_str() {
        "library" => MapScope::Library(LibraryId::from_uuid(query.scope_id)),
        "album" => MapScope::Album(AlbumId::from_uuid(query.scope_id)),
        "folder" => MapScope::Folder(FolderId::from_uuid(query.scope_id)),
        "search" => MapScope::Search(query.scope_id),
        _ => {
            return Err(Problem::bad_request(
                "invalid-map-scope",
                "Invalid map scope",
            ));
        }
    };

    let rows = GeoRepo::new(&state.db)
        .clusters(&ctx, bounds, zoom, map_scope)
        .await?;
    Ok(Json(rows.into_iter().map(MapClusterView::from).collect()))
}

/// Serves the local PMTiles file via byte range. The coordinates stay in
/// the path for the map protocol, but the payload is read directly from
/// the archive with no server-side decompression or rendering.
///
/// # Errors
/// `401` without a session; `404` if the region is missing or has been
/// deleted; `416` for an invalid range.
#[utoipa::path(
    get,
    path = "/api/v1/map/tiles/{region}/{z}/{x}/{y}",
    tag = "map",
    operation_id = "map_tile_archive",
    summary = "Serve a map tile from the archive",
    security(("session_cookie" = [])),
    params(
        ("region" = String, Path, description = "Local region id"),
        ("z" = u8, Path, description = "Zoom"),
        ("x" = u32, Path, description = "Tile X coordinate"),
        ("y" = u32, Path, description = "Tile Y coordinate")
    ),
    responses(
        (status = 200, description = "PMTiles archive"),
        (status = 206, description = "PMTiles byte range"),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 404, description = "Region not available", body = Problem),
        (status = 416, description = "Invalid range", body = Problem)
    )
)]
pub async fn tiles(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    path: Result<AxumPath<(String, u8, u32, u32)>, PathRejection>,
    headers: HeaderMap,
) -> Result<Response, Problem> {
    let AxumPath((region_id, _z, _x, _y)) = path.map_err(|rejection| {
        Problem::bad_request("invalid-map-tile-path", "Invalid map tile path")
            .with_detail(rejection.body_text())
    })?;
    let region = RegionRepo::new(&state.db)
        .find_available(&ctx, &region_id)
        .await?;
    let path = state.data_dir.join(region.file_path);
    super::media::stream_file(
        &path,
        headers
            .get(header::RANGE)
            .and_then(|value| value.to_str().ok()),
        "application/vnd.pmtiles",
        false,
    )
    .await
}

/// Same byte-range archive stream as [`tiles`], addressed without the
/// (unused) z/x/y path segments.
///
/// Exists because the frontend's `pmtiles` protocol handler needs one
/// fixed URL per region to both register a `Source` under *and* build a
/// `tiles: ["pmtiles://<this>/{z}/{x}/{y}"]` template from — reusing
/// [`tiles`] itself for that (with a placeholder like `0/0/0`) makes the
/// resulting request URL end in six consecutive numeric segments once a
/// real z/x/y is appended, which the client-side pmtiles library can't
/// reliably split back into "archive part" vs "z/x/y part". A URL that
/// ends in a non-numeric segment has no such ambiguity.
///
/// # Errors
/// `401` without a session; `404` if the region is missing or has been
/// deleted; `416` for an invalid range.
#[utoipa::path(
    get,
    path = "/api/v1/map/tiles/{region}/archive",
    tag = "map",
    operation_id = "map_tile_archive_raw",
    summary = "Serve a map region's raw archive by byte range",
    security(("session_cookie" = [])),
    params(("region" = String, Path, description = "Local region id")),
    responses(
        (status = 200, description = "PMTiles archive"),
        (status = 206, description = "PMTiles byte range"),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 404, description = "Region not available", body = Problem),
        (status = 416, description = "Invalid range", body = Problem)
    )
)]
pub async fn tile_archive(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    path: Result<AxumPath<String>, PathRejection>,
    headers: HeaderMap,
) -> Result<Response, Problem> {
    let AxumPath(region_id) = path.map_err(|rejection| {
        Problem::bad_request("invalid-map-tile-path", "Invalid map tile path")
            .with_detail(rejection.body_text())
    })?;
    let region = RegionRepo::new(&state.db)
        .find_available(&ctx, &region_id)
        .await?;
    let path = state.data_dir.join(region.file_path);
    super::media::stream_file(
        &path,
        headers
            .get(header::RANGE)
            .and_then(|value| value.to_str().ok()),
        "application/vnd.pmtiles",
        false,
    )
    .await
}

pub(crate) fn parse_bounds(raw: &str) -> Result<MapBounds, Problem> {
    let values = raw
        .split(',')
        .map(str::trim)
        .map(str::parse::<f64>)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| Problem::bad_request("invalid-bbox", "Invalid map bounding box"))?;
    let [west, south, east, north] = values.as_slice() else {
        return Err(Problem::bad_request(
            "invalid-bbox",
            "Invalid map bounding box",
        ));
    };
    if !values.iter().all(|value| value.is_finite())
        || !(-180.0..=180.0).contains(west)
        || !(-180.0..=180.0).contains(east)
        || !(-90.0..=90.0).contains(south)
        || !(-90.0..=90.0).contains(north)
        || south >= north
        || west.total_cmp(east).is_eq()
    {
        return Err(Problem::bad_request(
            "invalid-bbox",
            "Invalid map bounding box",
        ));
    }
    Ok(MapBounds {
        west: *west,
        south: *south,
        east: *east,
        north: *north,
    })
}
