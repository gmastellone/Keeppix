use axum::extract::rejection::PathRejection;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Datelike, NaiveDate, SecondsFormat, Utc};
use keeppix_db::{
    AssetRepo, AssetTagRepo, AssetWithStack, ConfirmedFace, ConfirmedTag, FaceRepo, FlagRepo,
    Geometry, GeometryPage, GeometryRecord, GeometryStamp, OverrideRepo, TimelineRepo,
};
use keeppix_domain::{Asset, AssetId, AssetKind, AuthContext, LibraryId};
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

/// Query for `/timeline/geometry` — a superset of [`BucketsQuery`], not the
/// same type: `limit`/`cursor` make no sense on `/timeline/buckets`.
#[derive(Deserialize)]
pub struct GeometryQuery {
    library: Option<LibraryId>,
    bbox: Option<String>,
    /// Present only on the first cold-screen request: asks for only the
    /// first `limit` shots instead of the whole view, to draw without
    /// waiting for the whole payload on a slow network. Absent →
    /// unchanged behavior, whole view with `ETag` validation.
    limit: Option<i64>,
    /// The opaque cursor from `X-Keeppix-Geometry-Cursor` of the previous
    /// response, as-is — the client does not interpret it, just reports it
    /// back.
    cursor: Option<String>,
}

fn invalid_geometry_cursor() -> Problem {
    Problem::bad_request("invalid-geometry-cursor", "Invalid geometry cursor")
}

fn parse_geometry_cursor(raw: &str) -> Result<(DateTime<Utc>, AssetId), Problem> {
    let (time, id) = raw.split_once(',').ok_or_else(invalid_geometry_cursor)?;
    let time = DateTime::parse_from_rfc3339(time)
        .map_err(|_| invalid_geometry_cursor())?
        .with_timezone(&Utc);
    let id = id
        .parse::<uuid::Uuid>()
        .map_err(|_| invalid_geometry_cursor())?;
    Ok((time, AssetId::from_uuid(id)))
}

fn encode_geometry_cursor((time, id): (DateTime<Utc>, AssetId)) -> String {
    format!(
        "{},{}",
        time.to_rfc3339_opts(SecondsFormat::Micros, true),
        id.as_uuid()
    )
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct MonthBucketView {
    /// Calendar `YYYY-MM`.
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
    /// 1 if not stacked, otherwise the number of files in the stack.
    /// Additive field.
    pub stack_size: u16,
    /// Badge: `"raw"` / `"jpeg"` / `"raw+jpeg"`. `None` for a kind that is
    /// neither (video, unknown). Additive field.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_kind: Option<String>,
    /// "Favorite" of the **caller**. `AssetView` is shared across many
    /// views, but this field is per-user: two callers on the same `Asset`
    /// can read different values. Defaults to `false` until resolved with
    /// [`Self::with_favorite`] using the caller's set
    /// (`FlagRepo::get`/`favorites_among`). Additive field.
    pub favorite: bool,
    /// Camera confirmed from the exif ("Camera" dimension). `None` if the
    /// asset has no readable `asset_exif` or the exif does not carry the
    /// model. Additive field.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub camera_model: Option<String>,
    /// Confirmed tags ("Tags"/"Categories" dimensions) — only
    /// `state='confirmed'`, never pending proposals. Empty, never absent:
    /// `[]` if the asset has no tags or pgvector is not installed. Additive
    /// field.
    pub tags: Vec<AssetTagBadgeView>,
    /// Confirmed faces ("People" dimension) — `person_id IS NOT NULL AND
    /// rejected_at IS NULL`, assigned by hand or by automatic clustering.
    /// Empty, never absent. Additive field.
    pub faces: Vec<AssetFaceBadgeView>,
    /// Full EXIF ("SHOT" section) — populated **only** by [`asset`] (one
    /// asset's detail, the lightbox), never by [`page`]/`/search`: an extra
    /// query round trip for every row of every timeline page serves no one
    /// there. Additive field, always absent (not `null`) on bulk views.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub full_exif: Option<AssetExifDetailView>,
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
            stack_size: 1,
            raw_kind: default_raw_kind(a.kind).map(str::to_owned),
            favorite: false,
            camera_model: None,
            tags: Vec::new(),
            faces: Vec::new(),
            full_exif: None,
        }
    }

    /// Like [`Self::from_asset`], but with the real stack badge: used by
    /// browse views (timeline, search) that return only the primary of
    /// each stack.
    pub(crate) fn from_asset_with_stack(a: &AssetWithStack) -> Self {
        let mut view = Self::from_asset(&a.asset);
        view.stack_size = a.stack.stack_size;
        view.raw_kind.clone_from(&a.stack.raw_kind);
        view
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

    pub(crate) fn with_favorite(mut self, favorite: bool) -> Self {
        self.favorite = favorite;
        self
    }

    pub(crate) fn with_camera(mut self, camera_model: Option<String>) -> Self {
        self.camera_model = camera_model;
        self
    }

    pub(crate) fn with_tags(mut self, tags: Vec<AssetTagBadgeView>) -> Self {
        self.tags = tags;
        self
    }

    pub(crate) fn with_faces(mut self, faces: Vec<AssetFaceBadgeView>) -> Self {
        self.faces = faces;
        self
    }

    pub(crate) fn with_full_exif(mut self, full_exif: Option<AssetExifDetailView>) -> Self {
        self.full_exif = full_exif;
        self
    }
}

