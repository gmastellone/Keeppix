//! Cold-start bundle: user, preferences, folder tree, disk space, badge counts.
//!
//! Additive — does not replace the individual endpoints. Composes the same
//! repositories they use; no SQL of its own.

use std::collections::BTreeMap;

use axum::extract::{Query, State};
use keeppix_db::{
    AssetTagRepo, CullingRepo, Db, FaceRepo, FolderRepo, LibraryRepo, PreferencesRepo, UserRepo,
};
use keeppix_domain::AuthContext;
use serde::{Deserialize, Serialize};

use crate::extract::Auth;
use crate::json::Json;
use crate::problem::Problem;
use crate::routes::auth::UserView;
use crate::routes::folders::FolderView;
use crate::routes::libraries::LibraryStorageView;
use crate::routes::preferences::UserPreferencesView;
use crate::state::AppState;

#[derive(Serialize, utoipa::ToSchema)]
pub struct BadgeCountsView {
    /// Photos still to be culled, summed across all lots of all libraries
    /// with a designated root. `list_lots` is already owner/admin-scoped by
    /// construction — a library without a designated root contributes zero
    /// without an extra query (`list_lots` skips it immediately).
    pub culling: i64,
    /// Pending tag/face proposals. Zero until there are queues.
    pub revision: i64,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct BootstrapResponse {
    pub user: UserView,
    pub preferences: UserPreferencesView,
    pub folders: Vec<FolderView>,
    /// Free/total space for each visible library — same payload as
    /// `GET /libraries/{id}/storage`, indexed by library id.
    pub storage: BTreeMap<String, LibraryStorageView>,
    pub badges: BadgeCountsView,
}

#[derive(Deserialize)]
pub struct BootstrapQuery {
    #[serde(default)]
    roots: bool,
}

/// Same composition as the HTTP handler, exposed for tests that count the
/// emitted queries.
///
/// # Errors
/// Same as the individual endpoints it composes.
pub async fn compose(
    db: &Db,
    ctx: &AuthContext,
    roots: bool,
    server_name: &str,
) -> Result<BootstrapResponse, Problem> {
    let user_id = ctx.user_id().ok_or_else(Problem::unauthenticated)?;
    let user = UserRepo::new(db).find_by_id(ctx, user_id).await?;
    let prefs = PreferencesRepo::new(db).get(ctx).await?;

    let folder_repo = FolderRepo::new(db);
    let folders = if roots {
        folder_repo.roots(ctx).await?
    } else {
        folder_repo.tree(ctx).await?
    };

    let libraries = LibraryRepo::new(db).list(ctx).await?;
    let library_repo = LibraryRepo::new(db);
    let mut storage = BTreeMap::new();
    for library in &libraries {
        let usage = library_repo.storage(ctx, library.id).await?;
        storage.insert(library.id.to_string(), LibraryStorageView::from(usage));
    }

    // "Culling" half of the badge: sum of `pending` across the lots of
    // every library that has a designated root. `list_lots` is safe to call
    // for each of `libraries` without risking `Forbidden`: `LibraryRepo::list`
    // has already filtered them to owner-or-admin, the same scope
    // `list_lots` expects internally.
    let culling_repo = CullingRepo::new(db);
    let mut culling = 0_i64;
    for library in &libraries {
        if library.culling_root_folder_id.is_none() {
            continue;
        }
        let lots = culling_repo.list_lots(ctx, library.id).await?;
        culling = culling.saturating_add(lots.iter().map(|lot| lot.pending).sum());
    }

    // "Tag" half of the badge. `count_proposed_visible` does not propagate
    // the absence of pgvector (returns 0) — bootstrap must never fail
    // because of an optional AI feature. The "faces" half is added on the
    // same field with the same guarantee (does not propagate the absence
    // of pgvector — see `FaceRepo::count_proposed_visible`).
    let tag_revision = AssetTagRepo::new(db).count_proposed_visible(ctx).await?;
    let face_revision = FaceRepo::new(db).count_proposed_visible(ctx).await?;
    let revision = tag_revision.saturating_add(face_revision);

    Ok(BootstrapResponse {
        user: UserView::new(&user, server_name),
        preferences: UserPreferencesView::from(prefs),
        folders: folders.iter().map(FolderView::from_folder).collect(),
        storage,
        badges: BadgeCountsView { culling, revision },
    })
}

/// # Errors
/// `401` if not authenticated; other codes as the individual composed
/// endpoints.
#[utoipa::path(
    get,
    path = "/api/v1/bootstrap",
    tag = "auth",
    operation_id = "bootstrap_get",
    summary = "Return user, preferences, folders, storage and badge counts in one response",
    security(("session_cookie" = [])),
    params(
        ("roots" = Option<bool>, Query, description = "Same as GET /folders/tree: roots only if true")
    ),
    responses(
        (status = 200, description = "Startup bundle", body = BootstrapResponse),
        (status = 401, description = "Not authenticated", body = Problem)
    )
)]
pub async fn get(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Query(query): Query<BootstrapQuery>,
) -> Result<Json<BootstrapResponse>, Problem> {
    Ok(Json(
        compose(&state.db, &ctx, query.roots, &state.server_name).await?,
    ))
}
