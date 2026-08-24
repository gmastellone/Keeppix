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
    /// Foto ancora da valutare nel culling, sommate su tutti i lotti di
    /// tutte le librerie con una radice designata (Fase 9, esposto in Fase
    /// 11 Task 17). `list_lots` è già owner/admin-scoped per costruzione —
    /// una libreria senza radice designata contribuisce zero senza una
    /// query in più (`list_lots` la salta subito).
    pub culling: i64,
    /// Proposte tag/volti in attesa (Fasi 7/8). Zero finché non ci sono code.
    pub revision: i64,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct BootstrapResponse {
    pub user: UserView,
    pub preferences: UserPreferencesView,
    pub folders: Vec<FolderView>,
    /// Spazio libero/totale per ogni libreria visibile — stesso payload di
    /// `GET /libraries/{id}/storage`, indicizzato per id libreria.
    pub storage: BTreeMap<String, LibraryStorageView>,
    pub badges: BadgeCountsView,
}

#[derive(Deserialize)]
pub struct BootstrapQuery {
    #[serde(default)]
    roots: bool,
}

/// Stessa composizione del handler HTTP, esposta per i test che contano le
/// query emesse (Task 17).
///
/// # Errors
/// Come i singoli endpoint che compone.
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

    // Metà "culling" del badge: somma di `pending` sui lotti di ogni
    // libreria che ha una radice designata. `list_lots` è sicuro da
    // chiamare per ognuna di `libraries` senza rischiare `Forbidden`:
    // `LibraryRepo::list` le ha già filtrate a owner-o-admin, lo stesso
    // ambito che `list_lots` pretende internamente.
    let culling_repo = CullingRepo::new(db);
    let mut culling = 0_i64;
    for library in &libraries {
        if library.culling_root_folder_id.is_none() {
            continue;
        }
        let lots = culling_repo.list_lots(ctx, library.id).await?;
        culling = culling.saturating_add(lots.iter().map(|lot| lot.pending).sum());
    }

    // Fase 7 Task 9: metà "tag" del badge. `count_proposed_visible` non
    // propaga l'assenza di pgvector (torna 0) — il bootstrap non deve mai
    // fallire per una feature IA opzionale (Task 3 ruling). Fase 8 Task 8
    // aggiunge la metà "volti" sullo stesso campo, stessa garanzia (non
    // propaga l'assenza di pgvector — vedi `FaceRepo::count_proposed_visible`).
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
/// `401` se non autenticato; gli altri codici come i singoli endpoint composti.
#[utoipa::path(
    get,
    path = "/api/v1/bootstrap",
    tag = "auth",
    operation_id = "bootstrap_get",
    summary = "Return user, preferences, folders, storage and badge counts in one response",
    security(("session_cookie" = [])),
    params(
        ("roots" = Option<bool>, Query, description = "Come GET /folders/tree: solo radici se true")
    ),
    responses(
        (status = 200, description = "Bundle di avvio", body = BootstrapResponse),
        (status = 401, description = "Non autenticato", body = Problem)
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
