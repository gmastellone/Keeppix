//! Virtual albums. CRUD + asset management within an album.
//! Authorization: owner or admin for mutations; shared users for reading
//! via a direct permission on the album.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use keeppix_db::{AlbumPatch, AlbumRepo, NewAlbum, SearchNode};
use keeppix_domain::{AlbumId, AssetId};
use serde::{Deserialize, Serialize};

use crate::bulk::BulkOutcome;
use crate::extract::Auth;
use crate::json::Json;
use crate::problem::Problem;
use crate::routes::timeline::AssetView;
use crate::state::AppState;

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct AlbumView {
    pub id: String,
    pub name: String,
    pub description: String,
    pub owner_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cover_asset_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Present if the album can be refreshed with `POST .../refresh`
    /// (there is no dynamic album, only a filter that re-runs when the
    /// user asks for it).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<Object>)]
    pub rule: Option<SearchNode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule_run_at: Option<DateTime<Utc>>,
    pub is_shared: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cover_tint: Option<String>,
    pub monochrome: bool,
    // No member count per row in `GET /albums` — that stays a trivial
    // read on `album_assets` only when needed (e.g. the album page),
    // not an aggregate tacked onto every list entry.
}

impl AlbumView {
    fn from_album(album: keeppix_db::Album) -> Self {
        Self {
            id: album.id.to_string(),
            name: album.name,
            description: album.description,
            owner_id: album.owner_id.to_string(),
            cover_asset_id: album.cover_asset_id.map(|id| id.to_string()),
            created_at: album.created_at,
            updated_at: album.updated_at,
            rule: album.rule,
            rule_run_at: album.rule_run_at,
            is_shared: album.is_shared,
            cover_tint: album.cover_tint,
            monochrome: album.monochrome,
        }
    }
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct AlbumAssetView {
    #[serde(flatten)]
    pub asset: AssetView,
    pub position: i64,
    pub added_by: String,
    pub added_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateAlbumBody {
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// The filter the album is created with. `None` for a purely manual
    /// album, which can then never be refreshed via `refresh`.
    #[serde(default)]
    #[schema(value_type = Option<Object>)]
    pub rule: Option<SearchNode>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct PatchAlbumBody {
    pub name: Option<String>,
    pub description: Option<String>,
    /// Explicit `null` removes the cover; absent = unchanged.
    #[allow(clippy::option_option)]
    pub cover_asset_id: Option<Option<String>>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct ReorderBody {
    pub position: i64,
}

/// # Errors
/// `401` if not authenticated; `400` if the name is missing.
#[utoipa::path(
    post,
    path = "/api/v1/albums",
    tag = "albums",
    operation_id = "albums_create",
    summary = "Create an album",
    security(("session_cookie" = [])),
    request_body = CreateAlbumBody,
    responses(
        (status = 201, description = "Album created", body = AlbumView),
        (status = 401, description = "Not authenticated", body = Problem),
    )
)]
pub async fn create(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Json(body): Json<CreateAlbumBody>,
) -> Result<(StatusCode, Json<AlbumView>), Problem> {
    let album = AlbumRepo::new(&state.db)
        .create(
            &ctx,
            NewAlbum {
                name: body.name,
                description: body.description,
                rule: body.rule,
            },
        )
        .await?;
    Ok((StatusCode::CREATED, Json(AlbumView::from_album(album))))
}

/// # Errors
/// `401` if not authenticated.
#[utoipa::path(
    get,
    path = "/api/v1/albums",
    tag = "albums",
    operation_id = "albums_list",
    summary = "List albums",
    security(("session_cookie" = [])),
    responses(
        (status = 200, description = "List of visible albums", body = Vec<AlbumView>),
        (status = 401, description = "Not authenticated", body = Problem),
    )
)]
pub async fn list(
    State(state): State<AppState>,
    Auth(ctx): Auth,
) -> Result<Json<Vec<AlbumView>>, Problem> {
    let albums = AlbumRepo::new(&state.db).list(&ctx).await?;
    Ok(Json(
        albums.into_iter().map(AlbumView::from_album).collect(),
    ))
}

