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
}

impl From<MapCluster> for MapClusterView {
    fn from(cluster: MapCluster) -> Self {
        Self {
            lat: cluster.lat,
            lon: cluster.lon,
            count: cluster.count,
            cover_asset_id: cluster.cover_asset_id.to_string(),
            clustered: cluster.clustered,
        }
    }
}

/// # Errors
/// `400` per bbox, zoom o scope malformati; `401` senza sessione; `403` per
/// uno scope sconosciuto o non visibile.
#[utoipa::path(
    get,
    path = "/api/v1/map/clusters",
    tag = "map",
    operation_id = "map_clusters",
    security(("session_cookie" = [])),
    params(
        ("bbox" = String, Query, description = "west,south,east,north in gradi WGS84"),
        ("zoom" = i32, Query, description = "Livello di zoom, 0..=30"),
        ("scope" = String, Query, description = "library, album, folder o search"),
        ("scope_id" = String, Query, description = "UUID dello scope")
    ),
    responses(
        (status = 200, description = "Celle aggregate o punti singoli", body = [MapClusterView]),
        (status = 400, description = "Parametri non validi", body = Problem),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Scope non visibile", body = Problem),
        (status = 503, description = "Database non disponibile", body = Problem),
        (status = 500, description = "Errore del database", body = Problem)
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

/// Serve il file PMTiles locale tramite byte range. Le coordinate restano nel
/// percorso per il protocollo della mappa, ma il payload è letto direttamente
/// dall'archivio senza decompressione o rendering server-side.
///
/// # Errors
/// `401` senza sessione; `404` se la regione è assente o è stata cancellata;
/// `416` per un range non valido.
#[utoipa::path(
    get,
    path = "/api/v1/map/tiles/{region}/{z}/{x}/{y}",
    tag = "map",
    operation_id = "map_tile_archive",
    security(("session_cookie" = [])),
    params(
        ("region" = String, Path, description = "Id della regione locale"),
        ("z" = u8, Path, description = "Zoom"),
        ("x" = u32, Path, description = "Coordinata tile X"),
        ("y" = u32, Path, description = "Coordinata tile Y")
    ),
    responses(
        (status = 200, description = "Archivio PMTiles"),
        (status = 206, description = "Byte range PMTiles"),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 404, description = "Regione non disponibile", body = Problem),
        (status = 416, description = "Range non valido", body = Problem)
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
    RegionRepo::new(&state.db)
        .find_available(&ctx, &region_id)
        .await?;
    let path = state
        .data_dir
        .join("maps")
        .join(format!("{region_id}.pmtiles"));
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

fn parse_bounds(raw: &str) -> Result<MapBounds, Problem> {
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