/// Full EXIF of an asset — unlike [`AssetView::camera_model`] (a single
/// string), the lightbox shows lens, exposure (aperture/shutter/ISO), and
/// focal length.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct AssetExifDetailView {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub camera_make: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub camera_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lens: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iso: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub f_number: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exposure: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub focal_length: Option<f32>,
}

impl From<keeppix_db::AssetExifDetail> for AssetExifDetailView {
    fn from(e: keeppix_db::AssetExifDetail) -> Self {
        Self {
            camera_make: e.camera_make,
            camera_model: e.camera_model,
            lens: e.lens,
            iso: e.iso,
            f_number: e.f_number,
            exposure: e.exposure,
            focal_length: e.focal_length,
        }
    }
}

/// A confirmed tag, as the UI shows it: name and color for the chip,
/// `category_id` for the AND between the "Tags"/"Categories" dimensions —
/// never a need for a second round trip on the tag to resolve its
/// category.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct AssetTagBadgeView {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category_id: Option<String>,
}

impl From<ConfirmedTag> for AssetTagBadgeView {
    fn from(tag: ConfirmedTag) -> Self {
        Self {
            id: tag.tag_id.to_string(),
            name: tag.name,
            color: tag.color,
            category_id: tag.category_id.map(|id| id.to_string()),
        }
    }
}

/// A confirmed face, as the UI shows it: only the identity of the person
/// (bbox/scores are not needed for a filter chip).
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct AssetFaceBadgeView {
    pub person_id: String,
    /// `None` for an unnamed person ("Person 4" — the fallback label is
    /// the frontend's job, as with the other People screens).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub person_name: Option<String>,
}

impl From<ConfirmedFace> for AssetFaceBadgeView {
    fn from(face: ConfirmedFace) -> Self {
        Self {
            person_id: face.person_id.to_string(),
            person_name: face.person_name,
        }
    }
}

