//! Editing dei metadati (spec §3): valori effettivi (`COALESCE(override,
//! exif)`), applicazione a un singolo asset o in blocco, scostamento
//! dell'ora di scatto, e annullamento finché il sidecar non è stato scritto.
//! Tutta la logica vive in [`OverrideRepo`]; qui solo la traduzione da/verso
//! JSON.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use keeppix_db::OverrideRepo;
use keeppix_domain::{AssetId, BatchId, EffectiveMetadata, GeoPoint, OverridePatch};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize};

use crate::extract::Auth;
use crate::json::Json;
use crate::problem::Problem;
use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct GeoPointView {
    pub lat: f64,
    pub lon: f64,
}

impl From<GeoPoint> for GeoPointView {
    fn from(p: GeoPoint) -> Self {
        Self {
            lat: p.lat,
            lon: p.lon,
        }
    }
}

impl From<GeoPointView> for GeoPoint {
    fn from(v: GeoPointView) -> Self {
        Self {
            lat: v.lat,
            lon: v.lon,
        }
    }
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct EffectiveMetadataView {
    pub title: Option<String>,
    pub description: Option<String>,
    pub taken_at: Option<DateTime<Utc>>,
    pub location: Option<GeoPointView>,
    pub place_id: Option<i64>,
    pub orientation: Option<i16>,
}

impl From<EffectiveMetadata> for EffectiveMetadataView {
    fn from(m: EffectiveMetadata) -> Self {
        Self {
            title: m.title,
            description: m.description,
            taken_at: m.taken_at,
            location: m.location.map(GeoPointView::from),
            place_id: m.place_id,
            orientation: m.orientation,
        }
    }
}

/// Distingue "campo assente" (non toccare, `#[serde(default)]` lo copre) da
/// "campo presente con `null`" (azzera): senza questa funzione,
/// `Option<Option<T>>` userebbe la `Deserialize` di `Option<T>` sull'intero
/// valore e collasserebbe i due casi — un client che manda `null` per
/// azzerare `description` non toccherebbe nulla. Stesso problema che
/// `serde_with::double_option` risolve; qui è riscritto a mano per non
/// aggiungere una dipendenza a un solo usarlo.
#[allow(clippy::option_option)]
fn double_option<'de, T, D>(de: D) -> Result<Option<Option<T>>, D::Error>
where
    T: DeserializeOwned,
    D: Deserializer<'de>,
{
    Option::<T>::deserialize(de).map(Some)
}

#[allow(clippy::option_option)]
#[derive(Debug, Clone, Default, Deserialize, utoipa::ToSchema)]
pub struct MetadataPatchRequest {
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<String>, nullable)]
    pub title: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<String>, nullable)]
    pub description: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<String>, nullable)]
    pub taken_at: Option<Option<DateTime<Utc>>>,
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<GeoPointView>, nullable)]
    pub location: Option<Option<GeoPointView>>,
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<i64>, nullable)]
    pub place_id: Option<Option<i64>>,
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<i16>, nullable)]
    pub orientation: Option<Option<i16>>,
}

