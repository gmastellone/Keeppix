use axum::extract::{Path, State};
use keeppix_db::{AssetRepo, FolderRepo};
use keeppix_domain::{Asset, Folder, FolderId};
use serde::Serialize;

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
}

impl FolderView {
    fn from_folder(f: &Folder) -> Self {
        Self {
            id: f.id.to_string(),
            library_id: f.library_id.to_string(),
            parent_id: f.parent_id.map(|id| id.to_string()),
            name: f.name.clone(),
            depth: f.depth,
        }
    }
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct FolderChildren {
    pub folders: Vec<FolderView>,
    pub assets: Vec<AssetView>,
}

/// # Errors
/// `401` se non autenticato.
#[utoipa::path(
    get,
    path = "/api/v1/folders/tree",
    tag = "folders",
    operation_id = "folders_tree",
    security(("session_cookie" = [])),
    responses(
        (status = 200, description = "Albero delle cartelle visibili", body = [FolderView]),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 500, description = "Errore del database", body = Problem)
    )
)]
pub async fn tree(
    State(state): State<AppState>,
    Auth(ctx): Auth,
) -> Result<Json<Vec<FolderView>>, Problem> {
    let folders = FolderRepo::new(&state.db).tree(&ctx).await?;
    Ok(Json(folders.iter().map(FolderView::from_folder).collect()))
}

/// # Errors
/// `401` se non autenticato; `403` se la cartella non è visibile (anche
/// inesistente, per chi non è admin); `404` solo a un admin su id assente.
#[utoipa::path(
    get,
    path = "/api/v1/folders/{id}/children",
    tag = "folders",
    operation_id = "folders_children",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Id della cartella")),
    responses(
        (status = 200, description = "Figli diretti e asset", body = FolderChildren),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Cartella non visibile", body = Problem),
        (status = 404, description = "Cartella inesistente (solo admin)", body = Problem),
        (status = 500, description = "Errore del database", body = Problem)
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