/// Enriches a page of `AssetWithStack` with everything `AssetView` carries
/// per-caller or outside the `assets` table — favorite, camera, confirmed
/// tags and faces. One bulk query for each, never one per row (same idiom
/// as `FlagRepo::favorites_among`). Extracted here because `/timeline` and
/// `/search` used to build this same sequence in duplicate before this
/// function existed — not duplicated again when adding the three newer
/// fields.
///
/// # Errors
/// Propagates the error of the first bulk fetch that fails.
pub(crate) async fn enrich_views(
    state: &AppState,
    ctx: &AuthContext,
    assets: &[AssetWithStack],
) -> Result<Vec<AssetView>, Problem> {
    let ids: Vec<AssetId> = assets.iter().map(|a| a.id).collect();
    let favorites = FlagRepo::new(&state.db).favorites_among(ctx, &ids).await?;
    let cameras = AssetRepo::new(&state.db).camera_models_among(&ids).await?;
    let tags = AssetTagRepo::new(&state.db).confirmed_among(&ids).await?;
    let faces = FaceRepo::new(&state.db).confirmed_among(&ids).await?;
    Ok(assets
        .iter()
        .map(|a| {
            AssetView::from_asset_with_stack(a)
                .with_favorite(favorites.contains(&a.id))
                .with_camera(cameras.get(&a.id).cloned())
                .with_tags(
                    tags.get(&a.id)
                        .cloned()
                        .unwrap_or_default()
                        .into_iter()
                        .map(Into::into)
                        .collect(),
                )
                .with_faces(
                    faces
                        .get(&a.id)
                        .cloned()
                        .unwrap_or_default()
                        .into_iter()
                        .map(Into::into)
                        .collect(),
                )
        })
        .collect())
}

