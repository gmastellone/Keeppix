//! Location assignments that require a distinct per-asset source (GPX) or
//! reading a source photo (copy).

use axum::extract::State;
use chrono::Duration;
use keeppix_db::OverrideRepo;
use keeppix_domain::{AssetId, LocationSource, OverridePatch};
use serde::Deserialize;

use crate::bulk::BulkOutcome;
use crate::extract::Auth;
use crate::json::Json;
use crate::problem::Problem;
use crate::state::AppState;

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CopyLocationRequest {
    #[schema(value_type = String)]
    pub source_asset_id: AssetId,
    #[schema(value_type = Vec<String>)]
    pub target_asset_ids: Vec<AssetId>,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct ImportGpxRequest {
    #[schema(value_type = Vec<String>)]
    pub asset_ids: Vec<AssetId>,
    /// Full UTF-8 GPX document.
    pub gpx: String,
    /// Defaults to five minutes if absent. Outside the covered range, the
    /// endpoint is used only within this tolerance.
    #[schema(example = 5)]
    pub tolerance_minutes: Option<u32>,
}

/// # Errors
/// `400` if the source has no coordinates; `403` if the source is not
/// visible. Non-editable targets end up in `failed`.
#[utoipa::path(
    post,
    path = "/api/v1/metadata/batch/copy-location",
    tag = "metadata",
    operation_id = "metadata_copy_location",
    summary = "Copy location metadata between assets",
    security(("session_cookie" = [])),
    request_body = CopyLocationRequest,
    responses(
        (status = 200, description = "Per-target outcome; batch_id undoable on successes", body = BulkOutcome),
        (status = 400, description = "Source has no location, or batch too large", body = Problem),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "Source not visible", body = Problem)
    )
)]
pub async fn copy_location(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Json(body): Json<CopyLocationRequest>,
) -> Result<Json<BulkOutcome>, Problem> {
    crate::batch::reject_oversized_batch(&body.target_asset_ids)?;
    let repo = OverrideRepo::new(&state.db);
    let source = repo.effective(&ctx, body.source_asset_id).await?;
    let location = source.location.ok_or_else(|| {
        Problem::bad_request(
            "source-has-no-location",
            "Source asset has no location to copy",
        )
    })?;
    let patch = OverridePatch {
        location: Some(Some(location)),
        place_id: Some(source.place_id),
        ..Default::default()
    };
    let (batch_id, succeeded, failed) = repo
        .apply_location_batch_partial(&ctx, &body.target_asset_ids, &patch, LocationSource::Copied)
        .await?;
    Ok(Json(BulkOutcome::from_partition(
        succeeded, &failed, batch_id,
    )))
}

/// # Errors
/// `400` for malformed GPX or a batch too large; non-editable assets end
/// up in `failed`.
#[utoipa::path(
    post,
    path = "/api/v1/metadata/batch/import-gpx",
    tag = "metadata",
    operation_id = "metadata_import_gpx",
    summary = "Import GPX location data",
    security(("session_cookie" = [])),
    request_body = ImportGpxRequest,
    responses(
        (status = 200, description = "Per-asset outcome; batch_id undoable on the geotagged ones", body = BulkOutcome),
        (status = 400, description = "Invalid GPX or batch too large", body = Problem),
        (status = 401, description = "Not authenticated", body = Problem)
    )
)]
pub async fn import_gpx(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Json(body): Json<ImportGpxRequest>,
) -> Result<Json<BulkOutcome>, Problem> {
    crate::batch::reject_oversized_batch(&body.asset_ids)?;
    let tolerance = body
        .tolerance_minutes
        .map_or(keeppix_media::gpx::DEFAULT_TOLERANCE, |minutes| {
            Duration::minutes(i64::from(minutes))
        });
    let outcome = keeppix_jobs::geotag::import_gpx(
        &state.db,
        &ctx,
        &body.asset_ids,
        body.gpx.as_bytes(),
        tolerance,
    )
    .await
    .map_err(|error| match error {
        keeppix_jobs::geotag::GeotagError::Gpx(error) => {
            Problem::bad_request("invalid-gpx", "Invalid GPX document")
                .with_detail(error.to_string())
        }
        keeppix_jobs::geotag::GeotagError::Db(error) => error.into(),
    })?;
    Ok(Json(BulkOutcome::from_partition(
        outcome.succeeded,
        &outcome.failed,
        outcome.batch_id,
    )))
}
