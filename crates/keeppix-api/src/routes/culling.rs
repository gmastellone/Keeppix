//! HTTP surface for folder-based culling. No permission check of its own:
//! every handler propagates [`keeppix_db::DbError`] from [`CullingRepo`],
//! which already embeds the right gate for each operation — owner/admin via
//! `LibraryRepo::find_by_id` for reading lots, `editor` on both folders via
//! `AssetRepo::move_asset` for the physical move, owner/admin via
//! `TrashRepo::assert_batch_purge_authorized` for emptying. Reinventing a
//! second check here would risk telling a different story from the one
//! that actually decides.

use axum::extract::{Path, State};
use keeppix_db::CullingRepo;
use keeppix_domain::{AssetId, FolderId, LibraryId, Pick};
use serde::{Deserialize, Serialize};

use crate::bulk::BulkOutcome;
use crate::extract::Auth;
use crate::json::Json;
use crate::problem::Problem;
use crate::routes::timeline::AssetView;
use crate::state::AppState;

#[derive(Serialize, utoipa::ToSchema)]
pub struct CullingLotView {
    pub folder_id: String,
    pub name: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub pending: i64,
    pub taken: i64,
    pub skipped: i64,
}

impl CullingLotView {
    fn from_lot(lot: &keeppix_domain::CullingLot) -> Self {
        Self {
            folder_id: lot.folder_id.to_string(),
            name: lot.name.clone(),
            created_at: lot.created_at,
            pending: lot.pending,
            taken: lot.taken,
            skipped: lot.skipped,
        }
    }
}

/// The lots under the library's culling root. Empty — not an error — if
/// the library does not yet have a designated root.
///
/// # Errors
/// `401` if not authenticated; `403` if the caller cannot see the library,
/// or sees it but is not its owner/admin.
#[utoipa::path(
    get,
    path = "/api/v1/libraries/{id}/culling/lots",
    tag = "culling",
    operation_id = "culling_list_lots",
    summary = "List culling lots for a library",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Library id")),
    responses(
        (status = 200, description = "Lots, most recent first", body = [CullingLotView]),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "Not owner/admin of this library", body = Problem)
    )
)]
pub async fn list_lots(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Path(id): Path<LibraryId>,
) -> Result<Json<Vec<CullingLotView>>, Problem> {
    let lots = CullingRepo::new(&state.db).list_lots(&ctx, id).await?;
    Ok(Json(lots.iter().map(CullingLotView::from_lot).collect()))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct PickRequest {
    #[schema(value_type = String, example = "pick")]
    pub pick: Pick,
}

/// Pick/reject/clear an asset's culling vote. Outside a culling lot this
/// stays just a vote, like `PUT /assets/{id}/flags`; inside a lot, the
/// physical move into `_taken`/`_skipped` accompanies the vote in the same
/// call ([`CullingRepo::set_pick`]). A dedicated route instead of extending
/// `PUT /assets/{id}/flags`: that route is already the hot path for
/// ordinary voting (no physical move, no variable permission scope) —
/// mixing in a conditional move would have made it harder to reason about
/// for both cases.
///
/// # Errors
/// `401` if not authenticated; `403` if the asset is not visible, or if it
/// is inside a lot and the caller is not editor of both the source and
/// destination folders; `409` on a name collision in the destination
/// folder.
#[utoipa::path(
    post,
    path = "/api/v1/assets/{id}/pick",
    tag = "culling",
    operation_id = "culling_set_pick",
    summary = "Pick, reject, or clear an asset's culling vote",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Asset id")),
    request_body = PickRequest,
    responses(
        (status = 200, description = "Asset updated (new folder_id if it moved)", body = AssetView),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "Asset not visible, or insufficient permission for the move", body = Problem),
        (status = 409, description = "Name collision in the destination folder", body = Problem)
    )
)]
pub async fn pick(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Path(id): Path<AssetId>,
    Json(body): Json<PickRequest>,
) -> Result<Json<AssetView>, Problem> {
    let asset = CullingRepo::new(&state.db)
        .set_pick(&ctx, id, body.pick)
        .await?;
    Ok(Json(AssetView::from_asset(&asset)))
}

/// "Empty rejected": permanently deletes from disk every asset currently
/// in `_skipped` for this lot. **Partial** success: an asset that fails to
/// purge does not block the others — see [`CullingRepo::empty_skipped`].
/// Authorization, however, stays all-or-nothing, checked before touching
/// any file.
///
/// # Errors
/// `401` if not authenticated; `403` if the caller cannot destroy even one
/// asset in the lot (owner/admin required).
#[utoipa::path(
    post,
    path = "/api/v1/culling/lots/{id}/empty-skipped",
    tag = "culling",
    operation_id = "culling_empty_skipped",
    summary = "Permanently delete every asset in a lot's _skipped folder",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Lot folder id")),
    responses(
        (status = 200, description = "Per-asset outcome (partial success allowed)", body = BulkOutcome),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "Not owner/admin of at least one asset in the lot", body = Problem)
    )
)]
pub async fn empty_skipped(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Path(id): Path<FolderId>,
) -> Result<Json<BulkOutcome>, Problem> {
    let results = CullingRepo::new(&state.db).empty_skipped(&ctx, id).await?;
    let mut succeeded = Vec::new();
    let mut failed = Vec::new();
    for (asset_id, outcome) in results {
        match outcome {
            Ok(()) => succeeded.push(asset_id),
            Err(e) => failed.push((asset_id, e)),
        }
    }
    Ok(Json(BulkOutcome::from_partition(succeeded, &failed, None)))
}
