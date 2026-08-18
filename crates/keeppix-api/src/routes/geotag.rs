//! Assegnazioni di posizione che richiedono una sorgente distinta per asset
//! (GPX) o la lettura di una fotografia sorgente (copia).

use axum::extract::State;
use chrono::Duration;
use keeppix_db::OverrideRepo;
use keeppix_domain::{AssetId, LocationSource, OverridePatch};
use serde::Deserialize;

use super::metadata::BatchView;
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
    /// Documento GPX UTF-8 completo.
    pub gpx: String,
    /// Se assente vale cinque minuti. Fuori dal range coperto si usa
    /// l'estremo solo entro questa tolleranza.
    #[schema(example = 5)]
    pub tolerance_minutes: Option<u32>,
}

/// # Errors
/// `400` se la sorgente non ha coordinate; `403` se sorgente o target non
/// sono visibili/modificabili.
#[utoipa::path(
    post,
    path = "/api/v1/metadata/batch/copy-location",
    tag = "metadata",
    operation_id = "metadata_copy_location",
    security(("session_cookie" = [])),
    request_body = CopyLocationRequest,
    responses(
        (status = 200, description = "Posizione effettiva copiata sui target, annullabile", body = BatchView),
        (status = 400, description = "Sorgente senza posizione o batch troppo grande", body = Problem),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Sorgente o target non visibile/modificabile", body = Problem)
    )
)]
pub async fn copy_location(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Json(body): Json<CopyLocationRequest>,
) -> Result<Json<BatchView>, Problem> {
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
    let batch_id = repo
        .apply_location_batch(&ctx, &body.target_asset_ids, &patch, LocationSource::Copied)
        .await?;
    Ok(Json(BatchView { batch_id }))
}

/// # Errors
/// `400` per GPX malformato o batch troppo grande; `403` se un asset non è
/// visibile/modificabile.
#[utoipa::path(
    post,
    path = "/api/v1/metadata/batch/import-gpx",
    tag = "metadata",
    operation_id = "metadata_import_gpx",
    security(("session_cookie" = [])),
    request_body = ImportGpxRequest,
    responses(
        (status = 200, description = "Coordinate GPX abbinate e applicate in un batch annullabile", body = BatchView),
        (status = 400, description = "GPX non valido o batch troppo grande", body = Problem),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Un asset non è visibile/modificabile", body = Problem)
    )
)]
pub async fn import_gpx(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Json(body): Json<ImportGpxRequest>,
) -> Result<Json<BatchView>, Problem> {
    crate::batch::reject_oversized_batch(&body.asset_ids)?;
    let tolerance = body
        .tolerance_minutes
        .map_or(keeppix_media::gpx::DEFAULT_TOLERANCE, |minutes| {
            Duration::minutes(i64::from(minutes))
        });
    let batch_id = keeppix_jobs::geotag::import_gpx(
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
    Ok(Json(BatchView { batch_id }))
}