/// Badge of a non-stacked asset, derived from its `kind`: no aggregate to
/// read, `None` only for a kind that is neither RAW nor JPEG (video,
/// unknown).
const fn default_raw_kind(kind: AssetKind) -> Option<&'static str> {
    match kind {
        AssetKind::RawImage => Some("raw"),
        AssetKind::Image => Some("jpeg"),
        AssetKind::Video | AssetKind::Unknown => None,
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
/// `401` if not authenticated; `403` if the asset is not visible; `404`
/// only for an admin requesting a nonexistent id.
#[utoipa::path(
    get,
    path = "/api/v1/assets/{id}",
    tag = "timeline",
    operation_id = "assets_get",
    summary = "Get an asset",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Asset id")),
    responses(
        (status = 200, description = "Public view of the asset", body = AssetView),
        (status = 400, description = "Invalid id", body = Problem),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "Asset not visible", body = Problem),
        (status = 404, description = "Asset does not exist for an admin", body = Problem)
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
    let flags = FlagRepo::new(&state.db).get(&ctx, id).await?;
    let ids = [id];
    let camera = AssetRepo::new(&state.db)
        .camera_models_among(&ids)
        .await?
        .remove(&id);
    let tags = AssetTagRepo::new(&state.db).confirmed_among(&ids).await?;
    let faces = FaceRepo::new(&state.db).confirmed_among(&ids).await?;
    let full_exif = AssetRepo::new(&state.db).exif_for(id).await?;
    let view = AssetView::from_asset(&asset)
        .with_location(
            effective.location.map(GeoPointView::from),
            effective.place_id,
        )
        .with_favorite(flags.favorite)
        .with_camera(camera)
        .with_tags(
            tags.get(&id)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .map(Into::into)
                .collect(),
        )
        .with_faces(
            faces
                .get(&id)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .map(Into::into)
                .collect(),
        )
        .with_full_exif(full_exif.map(AssetExifDetailView::from));
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
/// `401` if not authenticated; `403` if `library` does not belong to the
/// caller.
#[utoipa::path(
    get,
    path = "/api/v1/timeline/buckets",
    tag = "timeline",
    operation_id = "timeline_buckets",
    summary = "List timeline buckets",
    security(("session_cookie" = [])),
    params(
        ("library" = Option<String>, Query, description = "Filter to a library"),
        ("bbox" = Option<String>, Query, description = "west,south,east,north WGS84")
    ),
    responses(
        (status = 200, description = "Visible monthly counts", body = [MonthBucketView]),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "Library not visible", body = Problem),
        (status = 500, description = "Database error", body = Problem)
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

/// Version of `/timeline/geometry`'s binary format. Changes only if the
/// record shape changes (record width, field order) — an old client must
/// be able to reject a format it doesn't understand instead of reading
/// random bytes.
const GEOMETRY_FORMAT_VERSION: u32 = 1;

/// Name of the header carrying the opaque cursor for the next page — never
/// in the binary body, which stays without identifiers by construction.
const GEOMETRY_CURSOR_HEADER: &str = "x-keeppix-geometry-cursor";

/// # Errors
/// `400` if `cursor` is invalid; `401` if not authenticated; `403` if
/// `library` does not belong to the caller.
#[utoipa::path(
    get,
    path = "/api/v1/timeline/geometry",
    tag = "timeline",
    operation_id = "timeline_geometry",
    summary = "Get the compact width/height/month geometry of a timeline view, in full or paged",
    description = "A 6-byte binary record per shot (w:u16, h:u16, month:u16 = \
                    year*12+month), with no identifier: it only describes heights, it does \
                    not identify assets. Without `limit`: the whole view, with the same \
                    filters and the same visibility as /timeline, and 304 support via \
                    If-None-Match. With `limit`: only the first N shots (cold screen on a \
                    slow network) — if there is more, the response carries the \
                    x-keeppix-geometry-cursor header to pass as `cursor` on the next \
                    request; no header = that was the whole view. Paged requests do not \
                    validate If-None-Match: they are meant for the first load, not for \
                    returning to an unchanged view.",
    security(("session_cookie" = [])),
    params(
        ("library" = Option<String>, Query, description = "Filter to a library"),
        ("bbox" = Option<String>, Query, description = "west,south,east,north WGS84"),
        ("limit" = Option<i64>, Query, description = "Only the first N shots, instead of the whole view"),
        ("cursor" = Option<String>, Query, description = "Opaque cursor from the previous page")
    ),
    responses(
        (status = 200, description = "8-byte header (version, count) + \
                                       N 6-byte records (w, h, month), little-endian",
         body = [u8]),
        (status = 304, description = "Not modified relative to If-None-Match"),
        (status = 400, description = "Invalid cursor", body = Problem),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "Library not visible", body = Problem),
        (status = 500, description = "Database error", body = Problem)
    )
)]
pub async fn geometry(
    State(state): State<AppState>,
    SessionNotShare(ctx): SessionNotShare,
    Query(query): Query<GeometryQuery>,
    headers: HeaderMap,
) -> Result<Response, Problem> {
    let bounds = query
        .bbox
        .as_deref()
        .map(super::map::parse_bounds)
        .transpose()?;
    let page = query
        .limit
        .map(|limit| {
            let after = query
                .cursor
                .as_deref()
                .map(parse_geometry_cursor)
                .transpose()?;
            Ok::<_, Problem>(GeometryPage { limit, after })
        })
        .transpose()?;
    let repo = TimelineRepo::new(&state.db);
    // A paged request has no stable view to validate with If-None-Match:
    // it's for the first cold load, not for returning to a view. Only the
    // whole-view request pays for validation.
    if page.is_none() && headers.contains_key(header::IF_NONE_MATCH) {
        let stamp = if let Some(bounds) = bounds {
            repo.geometry_stamp_in_bounds(&ctx, query.library, bounds)
                .await?
        } else {
            repo.geometry_stamp(&ctx, query.library).await?
        };
        let etag = stamp_etag(&stamp);
        if if_none_match_matches(&headers, &etag) {
            let mut response = StatusCode::NOT_MODIFIED.into_response();
            set_etag(&mut response, &etag);
            return Ok(response);
        }
    }
    let geometry = if let Some(bounds) = bounds {
        repo.geometry_in_bounds(&ctx, query.library, bounds, page)
            .await?
    } else {
        repo.geometry(&ctx, query.library, page).await?
    };
    let next_cursor = geometry.next_cursor;
    let mut response = encode_geometry(&geometry.records).into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    if page.is_none() {
        set_etag(&mut response, &geometry_etag(&geometry));
    }
    if let Some(cursor) = next_cursor
        && let Ok(value) = HeaderValue::from_str(&encode_geometry_cursor(cursor))
    {
        response.headers_mut().insert(GEOMETRY_CURSOR_HEADER, value);
    }
    Ok(response)
}

