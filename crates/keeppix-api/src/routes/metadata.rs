//! Metadata editing: effective values (`COALESCE(override, exif)`),
//! applying to a single asset or in batch, shifting the taken-at time, and
//! undo as long as the sidecar has not been written yet. All the logic
//! lives in [`OverrideRepo`]; this module only handles the JSON
//! translation.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use keeppix_db::OverrideRepo;
use keeppix_domain::{
    AssetId, BatchId, EffectiveMetadata, GeoPoint, LibraryId, LocationSource, OverridePatch,
};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize};

use crate::bulk::BulkOutcome;
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

/// Distinguishes "field absent" (don't touch, `#[serde(default)]` covers
/// it) from "field present with `null`" (clear it): without this function,
/// `Option<Option<T>>` would use `Option<T>`'s `Deserialize` over the whole
/// value and collapse the two cases — a client sending `null` to clear
/// `description` would touch nothing. Same problem
/// `serde_with::double_option` solves; rewritten by hand here to avoid
/// adding a dependency for a single use.
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
    /// HTTP cap: [`crate::batch::MAX_BATCH_ASSETS`]. `apply_batch` is
    /// measured at 5,000 assets under a second.
    #[schema(value_type = Vec<String>)]
    pub asset_ids: Vec<AssetId>,
    pub patch: MetadataPatchRequest,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct BatchShiftRequest {
    #[schema(value_type = Vec<String>)]
    pub asset_ids: Vec<AssetId>,
    /// Hours to add to `taken_at`. Can be negative — the fix for a camera
    /// clock left wrong after a trip works in both directions.
    #[schema(example = -2)]
    pub hours: i32,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct RecalculateTimezonesRequest {
    #[schema(value_type = String)]
    pub library_id: LibraryId,
    /// Required on apply; returned by preview. Absent on preview requests.
    #[serde(default)]
    pub preview_token: Option<String>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct TimezoneExampleView {
    #[schema(value_type = String)]
    pub asset_id: AssetId,
    pub filename: String,
    pub before: DateTime<Utc>,
    pub after: DateTime<Utc>,
    pub timezone: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct TimezonePreviewView {
    pub count: usize,
    pub example: Option<TimezoneExampleView>,
    /// Opaque token to pass to the apply endpoint within 5 minutes.
    pub preview_token: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct TimezoneApplyView {
    pub changed_count: usize,
    /// Kept for additive `/api/v1` compatibility; same value as
    /// [`BulkOutcome::batch_id`].
    #[schema(value_type = Option<String>)]
    pub batch_id: Option<BatchId>,
    #[schema(value_type = Vec<String>)]
    pub succeeded: Vec<AssetId>,
    pub failed: Vec<crate::bulk::BulkFailure>,
}

/// # Errors
/// `401` if not authenticated; `403` if the asset is not visible.
#[utoipa::path(
    get,
    path = "/api/v1/assets/{id}/metadata",
    tag = "metadata",
    operation_id = "metadata_effective",
    summary = "Get effective asset metadata",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Asset id")),
    responses(
        (status = 200, description = "COALESCE(override, exif) field by field", body = EffectiveMetadataView),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "Asset not visible", body = Problem)
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
/// `401` if not authenticated; `403` if the asset is not visible.
#[utoipa::path(
    patch,
    path = "/api/v1/assets/{id}/metadata",
    tag = "metadata",
    operation_id = "metadata_apply",
    summary = "Apply metadata overrides to an asset",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Asset id")),
    request_body = MetadataPatchRequest,
    responses(
        (status = 204, description = "Override applied, sidecar queued"),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "Asset not visible", body = Problem)
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
/// `401` if not authenticated; non-editable assets end up in `failed`
/// instead of aborting the batch.
#[utoipa::path(
    post,
    path = "/api/v1/metadata/batch",
    tag = "metadata",
    operation_id = "metadata_apply_batch",
    summary = "Apply metadata overrides in batch",
    security(("session_cookie" = [])),
    request_body = BatchApplyRequest,
    responses(
        (status = 200, description = "Per-asset outcome; batch_id undoable on successes", body = BulkOutcome),
        (status = 400, description = "Batch too large", body = Problem),
        (status = 401, description = "Not authenticated", body = Problem)
    )
)]
pub async fn apply_batch(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Json(body): Json<BatchApplyRequest>,
) -> Result<Json<BulkOutcome>, Problem> {
    crate::batch::reject_oversized_batch(&body.asset_ids)?;
    let mut patch = body.patch.into_domain();
    let source = match (&patch.location, &patch.place_id) {
        (Some(Some(_)), Some(Some(_))) => Some(LocationSource::User),
        (Some(Some(_)), _) => {
            // A free coordinate is not tied to GeoNames: even if the
            // client omits `place_id`, a previous place must be removed.
            patch.place_id = Some(None);
            Some(LocationSource::MapPin)
        }
        _ => None,
    };
    let repo = OverrideRepo::new(&state.db);
    let (batch_id, succeeded, failed) = if let Some(source) = source {
        repo.apply_location_batch_partial(&ctx, &body.asset_ids, &patch, source)
            .await?
    } else {
        repo.apply_batch_partial(&ctx, &body.asset_ids, &patch)
            .await?
    };
    Ok(Json(BulkOutcome::from_partition(
        succeeded, &failed, batch_id,
    )))
}

/// # Errors
/// `401` if not authenticated; non-editable assets end up in `failed`
/// instead of aborting the batch.
#[utoipa::path(
    post,
    path = "/api/v1/metadata/batch/shift-taken-at",
    tag = "metadata",
    operation_id = "metadata_shift_taken_at",
    summary = "Shift taken-at timestamps in batch",
    security(("session_cookie" = [])),
    request_body = BatchShiftRequest,
    responses(
        (status = 200, description = "Per-asset outcome; batch_id undoable on successes", body = BulkOutcome),
        (status = 400, description = "Batch too large", body = Problem),
        (status = 401, description = "Not authenticated", body = Problem)
    )
)]
pub async fn shift_taken_at(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Json(body): Json<BatchShiftRequest>,
) -> Result<Json<BulkOutcome>, Problem> {
    crate::batch::reject_oversized_batch(&body.asset_ids)?;
    let (batch_id, succeeded, failed) = OverrideRepo::new(&state.db)
        .shift_taken_at_partial(&ctx, &body.asset_ids, body.hours)
        .await?;
    Ok(Json(BulkOutcome::from_partition(
        succeeded, &failed, batch_id,
    )))
}

