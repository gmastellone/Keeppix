//! `DELETE` — always goes through `TrashRepo::choose` with
//! `DiskAction::MovedToTrash`, **never** a direct deletion on the
//! filesystem: the protocol can't ask questions, so an accidental drag into
//! the trash in Finder must not be irreversible.
//!
//! **Note on permissions**: `TrashRepo::choose` on its own is *not* enough
//! to make an editor get `403` — for `DiskAction::MovedToTrash`,
//! `TrashRepo::choose` **deliberately** accepts an editor
//! (`assert_can_edit_assets` via `PermissionRepo`), not just owner/admin;
//! `may_purge` (owner/admin) only applies to `DiskAction::Purged`. This
//! module explicitly requires that an editor gets `403` on a `WebDAV`
//! `DELETE` — more restrictive than the REST API, because there's no
//! confirmation dialog here: an accidental drag into the trash in Finder
//! must not be silently accepted by anyone who has editor access on a
//! shared folder. The gate added here (owner/admin, the same predicate as
//! `may_purge`) runs before `TrashRepo::choose`, which remains the
//! **only** path by which a file physically ends up on disk — never a
//! direct `rm`.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use keeppix_db::{AssetRepo, FolderRepo, TrashRepo};
use keeppix_domain::{AssetId, AuthContext, DiskAction, FolderId};

use crate::problem::Problem;
use crate::state::AppState;

/// Library owner or admin — same predicate as `may_purge` in `trash.rs`
/// (private there, not re-exportable without modifying an already-stable
/// file: redefined here in one line, not duplicated in substance since it
/// takes an already-resolved `Library` as input from the caller and doesn't
/// run a query).
fn only_owner_or_admin(ctx: &AuthContext, library: &keeppix_domain::Library) -> bool {
    ctx.is_admin() || ctx.user_id() == Some(library.owner_id)
}

/// `DELETE /dav/asset/{id}` — trashes the asset via `TrashRepo::choose`,
/// but only for owner/admin (see the module note on why this is more
/// restrictive than the REST API): an editor gets `403` before reaching
/// `choose`.
///
/// # Errors
/// `403` if the caller can't see the asset, or can see it but isn't
/// owner/admin of the library. `500` for an I/O error on the `rename()`
/// into `.keeppix-trash/`.
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

/// `DELETE /dav/folder/{id}` — trashes every asset in the folder and its
/// descendants (`TrashRepo::choose` once per asset, never a direct
/// `rm -rf`), then removes the subtree's `folders` rows and the directory
/// from disk — only **after** every asset is already safe in the trash.
///
/// # Errors
/// `403` if the caller can't see the folder, or can see it but isn't
/// owner/admin (same more-restrictive gate as [`asset`] — see the module
/// note). `500` for an I/O error removing the final directory.
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

    // The path must be read while the folder row still exists: after
    // `delete_subtree` there would be no row left to reconstruct it from.
    let target_dir = folder_repo.absolute_path(ctx, id).await?;

    folder_repo.delete_subtree(ctx, id).await?;

    // DB before the filesystem, like `TrashRepo::choose(Purged)`: if the
    // commit above succeeded and this `remove_dir_all` fails, a directory
    // is left (now empty of real assets, only leftover dotfiles) with no
    // corresponding row — an orphan on disk, not a ghost row in the
    // database. The reverse — rows deleted but the directory intact with
    // real files still inside — can't happen: every asset has already been
    // moved out by `TrashRepo::choose` before we get here.
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
