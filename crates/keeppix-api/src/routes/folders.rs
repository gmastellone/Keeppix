use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use keeppix_db::{AssetRepo, FolderRepo, StackRepo};
use keeppix_domain::{Asset, Folder, FolderId};
use serde::{Deserialize, Serialize};

use crate::extract::Auth;
use crate::json::Json;
use crate::problem::Problem;
use crate::routes::timeline::AssetView;
use crate::state::AppState;

#[derive(Serialize, utoipa::ToSchema)]
pub struct FolderView {
    pub id: String,
    pub library_id: String,
    pub parent_id: Option<String>,
    pub name: String,
    pub depth: i32,
    // No `asset_count` per row — the prototype showed a count in the
    // sidebar, but that aggregate does not enter `/api/v1`.
}

impl FolderView {
    pub(crate) fn from_folder(f: &Folder) -> Self {
        Self {
            id: f.id.to_string(),
            library_id: f.library_id.to_string(),
            parent_id: f.parent_id.map(|id| id.to_string()),
            name: f.name.clone(),
            depth: f.depth,
        }
    }
}

#[derive(Deserialize)]
pub struct TreeQuery {
    #[serde(default)]
    roots: bool,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct MoveFolderRequest {
    parent_id: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct FolderChildren {
    pub folders: Vec<FolderView>,
    pub assets: Vec<AssetView>,
}

/// # Errors
/// `401` if not authenticated.
#[utoipa::path(
    get,
    path = "/api/v1/folders/tree",
    tag = "folders",
    operation_id = "folders_tree",
    summary = "Get the folder tree",
    security(("session_cookie" = [])),
    responses(
        (status = 200, description = "Tree of visible folders", body = [FolderView]),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 500, description = "Database error", body = Problem)
    )
)]
pub async fn tree(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Query(query): Query<TreeQuery>,
) -> Result<Json<Vec<FolderView>>, Problem> {
    let repo = FolderRepo::new(&state.db);
    let folders = if query.roots {
        repo.roots(&ctx).await?
    } else {
        repo.tree(&ctx).await?
    };
    Ok(Json(folders.iter().map(FolderView::from_folder).collect()))
}

/// # Errors
/// `401` if not authenticated; `403` if the folder is not visible (even
/// nonexistent, for a non-admin); `404` only for an admin on a missing id.
#[utoipa::path(
    get,
    path = "/api/v1/folders/{id}/children",
    tag = "folders",
    operation_id = "folders_children",
    summary = "List child folders",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Folder id")),
    responses(
        (status = 200, description = "Direct children and assets", body = FolderChildren),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "Folder not visible", body = Problem),
        (status = 404, description = "Folder does not exist (admin only)", body = Problem),
        (status = 500, description = "Database error", body = Problem)
    )
)]
pub async fn children(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Path(id): Path<FolderId>,
) -> Result<Json<FolderChildren>, Problem> {
    let folders = FolderRepo::new(&state.db).children(&ctx, id).await?;
    let assets: Vec<Asset> = AssetRepo::new(&state.db).find_by_folder(&ctx, id).await?;
    Ok(Json(FolderChildren {
        folders: folders.iter().map(FolderView::from_folder).collect(),
        assets: assets.iter().map(AssetView::from_asset).collect(),
    }))
}

/// Moves the subtree. Assets stay on the same row: only `folders.path` is
/// rewritten.
///
/// # Errors
/// `401`; `403` if not visible or if the caller is only a viewer; `409` on
/// a cycle or name collision.
#[utoipa::path(
    patch,
    path = "/api/v1/folders/{id}",
    tag = "folders",
    operation_id = "folders_move",
    summary = "Move a folder",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Id of the folder to move")),
    request_body = MoveFolderRequest,
    responses(
        (status = 204, description = "Subtree moved"),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "Folder not visible or insufficient role", body = Problem),
        (status = 409, description = "Cycle or name already present", body = Problem),
        (status = 500, description = "Database error", body = Problem)
    )
)]
pub async fn relocate(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Path(id): Path<FolderId>,
    Json(body): Json<MoveFolderRequest>,
) -> Result<StatusCode, Problem> {
    let parent_id: FolderId = body.parent_id.parse().map_err(|_| {
        Problem::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid-id",
            "Invalid folder id",
        )
    })?;
    let repo = FolderRepo::new(&state.db);
    let folder = repo.find_by_id(&ctx, id).await?;
    repo.move_subtree(&ctx, id, parent_id).await?;
    let stacks = StackRepo::new(&state.db);
    stacks.regroup_folder(id).await?;
    if let Some(old_parent) = folder.parent_id {
        stacks.regroup_folder(old_parent).await?;
    }
    stacks.regroup_folder(parent_id).await?;
    Ok(StatusCode::NO_CONTENT)
}
