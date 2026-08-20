use axum::extract::rejection::PathRejection;
use axum::extract::{Path, Query, State};
use chrono::{DateTime, NaiveDate, SecondsFormat, Utc};
use keeppix_db::{AssetRepo, OverrideRepo, TimelineRepo};
use keeppix_domain::{Asset, AssetId, LibraryId};
use serde::{Deserialize, Serialize};

use crate::extract::SessionNotShare;
use crate::json::Json;
use crate::problem::Problem;
use crate::routes::metadata::GeoPointView;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct BucketsQuery {
    library: Option<LibraryId>,
    bbox: Option<String>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct MonthBucketView {
    /// Calendario `YYYY-MM`.
    pub month: String,
    pub count: i64,
}

#[derive(Deserialize)]
pub struct TimelineQuery {
    bucket: String,
    cursor: Option<String>,
    limit: Option<i64>,
    bbox: Option<String>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct TimelinePage {
    pub assets: Vec<AssetView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct AssetView {
    pub id: String,
    pub folder_id: String,
    pub filename: String,
    pub content_hash: Option<String>,
    pub size_bytes: i64,
    pub kind: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub taken_at_utc: Option<DateTime<Utc>>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbhash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<GeoPointView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub place_id: Option<i64>,
}

impl AssetView {
    pub(crate) fn from_asset(a: &Asset) -> Self {
        Self {
            id: a.id.to_string(),
            folder_id: a.folder_id.to_string(),
            filename: a.filename.as_str().to_owned(),
            content_hash: a.content_hash.as_ref().map(hex_hash),
            size_bytes: a.size_bytes,
            kind: kind_str(a.kind).to_owned(),
            status: status_str(a.status).to_owned(),
            taken_at_utc: a.taken_at_utc,
            width: a.width,
            height: a.height,
            thumbhash: a.thumbhash.as_deref().map(hex_bytes),
            location: None,
            place_id: None,
        }
    }

    pub(crate) fn with_location(
        mut self,
        location: Option<GeoPointView>,
        place_id: Option<i64>,
    ) -> Self {
        self.location = location;
        self.place_id = place_id;
        self
    }
}

pub(crate) fn hex_hash(hash: &[u8; 32]) -> String {
    hex_bytes(hash)
}

pub(crate) fn hex_bytes(bytes: &[u8]) -> String {
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut out, byte| {
            use std::fmt::Write as _;
            let _ = write!(out, "{byte:02x}");
            out
        })
}

/// # Errors
/// `401` se non autenticato; `403` se l'asset non è visibile; `404` solo per
/// un admin che chiede un id inesistente.
#[utoipa::path(
    get,
    path = "/api/v1/assets/{id}",
    tag = "timeline",
    operation_id = "assets_get",
    summary = "Get an asset",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Id asset")),
    responses(
        (status = 200, description = "Vista pubblica dell'asset", body = AssetView),
        (status = 400, description = "Id non valido", body = Problem),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Asset non visibile", body = Problem),
        (status = 404, description = "Asset inesistente per un admin", body = Problem)
    )
)]
pub async fn asset(
    State(state): State<AppState>,
    SessionNotShare(ctx): SessionNotShare,
    path: Result<Path<AssetId>, PathRejection>,
) -> Result<Json<AssetView>, Problem> {
    let Path(id) = path.map_err(|rejection| {
        Problem::bad_request("invalid-asset-id", "Invalid asset id")
            .with_detail(rejection.body_text())
    })?;
    let asset = AssetRepo::new(&state.db).find_by_id(&ctx, id).await?;
    let effective = OverrideRepo::new(&state.db).effective(&ctx, id).await?;
    let view = AssetView::from_asset(&asset).with_location(
        effective.location.map(GeoPointView::from),
        effective.place_id,
    );
    Ok(Json(view))
}

const fn kind_str(kind: keeppix_domain::AssetKind) -> &'static str {
    match kind {
        keeppix_domain::AssetKind::Image => "image",
        keeppix_domain::AssetKind::RawImage => "raw_image",
        keeppix_domain::AssetKind::Video => "video",
        keeppix_domain::AssetKind::Unknown => "unknown",
    }
}

const fn status_str(status: keeppix_domain::AssetStatus) -> &'static str {
    match status {
        keeppix_domain::AssetStatus::Discovered => "discovered",
        keeppix_domain::AssetStatus::Indexed => "indexed",
        keeppix_domain::AssetStatus::Offline => "offline",
        keeppix_domain::AssetStatus::Error => "error",
        keeppix_domain::AssetStatus::Trashed => "trashed",
    }
}

