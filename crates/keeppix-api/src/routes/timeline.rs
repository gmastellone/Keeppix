use axum::extract::{Query, State};
use chrono::{DateTime, NaiveDate, SecondsFormat, Utc};
use keeppix_db::TimelineRepo;
use keeppix_domain::{Asset, AssetId, LibraryId};
use serde::{Deserialize, Serialize};

use crate::extract::SessionNotShare;
use crate::json::Json;
use crate::problem::Problem;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct BucketsQuery {
    library: Option<LibraryId>,
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
    pub taken_at_utc: Option<DateTime<Utc>>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbhash: Option<String>,
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
        }
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
    security(("session_cookie" = [])),
    params(("library" = Option<String>, Query, description = "Filtra su una libreria")),
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
    let buckets = TimelineRepo::new(&state.db)
        .buckets(&ctx, query.library)
        .await?;
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
    security(("session_cookie" = [])),
    params(
        ("bucket" = String, Query, description = "Mese YYYY-MM"),
        ("cursor" = Option<String>, Query, description = "Keyset taken_at|id"),
        ("limit" = Option<i64>, Query, description = "1..=200, default 200")
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
    let assets = TimelineRepo::new(&state.db)
        .page(&ctx, bucket, cursor, limit)
        .await?;
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
