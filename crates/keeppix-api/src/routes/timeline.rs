use axum::extract::rejection::PathRejection;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Datelike, NaiveDate, SecondsFormat, Utc};
use keeppix_db::{
    AssetRepo, AssetTagRepo, AssetWithStack, ConfirmedFace, ConfirmedTag, FaceRepo, FlagRepo,
    Geometry, GeometryRecord, GeometryStamp, OverrideRepo, TimelineRepo,
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
    /// 1 se non impilato, altrimenti il numero di file della pila (Task 3
    /// fase-10). Campo additivo.
    pub stack_size: u16,
    /// Badge SP-15: `"raw"` / `"jpeg"` / `"raw+jpeg"`. `None` per un kind
    /// che non è né l'uno né l'altro (video, unknown). Campo additivo.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_kind: Option<String>,
    /// «Preferito» del **chiamante** (Task 10 fase-10). `AssetView` è
    /// condiviso fra molte viste, ma questo campo è per utente: due
    /// chiamanti sullo stesso `Asset` possono leggere valori diversi.
    /// `false` di default finché non viene risolto con
    /// [`Self::with_favorite`] usando il set del chiamante
    /// (`FlagRepo::get`/`favorites_among`). Campo additivo.
    pub favorite: bool,
    /// Fotocamera confermata sull'exif (Fase 11 Task 7, SP-3 §11,
    /// dimensione "Fotocamera"). `None` se l'asset non ha `asset_exif`
    /// leggibile o l'exif non porta il modello. Campo additivo.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub camera_model: Option<String>,
    /// Tag confermati (Fase 11 Task 7, SP-3 §11, dimensioni "Tag"/
    /// "Categorie") — solo `state='confirmed'`, mai proposte in attesa.
    /// Vuoto, mai assente: `[]` se l'asset non ha tag o se pgvector non è
    /// installato. Campo additivo.
    pub tags: Vec<AssetTagBadgeView>,
    /// Volti confermati (Fase 11 Task 7, SP-3 §11, dimensione "Persone") —
    /// `person_id IS NOT NULL AND rejected_at IS NULL`, assegnato a mano o
    /// dal raggruppamento automatico. Vuoto, mai assente. Campo additivo.
    pub faces: Vec<AssetFaceBadgeView>,
    /// EXIF completo (Fase 11 Task 8, §19.2 sezione "SCATTO") — popolato
    /// **solo** da [`asset`] (dettaglio di un asset alla volta, il
    /// lightbox), mai da [`page`]/`/search`: un giro di query in più per
    /// ogni riga di ogni pagina della timeline non serve a nessuno lì.
    /// Campo additivo, sempre assente (non `null`) sulle viste bulk.
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

    /// Come [`Self::from_asset`], ma con il badge di stack vero (Task 3):
    /// usato dalle viste di browse (timeline, ricerca) che restituiscono
    /// solo il primario di ogni pila.
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

/// EXIF completo di un asset (Fase 11 Task 8, §19.2 campi 6-9) — a
/// differenza di [`AssetView::camera_model`] (una stringa sola, per SP-3),
/// il lightbox mostra obiettivo, esposizione (diaframma/tempo/ISO) e
/// focale.
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

/// Un tag confermato, come SP-3 §11 lo mostra: nome e colore per la chip,
/// `category_id` per l'AND fra le dimensioni "Tag"/"Categorie" — mai
/// bisogno di un secondo giro sul tag per risolvere la sua categoria.
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

/// Un volto confermato, come SP-3 §11 lo mostra: solo l'identità della
/// persona (bbox/punteggi non servono a un chip di filtro).
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct AssetFaceBadgeView {
    pub person_id: String,
    /// `None` per una persona senza nome ("Persona 4" — l'etichetta di
    /// fallback è compito del frontend, come già per le altre schermate
    /// Persone).
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

/// Arricchisce una pagina di `AssetWithStack` con tutto ciò che `AssetView`
/// porta per-chiamante o fuori dalla tabella `assets` — preferito,
/// fotocamera, tag e volti confermati (Fase 11 Task 7, SP-3). Una query
/// bulk per ciascuno, mai una per riga (stesso idioma di
/// `FlagRepo::favorites_among`). Estratta qui perché `/timeline` e
/// `/search` costruivano questa stessa sequenza duplicata prima di questa
/// funzione — non duplicata di nuovo aggiungendo i tre nuovi campi.
///
/// # Errors
/// Propaga l'errore del primo bulk-fetch che fallisce.
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

/// Badge di un asset non impilato, derivato dal suo `kind`: nessun aggregato
/// da leggere, `None` solo per un kind che non è né RAW né JPEG (video,
/// unknown) — vedi tabella `raw_kind` nel piano fase-10 Task 3.
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

/// Versione del formato binario di `/timeline/geometry`. Cambia solo se la
/// forma del record cambia (larghezza record, ordine dei campi) — un client
/// vecchio deve poter rifiutare un formato che non capisce invece di leggere
/// byte a caso.
const GEOMETRY_FORMAT_VERSION: u32 = 1;

/// # Errors
/// `401` se non autenticato; `403` se `library` non è del chiamante.
#[utoipa::path(
    get,
    path = "/api/v1/timeline/geometry",
    tag = "timeline",
    operation_id = "timeline_geometry",
    summary = "Get the compact width/height/month geometry of a whole timeline view",
    description = "Un record binario da 6 byte per scatto (w:u16, h:u16, month:u16 = \
                    anno*12+mese), senza identificativo: descrive solo altezze, non \
                    identifica asset. Nessuna paginazione: è la geometria di tutta la \
                    vista, con gli stessi filtri e la stessa visibilità di /timeline. \
                    Supporta 304 via If-None-Match.",
    security(("session_cookie" = [])),
    params(
        ("library" = Option<String>, Query, description = "Filtra su una libreria"),
        ("bbox" = Option<String>, Query, description = "west,south,east,north WGS84")
    ),
    responses(
        (status = 200, description = "8 byte di intestazione (versione, conteggio) + \
                                       N record da 6 byte (w, h, month), little-endian",
         body = [u8]),
        (status = 304, description = "Non modificato rispetto a If-None-Match"),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Libreria non visibile", body = Problem),
        (status = 500, description = "Errore del database", body = Problem)
    )
)]
pub async fn geometry(
    State(state): State<AppState>,
    SessionNotShare(ctx): SessionNotShare,
    Query(query): Query<BucketsQuery>,
    headers: HeaderMap,
) -> Result<Response, Problem> {
    let bounds = query
        .bbox
        .as_deref()
        .map(super::map::parse_bounds)
        .transpose()?;
    let repo = TimelineRepo::new(&state.db);
    // Cache hit is the common case on re-entry: validate ETag with a cheap
    // count+max stamp before paying for the full width/height scan.
    if headers.contains_key(header::IF_NONE_MATCH) {
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
        repo.geometry_in_bounds(&ctx, query.library, bounds).await?
    } else {
        repo.geometry(&ctx, query.library).await?
    };
    let etag = geometry_etag(&geometry);
    let mut response = encode_geometry(&geometry.records).into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    set_etag(&mut response, &etag);
    Ok(response)
}

