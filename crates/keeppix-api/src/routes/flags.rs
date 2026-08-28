//! Per-user culling votes: rating, pick/reject, color label. All the
//! visibility and "per-user" logic lives in [`FlagRepo`]; this module only
//! handles the JSON translation and out-of-range rating validation, which
//! becomes a `400` here instead of propagating `DomainError` down to the
//! database.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use keeppix_db::FlagRepo;
use keeppix_domain::{AssetFlags, AssetId, Pick, Rating};
use serde::{Deserialize, Serialize};

use crate::bulk::BulkOutcome;
use crate::extract::Auth;
use crate::json::Json;
use crate::problem::Problem;
use crate::state::AppState;

#[derive(Debug, Clone, Default, Serialize, Deserialize, utoipa::ToSchema)]
pub struct AssetFlagsBody {
    /// 0..=5. `null` (or absent on write) means "no vote".
    #[schema(example = 4, minimum = 0, maximum = 5)]
    pub rating: Option<u8>,
    #[serde(default)]
    #[schema(value_type = String, example = "pick")]
    pub pick: Pick,
    pub color_label: Option<String>,
    /// "Favorite": an axis independent from `pick`, not an alias for it.
    /// `false` if absent on write, like the other fields of this body — it
    /// is a full replacement of the vote, not a patch.
    #[serde(default)]
    pub favorite: bool,
}

impl AssetFlagsBody {
    fn into_domain(self) -> Result<AssetFlags, Problem> {
        let rating = self.rating.map(Rating::parse).transpose().map_err(|e| {
            Problem::bad_request("invalid-rating", "rating must be between 0 and 5")
                .with_detail(e.to_string())
        })?;
        Ok(AssetFlags {
            rating,
            pick: self.pick,
            color_label: self.color_label,
            favorite: self.favorite,
        })
    }

    fn from_domain(flags: AssetFlags) -> Self {
        Self {
            rating: flags.rating.map(Rating::value),
            pick: flags.pick,
            color_label: flags.color_label,
            favorite: flags.favorite,
        }
    }
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct BatchFlagsRequest {
    #[schema(value_type = Vec<String>)]
    pub asset_ids: Vec<AssetId>,
    #[serde(flatten)]
    pub flags: AssetFlagsBody,
}

/// # Errors
/// `401` if not authenticated; `403` if the asset is not visible.
#[utoipa::path(
    get,
    path = "/api/v1/assets/{id}/flags",
    tag = "flags",
    operation_id = "flags_get",
    summary = "Get asset flags",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Asset id")),
    responses(
        (status = 200, description = "Caller's flags on this asset, or the defaults if they have not voted", body = AssetFlagsBody),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "Asset not visible", body = Problem)
    )
)]
pub async fn get(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Path(id): Path<AssetId>,
) -> Result<Json<AssetFlagsBody>, Problem> {
    let flags = FlagRepo::new(&state.db).get(&ctx, id).await?;
    Ok(Json(AssetFlagsBody::from_domain(flags)))
}

/// # Errors
/// `400` if `rating` exceeds 5; `401` if not authenticated; `403` if the
/// asset is not visible.
#[utoipa::path(
    put,
    path = "/api/v1/assets/{id}/flags",
    tag = "flags",
    operation_id = "flags_set",
    summary = "Set asset flags",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Asset id")),
    request_body = AssetFlagsBody,
    responses(
        (status = 204, description = "Caller's flags updated"),
        (status = 400, description = "rating out of range", body = Problem),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "Asset not visible", body = Problem)
    )
)]
pub async fn set(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Path(id): Path<AssetId>,
    Json(body): Json<AssetFlagsBody>,
) -> Result<StatusCode, Problem> {
    let flags = body.into_domain()?;
    FlagRepo::new(&state.db).set(&ctx, id, &flags).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// # Errors
/// `400` if `rating` exceeds 5; `401` if not authenticated.
#[utoipa::path(
    post,
    path = "/api/v1/flags/batch",
    tag = "flags",
    operation_id = "flags_batch_set",
    summary = "Set flags on multiple assets",
    security(("session_cookie" = [])),
    request_body = BatchFlagsRequest,
    responses(
        (status = 200, description = "Per-asset outcome (partial success allowed)", body = BulkOutcome),
        (status = 400, description = "rating out of range or batch too large", body = Problem),
        (status = 401, description = "Not authenticated", body = Problem)
    )
)]
pub async fn batch_set(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Json(body): Json<BatchFlagsRequest>,
) -> Result<Json<BulkOutcome>, Problem> {
    crate::batch::reject_oversized_batch(&body.asset_ids)?;
    let flags = body.flags.into_domain()?;
    let (succeeded, failed) = FlagRepo::new(&state.db)
        .batch_set_partial(&ctx, &body.asset_ids, &flags)
        .await?;
    Ok(Json(BulkOutcome::from_partition(succeeded, &failed, None)))
}
