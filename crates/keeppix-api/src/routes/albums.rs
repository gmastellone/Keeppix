//! Album virtuali (spec §5). CRUD + gestione asset nell'album.
//! Autorizzazione: owner o admin per le mutazioni; utenti condivisi per la
//! lettura tramite permesso diretto sull'album.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use keeppix_db::{AlbumPatch, AlbumRepo, NewAlbum};
use keeppix_domain::{AlbumId, AssetId};
use serde::{Deserialize, Serialize};

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
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct PatchAlbumBody {
    pub name: Option<String>,
    pub description: Option<String>,
    /// `null` esplicito rimuove la copertina; assente = invariato.
    #[allow(clippy::option_option)]
    pub cover_asset_id: Option<Option<String>>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct ReorderBody {
    pub position: i64,
}

/// # Errors
/// `401` se non autenticato; `400` se il nome è assente.
#[utoipa::path(
    post,
    path = "/api/v1/albums",
    tag = "albums",
    operation_id = "albums_create",
    summary = "Create an album",
    security(("session_cookie" = [])),
    request_body = CreateAlbumBody,
    responses(
        (status = 201, description = "Album creato", body = AlbumView),
        (status = 401, description = "Non autenticato", body = Problem),
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
            },
        )
        .await?;
    Ok((StatusCode::CREATED, Json(AlbumView::from_album(album))))
}

/// # Errors
/// `401` se non autenticato.
#[utoipa::path(
    get,
    path = "/api/v1/albums",
    tag = "albums",
    operation_id = "albums_list",
    summary = "List albums",
    security(("session_cookie" = [])),
    responses(
        (status = 200, description = "Lista album visibili", body = Vec<AlbumView>),
        (status = 401, description = "Non autenticato", body = Problem),
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
/// `401` se non autenticato; `403` se non visibile.
#[utoipa::path(
    get,
    path = "/api/v1/albums/{id}",
    tag = "albums",
    operation_id = "albums_get",
    summary = "Get an album",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Id dell'album")),
    responses(
        (status = 200, description = "Album", body = AlbumView),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Album non visibile", body = Problem),
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
/// `401` se non autenticato; `403` se non owner/admin.
#[utoipa::path(
    patch,
    path = "/api/v1/albums/{id}",
    tag = "albums",
    operation_id = "albums_patch",
    summary = "Update an album",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Id dell'album")),
    request_body = PatchAlbumBody,
    responses(
        (status = 200, description = "Album aggiornato", body = AlbumView),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Non owner/admin", body = Problem),
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
/// `401` se non autenticato; `403` se non owner/admin.
#[utoipa::path(
    delete,
    path = "/api/v1/albums/{id}",
    tag = "albums",
    operation_id = "albums_delete",
    summary = "Delete an album",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Id dell'album")),
    responses(
        (status = 204, description = "Album eliminato, asset intatti"),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Non owner/admin", body = Problem),
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
/// `401` se non autenticato; `403` se non owner/admin o asset non visibile.
#[utoipa::path(
    post,
    path = "/api/v1/albums/{id}/assets/{asset_id}",
    tag = "albums",
    operation_id = "albums_add_asset",
    summary = "Add an asset to an album",
    security(("session_cookie" = [])),
    params(
        ("id" = String, Path, description = "Id dell'album"),
        ("asset_id" = String, Path, description = "Id dell'asset"),
    ),
    responses(
        (status = 204, description = "Asset aggiunto all'album"),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Non owner/admin o asset non visibile", body = Problem),
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
/// `401` se non autenticato; `403` se non owner/admin.
#[utoipa::path(
    delete,
    path = "/api/v1/albums/{id}/assets/{asset_id}",
    tag = "albums",
    operation_id = "albums_remove_asset",
    summary = "Remove an asset from an album",
    security(("session_cookie" = [])),
    params(
        ("id" = String, Path, description = "Id dell'album"),
        ("asset_id" = String, Path, description = "Id dell'asset"),
    ),
    responses(
        (status = 204, description = "Asset rimosso dall'album, asset intatto"),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Non owner/admin", body = Problem),
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
/// `401` se non autenticato; `403` se non owner/admin.
#[utoipa::path(
    patch,
    path = "/api/v1/albums/{id}/assets/{asset_id}/position",
    tag = "albums",
    operation_id = "albums_reorder_asset",
    summary = "Reorder an album asset",
    security(("session_cookie" = [])),
    params(
        ("id" = String, Path, description = "Id dell'album"),
        ("asset_id" = String, Path, description = "Id dell'asset"),
    ),
    request_body = ReorderBody,
    responses(
        (status = 204, description = "Posizione aggiornata"),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Non owner/admin", body = Problem),
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
/// `401` se non autenticato; `403` se album non visibile.
#[utoipa::path(
    get,
    path = "/api/v1/albums/{id}/assets",
    tag = "albums",
    operation_id = "albums_list_assets",
    summary = "List album assets",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Id dell'album")),
    responses(
        (status = 200, description = "Asset dell'album nell'ordine manuale", body = Vec<AlbumAssetView>),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Album non visibile", body = Problem),
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