/// Intestazione da 8 byte (versione `u32`, conteggio `u32`) seguita da un
/// record da 6 byte per scatto (`w:u16`, `h:u16`, `month:u16`), tutto
/// little-endian. Niente uuid: la geometria non identifica nulla, descrive
/// solo altezze (Ruling, spec fase-10 §2.3) — le tessere vere arrivano dalle
/// pagine, nello stesso ordine.
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

/// Un asset non ancora dimensionato (sizing non passato) entra con `0`,
/// invece di essere escluso: altrimenti il layout "salta" quando il sizing
/// arriva (spec fase-10 §2.3, punto 5).
fn saturating_u16(value: Option<i32>) -> u16 {
    value.map_or(0, |v| u16::try_from(v).unwrap_or(u16::MAX))
}

/// `month = anno*12 + mese_di_calendario (1..=12)`. Satura ai margini di
/// `u16` invece di traboccare: una data EXIF corrotta (anno 1 o anno 9999)
/// resta cosmetica, non un panic — la geometria non identifica nulla.
fn month_index(taken_at_utc: DateTime<Utc>) -> u16 {
    let year = i64::from(taken_at_utc.year());
    let month = i64::from(taken_at_utc.month());
    let index = year.saturating_mul(12).saturating_add(month);
    u16::try_from(index.clamp(0, i64::from(u16::MAX))).unwrap_or(u16::MAX)
}

/// `ETag` derivato dal conteggio e dal massimo `updated_at` della vista:
/// rientrare sulla stessa vista senza modifiche rende `304` (spec fase-10
/// §2.3). Non è pensato per essere confrontato fra viste diverse.
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