/// # Errors
/// `401` se non autenticato; `403` se `library` non è del chiamante.
#[utoipa::path(
    get,
    path = "/api/v1/timeline/buckets",
    tag = "timeline",
    operation_id = "timeline_buckets",
    summary = "List timeline buckets",
    security(("session_cookie" = [])),
    params(
        ("library" = Option<String>, Query, description = "Filtra su una libreria"),
        ("bbox" = Option<String>, Query, description = "west,south,east,north WGS84")
    ),
    responses(
        (status = 200, description = "Conteggi mensili visibili", body = [MonthBucketView]),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Libreria non visibile", body = Problem),
        (status = 500, description = "Errore del database", body = Problem)
    )
)]
pub async fn buckets(
    State(state): State<AppState>,
    SessionNotShare(ctx): SessionNotShare,
    Query(query): Query<BucketsQuery>,
) -> Result<Json<Vec<MonthBucketView>>, Problem> {
    let bounds = query
        .bbox
        .as_deref()
        .map(super::map::parse_bounds)
        .transpose()?;
    let repo = TimelineRepo::new(&state.db);
    let buckets = if let Some(bounds) = bounds {
        repo.buckets_in_bounds(&ctx, query.library, bounds).await?
    } else {
        repo.buckets(&ctx, query.library).await?
    };
    Ok(Json(
        buckets
            .into_iter()
            .map(|b| MonthBucketView {
                month: b.month.format("%Y-%m").to_string(),
                count: b.count,
            })
            .collect(),
    ))
}

/// # Errors
/// `400` se `bucket` o `cursor` non sono leggibili; `401` se non autenticato.
#[utoipa::path(
    get,
    path = "/api/v1/timeline",
    tag = "timeline",
    operation_id = "timeline_page",
    summary = "List timeline assets",
    security(("session_cookie" = [])),
    params(
        ("bucket" = String, Query, description = "Mese YYYY-MM"),
        ("cursor" = Option<String>, Query, description = "Keyset taken_at|id"),
        ("limit" = Option<i64>, Query, description = "1..=200, default 200"),
        ("bbox" = Option<String>, Query, description = "west,south,east,north WGS84")
    ),
    responses(
        (status = 200, description = "Pagina di asset del mese", body = TimelinePage),
        (status = 400, description = "bucket o cursor illeggibili", body = Problem),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 500, description = "Errore del database", body = Problem)
    )
)]
pub async fn page(
    State(state): State<AppState>,
    SessionNotShare(ctx): SessionNotShare,
    Query(query): Query<TimelineQuery>,
) -> Result<Json<TimelinePage>, Problem> {
    let bucket = parse_bucket(&query.bucket)?;
    let cursor = match query.cursor.as_deref() {
        None | Some("") => None,
        Some(raw) => Some(parse_cursor(raw)?),
    };
    let limit = query.limit.unwrap_or(200).clamp(1, 200);
    let bounds = query
        .bbox
        .as_deref()
        .map(super::map::parse_bounds)
        .transpose()?;
    let repo = TimelineRepo::new(&state.db);
    let assets = if let Some(bounds) = bounds {
        repo.page_in_bounds(&ctx, bucket, cursor, limit, bounds)
            .await?
    } else {
        repo.page(&ctx, bucket, cursor, limit).await?
    };
    let filled = i64::try_from(assets.len()).unwrap_or(i64::MAX) >= limit;
    let next_cursor = filled.then(|| assets.last().map(encode_cursor)).flatten();
    Ok(Json(TimelinePage {
        assets: assets.iter().map(AssetView::from_asset).collect(),
        next_cursor,
    }))
}

fn parse_bucket(raw: &str) -> Result<NaiveDate, Problem> {
    let padded = if raw.len() == 7 {
        format!("{raw}-01")
    } else {
        raw.to_owned()
    };
    NaiveDate::parse_from_str(&padded, "%Y-%m-%d").map_err(|_| {
        Problem::bad_request("invalid-query", "Invalid timeline bucket").with_detail(raw)
    })
}

fn parse_cursor(raw: &str) -> Result<(DateTime<Utc>, AssetId), Problem> {
    let (time, id) = raw.split_once('|').ok_or_else(|| {
        Problem::bad_request("invalid-query", "Invalid timeline cursor").with_detail(raw)
    })?;
    let taken_at = DateTime::parse_from_rfc3339(time)
        .map(|t| t.with_timezone(&Utc))
        .map_err(|_| {
            Problem::bad_request("invalid-query", "Invalid timeline cursor").with_detail(raw)
        })?;
    let asset_id = id.parse::<AssetId>().map_err(|_| {
        Problem::bad_request("invalid-query", "Invalid timeline cursor").with_detail(raw)
    })?;
    Ok((taken_at, asset_id))
}

pub(crate) fn encode_cursor(asset: &Asset) -> String {
    let taken = asset
        .taken_at_utc
        .unwrap_or(DateTime::<Utc>::UNIX_EPOCH)
        .to_rfc3339_opts(SecondsFormat::Micros, true);
    format!("{taken}|{}", asset.id)
}
