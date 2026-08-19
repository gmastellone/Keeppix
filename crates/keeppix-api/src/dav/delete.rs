//! `DELETE` (Task 8, Fase 5) — passa sempre da `TrashRepo::choose` con
//! `DiskAction::MovedToTrash`, **mai** una cancellazione diretta sul
//! filesystem (invariante di `AGENTS.md`): il protocollo non può fare
//! domande, quindi un trascinamento per sbaglio nel Finder non deve essere
//! irreversibile.
//!
//! **Scoperta rispetto al brief** (da riportare nel ledger): il brief
//! affermava che `TrashRepo::choose` da sola basta a far ricevere `403` a
//! un editor, "stessa regola di `may_purge`". Falso — verificato leggendo
//! `trash.rs` e il test già esistente
//! `permissions_roles.rs::a_folder_editor_can_edit_metadata_and_trash`
//! (Task 14b): per `DiskAction::MovedToTrash`, `TrashRepo::choose` accetta
//! **volutamente** un editor (`assert_can_edit_assets` via
//! `PermissionRepo`), non solo owner/admin — `may_purge` (owner/admin) si
//! applica solo a `DiskAction::Purged`. Il compito richiede esplicitamente
//! e senza margine di interpretazione che un editor riceva `403` su
//! `DELETE` `WebDAV` — più restrittivo della REST API, perché qui non c'è
//! un dialogo di conferma: un trascinamento per sbaglio nel Finder non deve
//! poter essere silenziosamente accettato da chiunque abbia editor su una
//! cartella condivisa. Il gate aggiunto qui (owner/admin, stesso predicato
//! di `may_purge`) precede `TrashRepo::choose`, che resta comunque
//! l'**unica** via con cui il file finisce fisicamente sul disco — mai un
//! `rm` diretto.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use keeppix_db::{AssetRepo, FolderRepo, TrashRepo};
use keeppix_domain::{AssetId, AuthContext, DiskAction, FolderId};

use crate::problem::Problem;
use crate::state::AppState;

/// Owner della libreria o admin — stesso predicato di `may_purge` in
/// `trash.rs` (privato lì, non riesportabile senza modificare un file già
/// stabile: ridefinito qui una riga, non duplicato in sostanza perché
/// prende in input un `Library` già risolto dal chiamante, non fa query).
fn only_owner_or_admin(ctx: &AuthContext, library: &keeppix_domain::Library) -> bool {
    ctx.is_admin() || ctx.user_id() == Some(library.owner_id)
}

/// `DELETE /dav/asset/{id}` — cestina l'asset via `TrashRepo::choose`, ma
/// solo per owner/admin (vedi la nota di modulo sul perché più restrittivo
/// della REST API): un editor riceve `403` prima di arrivare a `choose`.
///
/// # Errors
/// `403` se il chiamante non vede l'asset, o lo vede ma non è owner/admin
/// della libreria. `500` per un errore di I/O sul `rename()` verso
/// `.keeppix-trash/`.
pub async fn asset(state: &AppState, ctx: &AuthContext, id: AssetId) -> Result<Response, Problem> {
    let asset = AssetRepo::new(&state.db).find_by_id(ctx, id).await?;
    let (_, library) = FolderRepo::new(&state.db)
        .assert_editor(ctx, asset.folder_id)
        .await?;
    if !only_owner_or_admin(ctx, &library) {
        return Err(Problem::forbidden());
    }

    TrashRepo::new(&state.db)
        .choose(ctx, id, DiskAction::MovedToTrash)
        .await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

/// `DELETE /dav/folder/{id}` — cestina ogni asset della cartella e dei suoi
/// discendenti (`TrashRepo::choose` una volta per asset, mai un `rm -rf`
/// diretto), poi rimuove le righe `folders` del sottoalbero e la directory
/// dal disco — solo **dopo** che ogni asset è già al sicuro nel cestino.
///
/// # Errors
/// `403` se il chiamante non vede la cartella, o la vede ma non è
/// owner/admin (stesso gate più restrittivo di [`asset`] — vedi la nota di
/// modulo). `500` per un errore di I/O sulla rimozione finale della
/// directory.
pub async fn folder(
    state: &AppState,
    ctx: &AuthContext,
    id: FolderId,
) -> Result<Response, Problem> {
    let folder_repo = FolderRepo::new(&state.db);
    let (_, library) = folder_repo.assert_editor(ctx, id).await?;
    if !only_owner_or_admin(ctx, &library) {
        return Err(Problem::forbidden());
    }

    let folders = folder_repo.subtree(ctx, id).await?;
    let asset_repo = AssetRepo::new(&state.db);
    let trash_repo = TrashRepo::new(&state.db);
    for descendant in &folders {
        let assets = asset_repo.find_by_folder(ctx, descendant.id).await?;
        for asset in assets {
            trash_repo
                .choose(ctx, asset.id, DiskAction::MovedToTrash)
                .await?;
        }
    }

    // Il percorso va letto mentre la cartella esiste ancora: dopo
    // `delete_subtree` non ci sarebbe più una riga da cui ricostruirlo.
    let target_dir = folder_repo.absolute_path(ctx, id).await?;

    folder_repo.delete_subtree(ctx, id).await?;

    // DB prima del filesystem, come `TrashRepo::choose(Purged)`: se il
    // commit sopra è riuscito e questa `remove_dir_all` fallisce, resta una
    // directory (ormai vuota di asset reali, solo dotfile residui) senza
    // riga corrispondente — un orfano su disco, non una riga fantasma nel
    // database. L'inverso — righe cancellate ma la directory intatta con
    // ancora dentro file veri — non può capitare: ogni asset è già stato
    // spostato via da `TrashRepo::choose` prima di arrivare qui.
    if let Err(err) = tokio::fs::remove_dir_all(&target_dir).await
        && err.kind() != std::io::ErrorKind::NotFound
    {
        tracing::error!(
            error = %err,
            path = %target_dir.display(),
            "webdav DELETE: folder removed from the database but its directory could not be removed from disk"
        );
        return Err(Problem::internal());
    }

    Ok(StatusCode::NO_CONTENT.into_response())
}