/// # Errors
/// `401` if not authenticated; `403` if not visible.
#[utoipa::path(
    get,
    path = "/api/v1/albums/{id}",
    tag = "albums",
    operation_id = "albums_get",
    summary = "Get an album",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Album id")),
    responses(
        (status = 200, description = "Album", body = AlbumView),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "Album not visible", body = Problem),
    )
)]
pub async fn get(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Path(id): Path<AlbumId>,
) -> Result<Json<AlbumView>, Problem> {
    let album = AlbumRepo::new(&state.db).get(&ctx, id).await?;
    Ok(Json(AlbumView::from_album(album)))
}

/// # Errors
/// `401` if not authenticated; `403` if not owner/admin.
#[utoipa::path(
    patch,
    path = "/api/v1/albums/{id}",
    tag = "albums",
    operation_id = "albums_patch",
    summary = "Update an album",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Album id")),
    request_body = PatchAlbumBody,
    responses(
        (status = 200, description = "Album updated", body = AlbumView),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "Not owner/admin", body = Problem),
    )
)]
pub async fn patch(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Path(id): Path<AlbumId>,
    Json(body): Json<PatchAlbumBody>,
) -> Result<Json<AlbumView>, Problem> {
    let cover_asset_id = match body.cover_asset_id {
        None => None,
        Some(None) => Some(None),
        Some(Some(raw)) => {
            Some(Some(raw.parse::<AssetId>().map_err(|_| {
                Problem::bad_request("invalid-asset-id", "Invalid asset id")
            })?))
        }
    };
    let album = AlbumRepo::new(&state.db)
        .update(
            &ctx,
            id,
            AlbumPatch {
                name: body.name,
                description: body.description,
                cover_asset_id,
            },
        )
        .await?;
    Ok(Json(AlbumView::from_album(album)))
}

/// # Errors
/// `401` if not authenticated; `403` if not owner/admin.
#[utoipa::path(
    delete,
    path = "/api/v1/albums/{id}",
    tag = "albums",
    operation_id = "albums_delete",
    summary = "Delete an album",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Album id")),
    responses(
        (status = 204, description = "Album deleted, assets untouched"),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "Not owner/admin", body = Problem),
    )
)]
pub async fn delete(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Path(id): Path<AlbumId>,
) -> Result<StatusCode, Problem> {
    AlbumRepo::new(&state.db).delete(&ctx, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// # Errors
/// `401` if not authenticated; `403` if not owner/admin or the asset is not visible.
#[utoipa::path(
    post,
    path = "/api/v1/albums/{id}/assets/{asset_id}",
    tag = "albums",
    operation_id = "albums_add_asset",
    summary = "Add an asset to an album",
    security(("session_cookie" = [])),
    params(
        ("id" = String, Path, description = "Album id"),
        ("asset_id" = String, Path, description = "Asset id"),
    ),
    responses(
        (status = 204, description = "Asset added to the album"),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "Not owner/admin or asset not visible", body = Problem),
    )
)]
pub async fn add_asset(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Path((id, asset_id)): Path<(AlbumId, AssetId)>,
) -> Result<StatusCode, Problem> {
    AlbumRepo::new(&state.db)
        .add_asset(&ctx, id, asset_id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// # Errors
/// `401` if not authenticated; `403` if not owner/admin.
#[utoipa::path(
    delete,
    path = "/api/v1/albums/{id}/assets/{asset_id}",
    tag = "albums",
    operation_id = "albums_remove_asset",
    summary = "Remove an asset from an album",
    security(("session_cookie" = [])),
    params(
        ("id" = String, Path, description = "Album id"),
        ("asset_id" = String, Path, description = "Asset id"),
    ),
    responses(
        (status = 204, description = "Asset removed from the album, asset untouched"),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "Not owner/admin", body = Problem),
    )
)]
pub async fn remove_asset(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Path((id, asset_id)): Path<(AlbumId, AssetId)>,
) -> Result<StatusCode, Problem> {
    AlbumRepo::new(&state.db)
        .remove_asset(&ctx, id, asset_id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// # Errors
/// `401` if not authenticated; `403` if not owner/admin.
#[utoipa::path(
    patch,
    path = "/api/v1/albums/{id}/assets/{asset_id}/position",
    tag = "albums",
    operation_id = "albums_reorder_asset",
    summary = "Reorder an album asset",
    security(("session_cookie" = [])),
    params(
        ("id" = String, Path, description = "Album id"),
        ("asset_id" = String, Path, description = "Asset id"),
    ),
    request_body = ReorderBody,
    responses(
        (status = 204, description = "Position updated"),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "Not owner/admin", body = Problem),
    )
)]
pub async fn reorder_asset(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Path((id, asset_id)): Path<(AlbumId, AssetId)>,
    Json(body): Json<ReorderBody>,
) -> Result<StatusCode, Problem> {
    AlbumRepo::new(&state.db)
        .reorder(&ctx, id, asset_id, body.position)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// # Errors
/// `401` if not authenticated; `403` if the album is not visible.
#[utoipa::path(
    get,
    path = "/api/v1/albums/{id}/assets",
    tag = "albums",
    operation_id = "albums_list_assets",
    summary = "List album assets",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Album id")),
    responses(
        (status = 200, description = "Album assets in manual order", body = Vec<AlbumAssetView>),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "Album not visible", body = Problem),
    )
)]
pub async fn list_assets(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Path(id): Path<AlbumId>,
) -> Result<Json<Vec<AlbumAssetView>>, Problem> {
    let items = AlbumRepo::new(&state.db).list_assets(&ctx, id).await?;
    Ok(Json(
        items
            .into_iter()
            .map(|item| AlbumAssetView {
                asset: AssetView::from_asset(&item.asset),
                position: item.position,
                added_by: item.added_by.to_string(),
                added_at: item.added_at,
            })
            .collect(),
    ))
}

