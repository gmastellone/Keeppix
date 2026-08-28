use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use keeppix_domain::{Asset, AssetId, AuthContext, DiskAction, TrashEntry, TrashEntryId, UserId};
use sqlx::PgConnection;

use crate::visibility::VisibilityScope;
use crate::{AssetRepo, Db, DbError, FolderRepo};

/// Retention days for `moved_to_trash` before cleanup.
pub const TRASH_RETENTION_DAYS: i64 = 30;

/// Name of the trash folder, inside the library root. **Must** stay
/// identical to the one excluded by the walker
/// (`keeppix_media::walk::is_excluded_name`): a mismatch here would
/// produce an infinite reindexing loop on a large library.
pub const TRASH_DIR_NAME: &str = ".keeppix-trash";

const COLUMNS: &str = "id, asset_id, deleted_by, deleted_at, original_path, trash_path, \
                       disk_action, restored_at";
const TE_COLUMNS: &str = "te.id, te.asset_id, te.deleted_by, te.deleted_at, te.original_path, \
                          te.trash_path, te.disk_action, te.restored_at";

#[derive(sqlx::FromRow)]
struct EntryRow {
    id: uuid::Uuid,
    asset_id: uuid::Uuid,
    deleted_by: Option<uuid::Uuid>,
    deleted_at: DateTime<Utc>,
    original_path: String,
    trash_path: Option<String>,
    disk_action: String,
    restored_at: Option<DateTime<Utc>>,
}

impl EntryRow {
    fn into_domain(self) -> Result<TrashEntry, DbError> {
        Ok(TrashEntry {
            id: TrashEntryId::from_uuid(self.id),
            asset_id: AssetId::from_uuid(self.asset_id),
            deleted_by: self.deleted_by.map(UserId::from_uuid),
            deleted_at: self.deleted_at,
            original_path: self.original_path,
            trash_path: self.trash_path,
            disk_action: DiskAction::parse(&self.disk_action)
                .map_err(|e| crate::row::corrupted("trash_entries.disk_action", e))?,
            restored_at: self.restored_at,
        })
    }
}

#[derive(sqlx::FromRow)]
struct PendingRow {
    id: uuid::Uuid,
    asset_id: uuid::Uuid,
    original_path: String,
    trash_path: Option<String>,
}

#[derive(sqlx::FromRow)]
struct LibraryInfo {
    root_path: String,
    owner_id: uuid::Uuid,
}

pub struct TrashRepo<'a> {
    db: &'a Db,
}

impl<'a> TrashRepo<'a> {
    #[must_use]
    pub const fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// Applies one of the three deletion options to an asset, and always
    /// records an audit row in `trash_entries`.
    ///
    /// # Errors
    /// `Forbidden` if the caller cannot see the asset — even when the id
    /// does not exist — if requesting [`DiskAction::Purged`] without
    /// being the library owner or an admin, or if requesting
    /// [`DiskAction::MovedToTrash`] / [`DiskAction::Kept`] without editor
    /// role. `Io` if the filesystem operation fails.
    pub async fn choose(
        &self,
        ctx: &AuthContext,
        asset_id: AssetId,
        action: DiskAction,
    ) -> Result<TrashEntry, DbError> {
        let (asset, library, folder_abs) = authorize_choose(self.db, ctx, asset_id, action).await?;
        let original_path = folder_abs.join(asset.filename.as_str());

        let entry_id = TrashEntryId::new();
        let deleted_by = ctx.user_id().map(|id| id.as_uuid());

        let row = match action {
            DiskAction::Kept => {
                let mut tx = self.db.pool().begin().await?;
                let row = insert_entry(
                    &mut tx,
                    entry_id,
                    asset_id,
                    deleted_by,
                    &original_path,
                    None,
                    action,
                )
                .await?;
                sqlx::query("DELETE FROM assets WHERE id = $1")
                    .bind(asset_id.as_uuid())
                    .execute(&mut *tx)
                    .await?;
                tx.commit().await?;
                row
            }
            DiskAction::Purged => {
                // DB before the filesystem: if the commit succeeds and
                // `remove_file` fails, an orphan is left on disk (the next
                // scan reindexes it). The reverse — file already deleted
                // and the row still in `assets` — is data loss with no audit.
                let mut tx = self.db.pool().begin().await?;
                let row = insert_entry(
                    &mut tx,
                    entry_id,
                    asset_id,
                    deleted_by,
                    &original_path,
                    None,
                    action,
                )
                .await?;
                sqlx::query("DELETE FROM assets WHERE id = $1")
                    .bind(asset_id.as_uuid())
                    .execute(&mut *tx)
                    .await?;
                tx.commit().await?;
                remove_file_tolerant(&original_path)?;
                row
            }
            DiskAction::MovedToTrash => {
                let root = PathBuf::from(&library.root_path);
                let trash_path =
                    prepare_trash_path(&root, &folder_abs, entry_id, asset.filename.as_str())?;
                // Same ordering as `Purged`: commit the audit row + `trashed`
                // status, then `rename()`. If the rename fails the row
                // stays (file still in `original_path`) and a retry can
                // complete the move; the reverse would leave a file in the
                // trash with no audit row, invisible to the UI.
                let mut tx = self.db.pool().begin().await?;
                let row = insert_entry(
                    &mut tx,
                    entry_id,
                    asset_id,
                    deleted_by,
                    &original_path,
                    Some(&trash_path),
                    action,
                )
                .await?;
                sqlx::query(
                    "UPDATE assets SET status = 'trashed', updated_at = now() WHERE id = $1",
                )
                .bind(asset_id.as_uuid())
                .execute(&mut *tx)
                .await?;
                tx.commit().await?;
                std::fs::rename(&original_path, &trash_path).map_err(|e| {
                    DbError::Io(format!(
                        "moving {} to {}: {e}",
                        original_path.display(),
                        trash_path.display()
                    ))
                })?;
                row
            }
        };

        row.into_domain()
    }