/// 8-byte header (`u32` version, `u32` count) followed by a 6-byte record
/// per shot (`w:u16`, `h:u16`, `month:u16`), all little-endian. No uuid:
/// the geometry identifies nothing, it only describes heights — the real
/// tiles arrive from the pages, in the same order.
fn encode_geometry(records: &[GeometryRecord]) -> Vec<u8> {
    let count = u32::try_from(records.len()).unwrap_or(u32::MAX);
    let mut out = Vec::with_capacity(8 + records.len() * 6);
    out.extend_from_slice(&GEOMETRY_FORMAT_VERSION.to_le_bytes());
    out.extend_from_slice(&count.to_le_bytes());
    for record in records {
        let w = saturating_u16(record.width);
        let h = saturating_u16(record.height);
        let m = month_index(record.taken_at_utc);
        out.extend_from_slice(&w.to_le_bytes());
        out.extend_from_slice(&h.to_le_bytes());
        out.extend_from_slice(&m.to_le_bytes());
    }
    out
}

/// An asset not yet sized (sizing not passed) enters with `0`, instead of
/// being excluded: otherwise the layout "jumps" once sizing arrives.
fn saturating_u16(value: Option<i32>) -> u16 {
    value.map_or(0, |v| u16::try_from(v).unwrap_or(u16::MAX))
}

/// `month = year*12 + calendar_month (1..=12)`. Saturates at `u16`'s
/// bounds instead of overflowing: a corrupted EXIF date (year 1 or year
/// 9999) stays cosmetic, not a panic — the geometry identifies nothing.
fn month_index(taken_at_utc: DateTime<Utc>) -> u16 {
    let year = i64::from(taken_at_utc.year());
    let month = i64::from(taken_at_utc.month());
    let index = year.saturating_mul(12).saturating_add(month);
    u16::try_from(index.clamp(0, i64::from(u16::MAX))).unwrap_or(u16::MAX)
}

/// `ETag` derived from the count and the maximum `updated_at` of the view:
/// returning to the same unchanged view yields `304`. Not meant to be
/// compared across different views.
fn geometry_etag(geometry: &Geometry) -> String {
    stamp_etag(&GeometryStamp {
        count: u64::try_from(geometry.records.len()).unwrap_or(u64::MAX),
        last_modified: geometry.last_modified,
    })
}

fn stamp_etag(stamp: &GeometryStamp) -> String {
    let micros = stamp.last_modified.map_or(0, |t| t.timestamp_micros());
    format!("\"{:x}-{micros:x}\"", stamp.count)
}

fn set_etag(response: &mut Response, etag: &str) {
    if let Ok(value) = HeaderValue::from_str(etag) {
        response.headers_mut().insert(header::ETAG, value);
    }
}

fn if_none_match_matches(headers: &HeaderMap, etag: &str) -> bool {
    let Some(value) = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
    else {
        return false;
    };
    value
        .split(',')
        .map(str::trim)
        .any(|candidate| candidate == "*" || candidate == etag)
}

/// # Errors
/// `400` if `bucket` or `cursor` are unreadable; `401` if not authenticated.
#[utoipa::path(
    get,
    path = "/api/v1/timeline",
    tag = "timeline",
    operation_id = "timeline_page",
    summary = "List timeline assets",
    security(("session_cookie" = [])),
    params(
        ("bucket" = String, Query, description = "Month YYYY-MM"),
        ("cursor" = Option<String>, Query, description = "Keyset taken_at|id"),
        ("limit" = Option<i64>, Query, description = "1..=200, default 200"),
        ("bbox" = Option<String>, Query, description = "west,south,east,north WGS84")
    ),
    responses(
        (status = 200, description = "Page of assets for the month", body = TimelinePage),
        (status = 400, description = "Unreadable bucket or cursor", body = Problem),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 500, description = "Database error", body = Problem)
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
    let next_cursor = filled
        .then(|| assets.last().map(|a| encode_cursor(a)))
        .flatten();
    Ok(Json(TimelinePage {
        assets: enrich_views(&state, &ctx, &assets).await?,
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