/// Re-runs the `rule` the album was created with ("Refresh album" instead
/// of a fully dynamic album): `succeeded` lists both the assets that
/// **entered** and those that **left** `album_assets` in this run — they
/// are the two faces of the same successful mutation, not two distinct
/// categories. `failed` stays empty today: the refresh is a server-side
/// diff over assets already visible to the caller, not a per-id operation
/// that could deny a single element.
///
/// # Errors
/// `401` if not authenticated; `403` if not owner/admin of the album; `400`
/// if the album has no `rule` to re-run.
#[utoipa::path(
    post,
    path = "/api/v1/albums/{id}/refresh",
    tag = "albums",
    operation_id = "albums_refresh",
    summary = "Refresh an album's rule-based membership",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Album id")),
    responses(
        (status = 200, description = "Photos added and removed (partial-success wrapper)", body = BulkOutcome),
        (status = 400, description = "The album has no rule to re-run", body = Problem),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "Not owner/admin", body = Problem),
    )
)]
pub async fn refresh(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Path(id): Path<AlbumId>,
) -> Result<Json<BulkOutcome>, Problem> {
    let refresh = AlbumRepo::new(&state.db)
        .refresh(&ctx, id)
        .await?
        .ok_or_else(|| Problem::bad_request("album-has-no-rule", "Album has no rule to refresh"))?;
    let mut succeeded = refresh.added;
    succeeded.extend(refresh.removed);
    Ok(Json(BulkOutcome::from_partition(succeeded, &[], None)))
}

/// An album an asset belongs to, as shown by the ALBUM section of the
/// lightbox info panel — id and name only: the chips are not clickable and
/// do not distinguish manual from dynamic.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct AlbumBadgeView {
    pub id: String,
    pub name: String,
}

impl From<keeppix_db::AlbumBadge> for AlbumBadgeView {
    fn from(a: keeppix_db::AlbumBadge) -> Self {
        Self {
            id: a.id.to_string(),
            name: a.name,
        }
    }
}

/// The opposite direction of [`list_assets`] — given an asset, which albums
/// it already belongs to. No such lookup existed before this
/// ([`AlbumRepo`] only went album→asset).
///
/// # Errors
/// `401` if not authenticated.
#[utoipa::path(
    get,
    path = "/api/v1/assets/{id}/albums",
    tag = "albums",
    operation_id = "assets_list_albums",
    summary = "List the albums (manual and dynamic) an asset already belongs to",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Asset id")),
    responses(
        (status = 200, description = "Albums the asset is a member of, by name", body = Vec<AlbumBadgeView>),
        (status = 401, description = "Not authenticated", body = Problem)
    )
)]
pub async fn list_for_asset(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Path(asset_id): Path<AssetId>,
) -> Result<Json<Vec<AlbumBadgeView>>, Problem> {
    let albums = AlbumRepo::new(&state.db).for_asset(&ctx, asset_id).await?;
    Ok(Json(albums.into_iter().map(AlbumBadgeView::from).collect()))
}