    /// Checks that the caller can request [`DiskAction::Purged`] on
    /// **all** assets in the batch, without performing any write to
    /// either the database or the filesystem. Used by
    /// `POST /assets/batch/delete` to make `purged` all-or-nothing on
    /// authorization: unlike `kept`/`moved_to_trash`, where an
    /// unauthorized id ends up in `failed` without blocking the others,
    /// a single non-purgeable id must reject the entire batch **before**
    /// [`Self::choose`] touches the first file — not a half-completed
    /// deletion.
    ///
    /// Reuses the same gate as [`Self::choose`] ([`authorize_choose`]),
    /// not a copy: no new authorization rule is introduced here.
    ///
    /// # Errors
    /// `Forbidden` same as [`Self::choose`] with [`DiskAction::Purged`],
    /// at the first id in the batch that does not pass it.
    pub async fn assert_batch_purge_authorized(
        &self,
        ctx: &AuthContext,
        asset_ids: &[AssetId],
    ) -> Result<(), DbError> {
        for &asset_id in asset_ids {
            authorize_choose(self.db, ctx, asset_id, DiskAction::Purged).await?;
        }
        Ok(())
    }

    /// Restores to its original path the most recently trashed file for
    /// this asset, and the asset back to `indexed`.
    ///
    /// **Never overwrites**: if the original path is occupied again — by
    /// another file, or by a concurrent restore — the operation fails
    /// with `Conflict` without touching anything.
    ///
    /// # Errors
    /// `Forbidden` same as [`Self::choose`]. `Conflict` if the asset has
    /// no pending trash entry, or if the original path is occupied. `Io`
    /// if `rename()` fails.
    pub async fn restore(&self, ctx: &AuthContext, asset_id: AssetId) -> Result<(), DbError> {
        AssetRepo::new(self.db)
            .assert_visible(ctx, std::slice::from_ref(&asset_id))
            .await?;

        let pending: Option<PendingRow> = sqlx::query_as(
            "SELECT id, asset_id, original_path, trash_path FROM trash_entries \
              WHERE asset_id = $1 AND disk_action = 'moved_to_trash' AND restored_at IS NULL \
              ORDER BY deleted_at DESC LIMIT 1",
        )
        .bind(asset_id.as_uuid())
        .fetch_optional(self.db.pool())
        .await?;

        let Some(pending) = pending else {
            return Err(DbError::Conflict("asset is not in the trash".to_owned()));
        };
        let Some(trash_path) = pending.trash_path else {
            return Err(crate::row::corrupted(
                "trash_entries.trash_path",
                "missing on a moved_to_trash row",
            ));
        };

        let original = PathBuf::from(&pending.original_path);
        let trash = PathBuf::from(&trash_path);
        // If a `choose(MovedToTrash)` committed the DB but the subsequent
        // `rename` failed, the file is still at `original_path`. There is
        // nothing to restore on disk: reopening the asset is enough.
        let needs_rename = if !trash.exists() && original.exists() {
            false
        } else if original.exists() {
            return Err(DbError::Conflict(
                "the original location is occupied by another file".to_owned(),
            ));
        } else {
            true
        };

        if needs_rename {
            std::fs::rename(&trash, &original).map_err(|e| {
                DbError::Io(format!(
                    "restoring {} to {}: {e}",
                    trash.display(),
                    original.display()
                ))
            })?;
        }

        let mut tx = self.db.pool().begin().await?;
        sqlx::query("UPDATE assets SET status = 'indexed', updated_at = now() WHERE id = $1")
            .bind(asset_id.as_uuid())
            .execute(&mut *tx)
            .await?;
        // The `restored_at IS NULL` condition closes the race between two
        // concurrent restores on the same trash entry: only the first to
        // get here marks it, but both have already passed the "the
        // original path is free" check above — a known, documented
        // residual race, not resolved here.
        sqlx::query(
            "UPDATE trash_entries SET restored_at = now() \
              WHERE id = $1 AND restored_at IS NULL",
        )
        .bind(pending.id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Keyset list of still-recoverable trash entries (`moved_to_trash`
    /// not yet restored), filtered by the caller's visibility.
    ///
    /// # Errors
    /// `Connection` if a query fails.
    pub async fn list_pending(
        &self,
        ctx: &AuthContext,
        cursor: Option<(DateTime<Utc>, TrashEntryId)>,
        limit: i64,
    ) -> Result<Vec<TrashEntry>, DbError> {
        let limit = limit.clamp(1, 100);
        let scope = VisibilityScope::resolve(self.db, ctx).await?;
        let filter = scope.filter("f.path", "f.library_id", "a.id", 4);
        let (cursor_time, cursor_id) = match cursor {
            Some((t, id)) => (Some(t), Some(id.as_uuid())),
            None => (None, None),
        };
        let sql = format!(
            "SELECT {TE_COLUMNS} FROM trash_entries te \
             JOIN assets a ON a.id = te.asset_id \
             JOIN folders f ON f.id = a.folder_id \
             WHERE te.disk_action = 'moved_to_trash' AND te.restored_at IS NULL \
               AND {} \
               AND ($1::timestamptz IS NULL \
                    OR te.deleted_at < $1 \
                    OR (te.deleted_at = $1 AND te.id < $2)) \
             ORDER BY te.deleted_at DESC, te.id DESC \
             LIMIT $3",
            filter.sql()
        );
        let rows: Vec<EntryRow> = sqlx::query_as(&sql)
            .bind(cursor_time)
            .bind(cursor_id)
            .bind(limit)
            .bind(filter.bind())
            .bind(filter.holes())
            .bind(filter.assets())
            .fetch_all(self.db.pool())
            .await?;
        rows.into_iter().map(EntryRow::into_domain).collect()
    }

    /// Immediately empties the trash of libraries visible to the caller.
    /// Owner of at least one library, or admin, only: a user with no
    /// libraries of their own gets `Forbidden`.
    ///
    /// # Errors
    /// `Forbidden` if the caller is not admin and does not own any
    /// library. `Connection` if a query fails.
    pub async fn empty(&self, ctx: &AuthContext) -> Result<u64, DbError> {
        if !ctx.is_admin() {
            let Some(owner_id) = ctx.user_id() else {
                return Err(DbError::Forbidden);
            };
            let owned: i64 =
                sqlx::query_scalar("SELECT count(*) FROM libraries WHERE owner_id = $1")
                    .bind(owner_id.as_uuid())
                    .fetch_one(self.db.pool())
                    .await?;
            if owned == 0 {
                return Err(DbError::Forbidden);
            }
        }

        let scope = VisibilityScope::resolve(self.db, ctx).await?;
        let filter = scope.filter("f.path", "f.library_id", "a.id", 1);
        let rows: Vec<PendingRow> = sqlx::query_as(&format!(
            "SELECT te.id, te.asset_id, te.original_path, te.trash_path \
               FROM trash_entries te \
               JOIN assets a ON a.id = te.asset_id \
               JOIN folders f ON f.id = a.folder_id \
              WHERE te.disk_action = 'moved_to_trash' AND te.restored_at IS NULL \
                AND {}",
            filter.sql()
        ))
        .bind(filter.bind())
        .bind(filter.holes())
        .bind(filter.assets())
        .fetch_all(self.db.pool())
        .await?;

        let mut emptied = 0u64;
        for row in rows {
            if let Some(trash_path) = &row.trash_path
                && let Err(e) = std::fs::remove_file(trash_path)
                && e.kind() != std::io::ErrorKind::NotFound
            {
                tracing::warn!(error = %e, path = %trash_path, "trash empty: cannot remove file, skipping");
                continue;
            }

            let mut tx = self.db.pool().begin().await?;
            sqlx::query("DELETE FROM assets WHERE id = $1 AND status = 'trashed'")
                .bind(row.asset_id)
                .execute(&mut *tx)
                .await?;
            sqlx::query("DELETE FROM trash_entries WHERE id = $1")
                .bind(row.id)
                .execute(&mut *tx)
                .await?;
            tx.commit().await?;
            emptied += 1;
        }
        Ok(emptied)
    }

    /// Nightly trash cleanup: every still-pending `moved_to_trash` entry
    /// older than `before` is deleted from disk and its row removed. Not
    /// yet wired to a scheduled job — whoever calls it will pass
    /// `Utc::now() - Duration::days(30)`.
    ///
    /// Does not take an `AuthContext`: this is system maintenance across
    /// all libraries, like `LibraryRepo::mark_scanned`.
    ///
    /// # Errors
    /// `Connection` if a query fails. A single file that cannot be
    /// deleted (permissions, disk already unmounted) does not abort the
    /// run: it is logged and that row stays for the next attempt.
    pub async fn cleanup_expired(&self, before: DateTime<Utc>) -> Result<u64, DbError> {
        let rows: Vec<PendingRow> = sqlx::query_as(
            "SELECT id, asset_id, original_path, trash_path FROM trash_entries \
              WHERE disk_action = 'moved_to_trash' AND restored_at IS NULL AND deleted_at < $1",
        )
        .bind(before)
        .fetch_all(self.db.pool())
        .await?;

        let mut cleaned = 0u64;
        for row in rows {
            if let Some(trash_path) = &row.trash_path
                && let Err(e) = std::fs::remove_file(trash_path)
                && e.kind() != std::io::ErrorKind::NotFound
            {
                tracing::warn!(error = %e, path = %trash_path, "trash cleanup: cannot remove file, retrying later");
                continue;
            }

            let mut tx = self.db.pool().begin().await?;
            sqlx::query("DELETE FROM assets WHERE id = $1 AND status = 'trashed'")
                .bind(row.asset_id)
                .execute(&mut *tx)
                .await?;
            sqlx::query("DELETE FROM trash_entries WHERE id = $1")
                .bind(row.id)
                .execute(&mut *tx)
                .await?;
            tx.commit().await?;
            cleaned += 1;
        }
        Ok(cleaned)
    }
}

/// Gates shared by [`TrashRepo::choose`] and
/// [`TrashRepo::assert_batch_purge_authorized`], extracted because the
/// latter must be able to run them without writing anything — only checking.
///
/// # Errors
/// `Forbidden` if the caller cannot see the asset — even when the id does
/// not exist — if requesting [`DiskAction::Purged`] without being the
/// library owner or an admin, or if requesting
/// [`DiskAction::MovedToTrash`] / [`DiskAction::Kept`] without editor role.
async fn authorize_choose(
    db: &Db,
    ctx: &AuthContext,
    asset_id: AssetId,
    action: DiskAction,
) -> Result<(Asset, LibraryInfo, PathBuf), DbError> {
    // Gate common to all three options: without visibility on the asset,
    // none of the three is allowed.
    AssetRepo::new(db)
        .assert_visible(ctx, std::slice::from_ref(&asset_id))
        .await?;

    let asset = AssetRepo::new(db).get_for_scan(asset_id).await?;
    let library = library_info_for_folder(db, asset.folder_id).await?;

    // A viewer can see, not write: trash and "kept" require editor+.
    // Purged stays owner/admin — an editor cannot destroy files.
    if !matches!(action, DiskAction::Purged) {
        crate::PermissionRepo::new(db)
            .assert_can_edit_assets(ctx, std::slice::from_ref(&asset_id))
            .await?;
    }

    // Second, narrower gate, only for `Purged`: deleting from disk stays
    // owner/admin even when others have visibility on the asset (an
    // editor cannot destroy files).
    if matches!(action, DiskAction::Purged) && !may_purge(ctx, UserId::from_uuid(library.owner_id))
    {
        return Err(DbError::Forbidden);
    }

    let folder_abs = FolderRepo::new(db)
        .absolute_path_for_scan(asset.folder_id)
        .await?;
    Ok((asset, library, folder_abs))
}

async fn library_info_for_folder(
    db: &Db,
    folder_id: keeppix_domain::FolderId,
) -> Result<LibraryInfo, DbError> {
    let row: Option<LibraryInfo> = sqlx::query_as(
        "SELECT l.root_path, l.owner_id FROM folders f JOIN libraries l ON l.id = f.library_id \
          WHERE f.id = $1",
    )
    .bind(folder_id.as_uuid())
    .fetch_optional(db.pool())
    .await?;
    row.ok_or(DbError::NotFound)
}

async fn insert_entry(
    tx: &mut PgConnection,
    id: TrashEntryId,
    asset_id: AssetId,
    deleted_by: Option<uuid::Uuid>,
    original_path: &Path,
    trash_path: Option<&Path>,
    action: DiskAction,
) -> Result<EntryRow, DbError> {
    let row: EntryRow = sqlx::query_as(&format!(
        "INSERT INTO trash_entries (id, asset_id, deleted_by, original_path, trash_path, disk_action) \
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING {COLUMNS}"
    ))
    .bind(id.as_uuid())
    .bind(asset_id.as_uuid())
    .bind(deleted_by)
    .bind(original_path.to_string_lossy().as_ref())
    .bind(trash_path.map(|p| p.to_string_lossy().into_owned()))
    .bind(action.as_str())
    .fetch_one(&mut *tx)
    .await?;
    Ok(row)
}

/// Only owner and admin can request [`DiskAction::Purged`] (an editor
/// cannot destroy files). Extracted as a pure function, separate from the
/// async resolution of library/visibility, so the rule can be pinned with
/// a direct test without going through the database — visibility alone
/// (only owner or admin, no sharing before this point) would otherwise
/// make this gate indistinguishable from the visibility check that
/// precedes it in [`TrashRepo::choose`].
fn may_purge(ctx: &AuthContext, library_owner: UserId) -> bool {
    ctx.is_admin() || ctx.user_id() == Some(library_owner)
}

fn remove_file_tolerant(path: &Path) -> Result<(), DbError> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(DbError::Io(format!("removing {}: {e}", path.display()))),
    }
}

/// Computes (and creates the folders for) the destination under
/// `<library_root>/.keeppix-trash/`, without moving the file yet.
///
/// The relative subpath stays manually browsable; the name is prefixed
/// with `entry_id` — unique by construction, no collision with another
/// file already trashed with the same basename.
fn prepare_trash_path(
    library_root: &Path,
    folder_abs: &Path,
    entry_id: TrashEntryId,
    filename: &str,
) -> Result<PathBuf, DbError> {
    let relative_dir = folder_abs
        .strip_prefix(library_root)
        .unwrap_or(Path::new(""));
    let target_dir = library_root.join(TRASH_DIR_NAME).join(relative_dir);
    std::fs::create_dir_all(&target_dir)
        .map_err(|e| DbError::Io(format!("creating {}: {e}", target_dir.display())))?;
    Ok(target_dir.join(format!("{entry_id}__{filename}")))
}

#[cfg(test)]
mod tests {
    use keeppix_domain::SystemRole;

    use super::*;

    #[test]
    fn an_admin_may_purge_a_library_they_do_not_own() {
        let stranger_admin = AuthContext::user(UserId::new(), SystemRole::Admin);
        assert!(may_purge(&stranger_admin, UserId::new()));
    }

    #[test]
    fn the_owner_may_purge_their_own_library_even_without_the_admin_role() {
        let owner = UserId::new();
        let ctx = AuthContext::user(owner, SystemRole::User);
        assert!(may_purge(&ctx, owner));
    }

    #[test]
    fn a_plain_user_who_is_neither_owner_nor_admin_may_not_purge() {
        let ctx = AuthContext::user(UserId::new(), SystemRole::User);
        assert!(!may_purge(&ctx, UserId::new()));
    }
}