/// # Errors
/// `401` if not authenticated; `403` if the library does not belong to the
/// caller.
#[utoipa::path(
    post,
    path = "/api/v1/metadata/batch/recalculate-timezones/preview",
    tag = "metadata",
    operation_id = "metadata_recalculate_timezones_preview",
    summary = "Preview timezone recalculation",
    security(("session_cookie" = [])),
    request_body = RecalculateTimezonesRequest,
    responses(
        (status = 200, description = "Count and example with no write at all", body = TimezonePreviewView),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "Library not accessible", body = Problem)
    )
)]
pub async fn preview_timezones(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Json(body): Json<RecalculateTimezonesRequest>,
) -> Result<Json<TimezonePreviewView>, Problem> {
    let user_id = ctx.user_id().ok_or_else(Problem::unauthenticated)?;
    let preview = keeppix_jobs::geotag::RecalculateTimezones::new(&state.db)
        .preview(&ctx, body.library_id)
        .await
        .map_err(geotag_problem)?;
    let preview_token = state
        .tz_previews
        .issue(user_id, body.library_id, preview.count);
    Ok(Json(TimezonePreviewView {
        count: preview.count,
        example: preview.example.map(|example| TimezoneExampleView {
            asset_id: example.asset_id,
            filename: example.filename,
            before: example.before,
            after: example.after,
            timezone: example.timezone,
        }),
        preview_token,
    }))
}

/// # Errors
/// `401` if not authenticated; `403` if the library does not belong to the
/// caller.
#[utoipa::path(
    post,
    path = "/api/v1/metadata/batch/recalculate-timezones",
    tag = "metadata",
    operation_id = "metadata_recalculate_timezones_apply",
    summary = "Apply timezone recalculation",
    security(("session_cookie" = [])),
    request_body = RecalculateTimezonesRequest,
    responses(
        (status = 200, description = "Corrections applied in a single undoable batch", body = TimezoneApplyView),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "Library not accessible", body = Problem)
    )
)]
pub async fn apply_timezones(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Json(body): Json<RecalculateTimezonesRequest>,
) -> Result<Json<TimezoneApplyView>, Problem> {
    let user_id = ctx.user_id().ok_or_else(Problem::unauthenticated)?;
    let token = body.preview_token.as_deref().unwrap_or("");
    // Re-run the preview count to detect data drift between preview and apply.
    let current = keeppix_jobs::geotag::RecalculateTimezones::new(&state.db)
        .preview(&ctx, body.library_id)
        .await
        .map_err(geotag_problem)?;
    if !state
        .tz_previews
        .consume(token, user_id, body.library_id, current.count)
    {
        return Err(Problem::new(
            StatusCode::CONFLICT,
            "preview-required",
            "A valid preview token is required before applying timezone changes",
        ));
    }
    let (applied, succeeded) = keeppix_jobs::geotag::RecalculateTimezones::new(&state.db)
        .apply(&ctx, body.library_id)
        .await
        .map_err(geotag_problem)?;
    Ok(Json(TimezoneApplyView {
        changed_count: applied.changed_count,
        batch_id: applied.batch_id,
        succeeded,
        failed: Vec::new(),
    }))
}

fn geotag_problem(error: keeppix_jobs::geotag::GeotagError) -> Problem {
    match error {
        keeppix_jobs::geotag::GeotagError::Db(error) => error.into(),
        keeppix_jobs::geotag::GeotagError::Gpx(error) => {
            Problem::bad_request("invalid-gpx", "Invalid GPX document")
                .with_detail(error.to_string())
        }
    }
}

/// # Errors
/// `401` if not authenticated; `403` if the batch does not belong to the
/// caller; `409` if the sidecar of an asset in the batch has already been
/// written with these values.
#[utoipa::path(
    post,
    path = "/api/v1/metadata/batch/{batch_id}/undo",
    tag = "metadata",
    operation_id = "metadata_undo_batch",
    summary = "Undo a metadata batch",
    security(("session_cookie" = [])),
    params(("batch_id" = String, Path, description = "Id of the batch returned by apply/shift")),
    responses(
        (status = 204, description = "Previous values restored"),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "Batch does not belong to the caller", body = Problem),
        (status = 409, description = "Sidecar already written with this batch's values", body = Problem)
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