impl MetadataPatchRequest {
    fn into_domain(self) -> OverridePatch {
        OverridePatch {
            title: self.title,
            description: self.description,
            taken_at: self.taken_at,
            location: self
                .location
                .map(|inner| inner.map(std::convert::Into::into)),
            place_id: self.place_id,
            orientation: self.orientation,
        }
    }
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct BatchApplyRequest {
    /// Fino a qualche migliaio di id: `apply_batch` è pensato per restare
    /// sotto un secondo anche a 5.000 (misurato, vedi il report del Task 8).
    #[schema(value_type = Vec<String>)]
    pub asset_ids: Vec<AssetId>,
    pub patch: MetadataPatchRequest,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct BatchShiftRequest {
    #[schema(value_type = Vec<String>)]
    pub asset_ids: Vec<AssetId>,
    /// Ore da sommare a `taken_at`. Può essere negativo — il rimedio per
    /// l'orologio della fotocamera sbagliato dopo un viaggio funziona in
    /// entrambe le direzioni.
    #[schema(example = -2)]
    pub hours: i32,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct BatchView {
    #[schema(value_type = String)]
    pub batch_id: BatchId,
}

/// # Errors
/// `401` se non autenticato; `403` se l'asset non è visibile.
#[utoipa::path(
    get,
    path = "/api/v1/assets/{id}/metadata",
    tag = "metadata",
    operation_id = "metadata_effective",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Id dell'asset")),
    responses(
        (status = 200, description = "COALESCE(override, exif) campo per campo", body = EffectiveMetadataView),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Asset non visibile", body = Problem)
    )
)]
pub async fn effective(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Path(id): Path<AssetId>,
) -> Result<Json<EffectiveMetadataView>, Problem> {
    let metadata = OverrideRepo::new(&state.db).effective(&ctx, id).await?;
    Ok(Json(metadata.into()))
}

/// # Errors
/// `401` se non autenticato; `403` se l'asset non è visibile.
#[utoipa::path(
    patch,
    path = "/api/v1/assets/{id}/metadata",
    tag = "metadata",
    operation_id = "metadata_apply",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Id dell'asset")),
    request_body = MetadataPatchRequest,
    responses(
        (status = 204, description = "Override applicato, sidecar accodato"),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Asset non visibile", body = Problem)
    )
)]
pub async fn apply(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Path(id): Path<AssetId>,
    Json(body): Json<MetadataPatchRequest>,
) -> Result<StatusCode, Problem> {
    OverrideRepo::new(&state.db)
        .apply(&ctx, id, &body.into_domain())
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// # Errors
/// `401` se non autenticato; `403` se anche un solo asset non è visibile.
#[utoipa::path(
    post,
    path = "/api/v1/metadata/batch",
    tag = "metadata",
    operation_id = "metadata_apply_batch",
    security(("session_cookie" = [])),
    request_body = BatchApplyRequest,
    responses(
        (status = 200, description = "Stesso patch applicato a ogni asset, annullabile", body = BatchView),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Un asset non è visibile", body = Problem)
    )
)]
pub async fn apply_batch(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Json(body): Json<BatchApplyRequest>,
) -> Result<Json<BatchView>, Problem> {
    let batch_id = OverrideRepo::new(&state.db)
        .apply_batch(&ctx, &body.asset_ids, &body.patch.into_domain())
        .await?;
    Ok(Json(BatchView { batch_id }))
}

/// # Errors
/// `401` se non autenticato; `403` se anche un solo asset non è visibile.
#[utoipa::path(
    post,
    path = "/api/v1/metadata/batch/shift-taken-at",
    tag = "metadata",
    operation_id = "metadata_shift_taken_at",
    security(("session_cookie" = [])),
    request_body = BatchShiftRequest,
    responses(
        (status = 200, description = "taken_at spostato di N ore per ogni asset, annullabile", body = BatchView),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Un asset non è visibile", body = Problem)
    )
)]
pub async fn shift_taken_at(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Json(body): Json<BatchShiftRequest>,
) -> Result<Json<BatchView>, Problem> {
    let batch_id = OverrideRepo::new(&state.db)
        .shift_taken_at(&ctx, &body.asset_ids, body.hours)
        .await?;
    Ok(Json(BatchView { batch_id }))
}

/// # Errors
/// `401` se non autenticato; `403` se il batch non è del chiamante; `409` se
/// il sidecar di un asset del batch è già stato scritto con questi valori.
#[utoipa::path(
    post,
    path = "/api/v1/metadata/batch/{batch_id}/undo",
    tag = "metadata",
    operation_id = "metadata_undo_batch",
    security(("session_cookie" = [])),
    params(("batch_id" = String, Path, description = "Id del batch restituito da apply/shift")),
    responses(
        (status = 204, description = "Valori precedenti ripristinati"),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Batch non del chiamante", body = Problem),
        (status = 409, description = "Sidecar già scritto con i valori di questo batch", body = Problem)
    )
)]
pub async fn undo_batch(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Path(batch_id): Path<BatchId>,
) -> Result<StatusCode, Problem> {
    OverrideRepo::new(&state.db)
        .undo_batch(&ctx, batch_id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
