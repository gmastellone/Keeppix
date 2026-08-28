use std::path::{Component, Path, PathBuf};

use keeppix_domain::{
    AuthContext, CullingRole, Folder, FolderId, FolderPath, Library, LibraryId, ObjectRole,
};
use sqlx::PgConnection;

use crate::visibility::VisibilityScope;
use crate::{Db, DbError, LibraryRepo};

pub struct FolderRepo<'a> {
    db: &'a Db,
}

#[derive(sqlx::FromRow)]
struct FolderRow {
    id: uuid::Uuid,
    library_id: uuid::Uuid,
    parent_id: Option<uuid::Uuid>,
    name: String,
    path: String,
    depth: i32,
    culling_role: Option<String>,
}

impl FolderRow {
    fn into_domain(self) -> Result<Folder, DbError> {
        Ok(Folder {
            id: FolderId::from_uuid(self.id),
            library_id: LibraryId::from_uuid(self.library_id),
            parent_id: self.parent_id.map(FolderId::from_uuid),
            name: self.name,
            path: FolderPath::parse(&self.path)
                .map_err(|e| crate::row::corrupted("folder path", e))?,
            depth: self.depth,
            culling_role: parse_culling_role(self.culling_role.as_deref())?,
        })
    }
}

fn parse_culling_role(raw: Option<&str>) -> Result<Option<CullingRole>, DbError> {
    match raw {
        None => Ok(None),
        Some("taken") => Ok(Some(CullingRole::Taken)),
        Some("skipped") => Ok(Some(CullingRole::Skipped)),
        Some(other) => Err(crate::row::corrupted("folders.culling_role", other)),
    }
}

// `path::text` because `ltree` has no sqlx decoding: the row carries a
// String that `FolderPath::parse` validates.
const COLUMNS: &str = "id, library_id, parent_id, name, path::text AS path, depth, culling_role";

async fn load(conn: &mut PgConnection, id: FolderId) -> Result<Option<Folder>, DbError> {
    let row: Option<FolderRow> =
        sqlx::query_as(&format!("SELECT {COLUMNS} FROM folders WHERE id = $1"))
            .bind(id.as_uuid())
            .fetch_optional(&mut *conn)
            .await?;

    row.map(FolderRow::into_domain).transpose()
}

/// Creates the library's root if it doesn't exist, and returns it.
///
/// The `ltree` label comes from the library's counter, in the same
/// statement as the insert: the `UPDATE` locks the `libraries` row, so two
/// concurrent scans of the same library serialize instead of racing on the
/// label.
async fn ensure_root_on(conn: &mut PgConnection, library_id: LibraryId) -> Result<Folder, DbError> {
    sqlx::query(
        "WITH label AS ( \
             UPDATE libraries SET next_folder_seq = next_folder_seq + 1 \
              WHERE id = $2 \
             RETURNING next_folder_seq - 1 AS seq \
         ) \
         INSERT INTO folders (id, library_id, parent_id, name, path, depth) \
         SELECT $1, $2, NULL::uuid, ''::text, label.seq::text::ltree, 1 FROM label \
         ON CONFLICT (library_id) WHERE parent_id IS NULL DO NOTHING",
    )
    .bind(FolderId::new().as_uuid())
    .bind(library_id.as_uuid())
    .execute(&mut *conn)
    .await?;

    // Re-read instead of `RETURNING`: with `DO NOTHING` the existing row is
    // not returned, and the existing row is exactly what's needed.
    let row: Option<FolderRow> = sqlx::query_as(&format!(
        "SELECT {COLUMNS} FROM folders WHERE library_id = $1 AND parent_id IS NULL"
    ))
    .bind(library_id.as_uuid())
    .fetch_optional(&mut *conn)
    .await?;

    row.ok_or(DbError::NotFound)?.into_domain()
}

/// Same as above for a child: the parent's path is re-read from the
/// database, so a `Folder` the caller has been holding since before a move
/// does not produce a wrong path.
async fn ensure_child_on(
    conn: &mut PgConnection,
    parent: &Folder,
    name: &str,
) -> Result<Folder, DbError> {
    sqlx::query(
        "WITH label AS ( \
             UPDATE libraries SET next_folder_seq = next_folder_seq + 1 \
              WHERE id = (SELECT library_id FROM folders WHERE id = $2) \
             RETURNING next_folder_seq - 1 AS seq \
         ) \
         INSERT INTO folders (id, library_id, parent_id, name, path, depth) \
         SELECT $1, p.library_id, p.id, $3::text, p.path || label.seq::text::ltree, p.depth + 1 \
           FROM folders p, label WHERE p.id = $2 \
         ON CONFLICT (parent_id, name) WHERE parent_id IS NOT NULL DO NOTHING",
    )
    .bind(FolderId::new().as_uuid())
    .bind(parent.id.as_uuid())
    .bind(name)
    .execute(&mut *conn)
    .await?;

    let row: Option<FolderRow> = sqlx::query_as(&format!(
        "SELECT {COLUMNS} FROM folders WHERE parent_id = $1 AND name = $2"
    ))
    .bind(parent.id.as_uuid())
    .bind(name)
    .fetch_optional(&mut *conn)
    .await?;

    row.ok_or(DbError::NotFound)?.into_domain()
}

/// Same as `ensure_child_on`, but marks the folder with `culling_role`:
/// `_taken`/`_skipped` are recognized by the column, not by name.
/// Self-heals if the folder already existed without a role (created by
/// hand before Keeppix needed it as special, or by an earlier version of
/// this function): the `UPDATE` after the ignored `INSERT` marks it
/// anyway, instead of leaving a `_taken` that behaves like an ordinary
/// folder.
async fn ensure_culling_child_on(
    conn: &mut PgConnection,
    parent: &Folder,
    name: &str,
    role: CullingRole,
) -> Result<Folder, DbError> {
    sqlx::query(
        "WITH label AS ( \
             UPDATE libraries SET next_folder_seq = next_folder_seq + 1 \
              WHERE id = (SELECT library_id FROM folders WHERE id = $2) \
             RETURNING next_folder_seq - 1 AS seq \
         ) \
         INSERT INTO folders (id, library_id, parent_id, name, path, depth, culling_role) \
         SELECT $1, p.library_id, p.id, $3::text, p.path || label.seq::text::ltree, p.depth + 1, $4 \
           FROM folders p, label WHERE p.id = $2 \
         ON CONFLICT (parent_id, name) WHERE parent_id IS NOT NULL DO NOTHING",
    )
    .bind(FolderId::new().as_uuid())
    .bind(parent.id.as_uuid())
    .bind(name)
    .bind(role.as_str())
    .execute(&mut *conn)
    .await?;

    sqlx::query(
        "UPDATE folders SET culling_role = $3 \
          WHERE parent_id = $1 AND name = $2 AND culling_role IS NULL",
    )
    .bind(parent.id.as_uuid())
    .bind(name)
    .bind(role.as_str())
    .execute(&mut *conn)
    .await?;

    let row: Option<FolderRow> = sqlx::query_as(&format!(
        "SELECT {COLUMNS} FROM folders WHERE parent_id = $1 AND name = $2"
    ))
    .bind(parent.id.as_uuid())
    .bind(name)
    .fetch_optional(&mut *conn)
    .await?;

    row.ok_or(DbError::NotFound)?.into_domain()
}

impl<'a> FolderRepo<'a> {
    #[must_use]
    pub const fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// The library's root, creating it if it does not exist.
    ///
    /// Does not take an `AuthContext` because the scanner calls this, and
    /// it does not act on behalf of a user — like
    /// `LibraryRepo::mark_scanned`.
    ///
    /// # Errors
    /// `NotFound` if the library does not exist.
    pub async fn ensure_root(&self, library_id: LibraryId) -> Result<Folder, DbError> {
        let mut conn = self.db.pool().acquire().await?;
        ensure_root_on(&mut conn, library_id).await
    }

    /// The child named `name`, creating it if it does not exist.
    ///
    /// Idempotent even under concurrency: `ON CONFLICT DO NOTHING` plus a
    /// re-read, not a `SELECT` followed by an `INSERT`. No `AuthContext`
    /// for the same reason as `ensure_root`.
    ///
    /// # Errors
    /// `NotFound` if the parent no longer exists.
    pub async fn ensure_child(&self, parent: &Folder, name: &str) -> Result<Folder, DbError> {
        let mut conn = self.db.pool().acquire().await?;
        ensure_child_on(&mut conn, parent, name).await
    }

    /// `_taken`/`_skipped` inside a culling lot, created if missing,
    /// always marked with the right `culling_role` — never recognized by
    /// name.
    ///
    /// # Errors
    /// Same as `ensure_child`.
    pub async fn ensure_culling_child(
        &self,
        parent: &Folder,
        role: CullingRole,
    ) -> Result<Folder, DbError> {
        let name = match role {
            CullingRole::Taken => "_taken",
            CullingRole::Skipped => "_skipped",
        };
        let mut conn = self.db.pool().acquire().await?;
        ensure_culling_child_on(&mut conn, parent, name, role).await
    }

    /// Creates the entire `relative` chain under the library's root and
    /// returns the last folder. No `AuthContext`: the scanner calls this.
    ///
    /// Everything in one transaction: a scan interrupted halfway through
    /// does not leave a branch missing a piece.
    ///
    /// # Errors
    /// `NotFound` if the library does not exist.
    pub async fn ensure_path(
        &self,
        library_id: LibraryId,
        relative: &[&str],
    ) -> Result<Folder, DbError> {
        let mut tx = self.db.pool().begin().await?;

        let mut current = ensure_root_on(&mut tx, library_id).await?;
        for name in relative {
            current = ensure_child_on(&mut tx, &current, name).await?;
        }

        tx.commit().await?;
        Ok(current)
    }

    /// # Errors
    /// `Forbidden` if the caller cannot see the folder's library — even
    /// when the id does not exist, so as not to offer an existence oracle.
    /// `NotFound` only for an admin requesting a nonexistent id.
    pub async fn find_by_id(&self, ctx: &AuthContext, id: FolderId) -> Result<Folder, DbError> {
        Ok(self.visible(ctx, id).await?.0)
    }

    /// Same as `find_by_id`, plus the library that contains it — the same
    /// visibility gate, not the narrower owner/admin gate of
    /// `LibraryRepo::find_by_id`. Used by `CullingRepo::set_pick` to read
    /// `culling_root_folder_id` without restricting who can set
    /// pick/reject to owner/admin: that permission stays the usual one
    /// (visibility) — the physical move inside a lot narrows it on its own
    /// via `AssetRepo::move_asset`.
    ///
    /// # Errors
    /// Same as `find_by_id`.
    pub async fn find_with_library(
        &self,
        ctx: &AuthContext,
        id: FolderId,
    ) -> Result<(Folder, Library), DbError> {
        self.visible(ctx, id).await
    }

    /// Write permission on the folder: library owner, admin, or an
    /// explicit `editor` via `PermissionRepo::effective_role` — the same
    /// gate as `move_subtree` and `UploadSessionRepo::create`, now
    /// reusable without duplicating it a third time for `WebDAV`
    /// `PUT`/`MKCOL`/`MOVE`/`COPY`.
    ///
    /// # Errors
    /// `Forbidden` if the caller can see the folder but cannot write to it
    /// (a viewer, or no permission) — never `NotFound`, so as not to
    /// offer an existence oracle to someone probing an id that isn't
    /// theirs. Otherwise same as `find_by_id`.
    pub async fn assert_editor(
        &self,
        ctx: &AuthContext,
        folder_id: FolderId,
    ) -> Result<(Folder, Library), DbError> {
        let (folder, library) = self.visible(ctx, folder_id).await?;
        if !ctx.is_admin() && ctx.user_id() != Some(library.owner_id) {
            match crate::PermissionRepo::new(self.db)
                .effective_role(ctx, folder_id)
                .await?
            {
                Some(ObjectRole::Editor) => {}
                _ => return Err(DbError::Forbidden),
            }
        }
        Ok((folder, library))
    }

    /// The tree visible to the caller, in `path` order.
    ///
    /// # Errors
    /// `Connection` if the query fails.
    pub async fn tree(&self, ctx: &AuthContext) -> Result<Vec<Folder>, DbError> {
        let scope = VisibilityScope::resolve(self.db, ctx).await?;
        let filter = scope.filter("folders.path", "folders.library_id", "NULL::uuid", 1);
        let rows: Vec<FolderRow> = sqlx::query_as(&format!(
            "SELECT {COLUMNS} FROM folders WHERE {} ORDER BY path",
            filter.sql()
        ))
        .bind(filter.bind())
        .bind(filter.holes())
        .bind(filter.assets())
        .fetch_all(self.db.pool())
        .await?;

        rows.into_iter().map(FolderRow::into_domain).collect()
    }

    /// Roots of the visible forest: owned libraries for the owner, granted
    /// folders for anyone with a share. Not the whole tree.
    ///
    /// # Errors
    /// `Connection` if the query fails.
    pub async fn roots(&self, ctx: &AuthContext) -> Result<Vec<Folder>, DbError> {
        let scope = VisibilityScope::resolve(self.db, ctx).await?;
        let filter = scope.filter("folders.path", "folders.library_id", "NULL::uuid", 1);
        let sql = if scope.is_unrestricted() {
            format!(
                "SELECT {COLUMNS} FROM folders WHERE parent_id IS NULL AND {} ORDER BY name",
                filter.sql()
            )
        } else {
            format!(
                "SELECT {COLUMNS} FROM folders WHERE id = ANY($1::uuid[]) AND {} ORDER BY name",
                filter.sql()
            )
        };
        let rows: Vec<FolderRow> = sqlx::query_as(&sql)
            .bind(filter.bind())
            .bind(filter.holes())
            .bind(filter.assets())
            .fetch_all(self.db.pool())
            .await?;

        rows.into_iter().map(FolderRow::into_domain).collect()
    }

    /// Direct children, in name order.
    ///
    /// # Errors
    /// Same as `find_by_id`.
    pub async fn children(
        &self,
        ctx: &AuthContext,
        folder_id: FolderId,
    ) -> Result<Vec<Folder>, DbError> {
        self.visible(ctx, folder_id).await?;

        let scope = VisibilityScope::resolve(self.db, ctx).await?;
        let filter = scope.filter("folders.path", "folders.library_id", "NULL::uuid", 2);
        let rows: Vec<FolderRow> = sqlx::query_as(&format!(
            "SELECT {COLUMNS} FROM folders WHERE parent_id = $1 AND {} ORDER BY name",
            filter.sql()
        ))
        .bind(folder_id.as_uuid())
        .bind(filter.bind())
        .bind(filter.holes())
        .bind(filter.assets())
        .fetch_all(self.db.pool())
        .await?;

        rows.into_iter().map(FolderRow::into_domain).collect()
    }

    /// The folder and all its descendants: an indexed condition
    /// (`path <@ prefix`) instead of a recursion.
    ///
    /// # Errors
    /// Same as `find_by_id`.
    pub async fn subtree(
        &self,
        ctx: &AuthContext,
        folder_id: FolderId,
    ) -> Result<Vec<Folder>, DbError> {
        let (folder, _) = self.visible(ctx, folder_id).await?;
        let scope = VisibilityScope::resolve(self.db, ctx).await?;
        let filter = scope.filter("folders.path", "folders.library_id", "NULL::uuid", 3);

        let rows: Vec<FolderRow> = sqlx::query_as(&format!(
            "SELECT {COLUMNS} FROM folders \
              WHERE library_id = $1 AND path <@ $2::text::ltree AND {} \
              ORDER BY path",
            filter.sql()
        ))
        .bind(folder.library_id.as_uuid())
        .bind(folder.path.as_str())
        .bind(filter.bind())
        .bind(filter.holes())
        .bind(filter.assets())
        .fetch_all(self.db.pool())
        .await?;

        rows.into_iter().map(FolderRow::into_domain).collect()
    }

    /// Moves the folder, and with it the entire subtree, under `new_parent`.
    ///
    /// Descendant paths are rewritten with **one** UPDATE: moving a folder
    /// with 40,000 photos touches `folders` rows, not `assets` rows, which
    /// is why no asset carries a denormalized absolute path.
    ///
    /// # Errors
    /// `Conflict` if `new_parent` is inside the subtree being moved
    /// (including the folder itself): the subtree would get disconnected
    /// and unreachable from any root. `Conflict` also if the two folders
    /// are in different libraries, or if the new parent already has a
    /// child with that name. Otherwise same as `find_by_id`.
    pub async fn move_subtree(
        &self,
        ctx: &AuthContext,
        folder_id: FolderId,
        new_parent: FolderId,
    ) -> Result<(), DbError> {
        let (folder, library) = self.visible(ctx, folder_id).await?;
        self.visible(ctx, new_parent).await?;
        if !ctx.is_admin() && ctx.user_id() != Some(library.owner_id) {
            match crate::PermissionRepo::new(self.db)
                .effective_role(ctx, folder_id)
                .await?
            {
                Some(keeppix_domain::ObjectRole::Editor) => {}
                _ => return Err(DbError::Forbidden),
            }
        }

        let mut tx = self.db.pool().begin().await?;

        // The `libraries` row is the tree's lock: the `ensure_*` functions
        // take it too, when incrementing the counter. Without it, between
        // the cycle check and the UPDATE another move could turn
        // `new_parent` into a descendant, disconnecting the subtree. A
        // single lock per library, so no deadlock.
        sqlx::query("SELECT 1 FROM libraries WHERE id = $1 FOR UPDATE")
            .bind(folder.library_id.as_uuid())
            .execute(&mut *tx)
            .await?;

        // Re-read under the lock: the paths seen earlier may already be stale.
        let folder = match load(&mut tx, folder_id).await? {
            Some(folder) => folder,
            None if ctx.is_admin() => return Err(DbError::NotFound),
            None => return Err(DbError::Forbidden),
        };
        let parent = match load(&mut tx, new_parent).await? {
            Some(parent) => parent,
            None if ctx.is_admin() => return Err(DbError::NotFound),
            None => return Err(DbError::Forbidden),
        };

        if parent.library_id != folder.library_id {
            return Err(DbError::Conflict(
                "a folder cannot be moved to another library".to_owned(),
            ));
        }
        // The check goes before the UPDATE. `is_descendant_of` has `<@`
        // semantics, so it also covers moving a folder into itself; and
        // since every folder descends from the root, the root cannot be
        // moved.
        if parent.path.is_descendant_of(&folder.path) {
            return Err(DbError::Conflict(
                "a folder cannot be moved inside its own subtree".to_owned(),
            ));
        }

        // `subpath(path, nlevel(old) - 1)` keeps the moved folder's own
        // label and everything under it, and reattaches it to the new
        // parent. Labels are unique per library, so the new path cannot
        // collide.
        sqlx::query(
            "UPDATE folders \
                SET path  = $3::text::ltree || subpath(path, nlevel($2::text::ltree) - 1), \
                    depth = nlevel($3::text::ltree) + nlevel(path) \
                            - nlevel($2::text::ltree) + 1, \
                    parent_id = CASE WHEN id = $4 THEN $5 ELSE parent_id END \
              WHERE library_id = $1 AND path <@ $2::text::ltree",
        )
        .bind(folder.library_id.as_uuid())
        .bind(folder.path.as_str())
        .bind(parent.path.as_str())
        .bind(folder.id.as_uuid())
        .bind(parent.id.as_uuid())
        .execute(&mut *tx)
        .await
        .map_err(map_sibling_name_conflict)?;

        tx.commit().await?;
        Ok(())
    }

    /// Removes the folder and its entire subtree **from the database
    /// only** (`WebDAV DELETE`): the caller must have already trashed
    /// every asset in the subtree (`TrashRepo::choose` with
    /// `DiskAction::MovedToTrash` — never an `rm -rf`) before calling this
    /// method, and removes the physical directory separately afterward,
    /// like `dav::delete::folder`.
    ///
    /// # Errors
    /// `403` if the caller is not editor on the folder. Otherwise same as
    /// `assert_editor`.
    pub async fn delete_subtree(
        &self,
        ctx: &AuthContext,
        folder_id: FolderId,
    ) -> Result<(), DbError> {
        let (folder, _library) = self.assert_editor(ctx, folder_id).await?;
        sqlx::query("DELETE FROM folders WHERE library_id = $1 AND path <@ $2::text::ltree")
            .bind(folder.library_id.as_uuid())
            .bind(folder.path.as_str())
            .execute(self.db.pool())
            .await?;
        Ok(())
    }

    /// The folder's on-disk path, reconstructed by walking up the tree.
    ///
    /// Names come from the database, never from the client.
    ///
    /// # Errors
    /// `Corrupted` if a stored name is not a single path component.
    /// Otherwise same as `find_by_id`.
    pub async fn absolute_path(
        &self,
        ctx: &AuthContext,
        folder_id: FolderId,
    ) -> Result<PathBuf, DbError> {
        let (_folder, library) = self.visible(ctx, folder_id).await?;
        self.join_folder_path(library.root_path, folder_id).await
    }

    /// Same as `absolute_path`, for the scanner: no `AuthContext`.
    ///
    /// # Errors
    /// `NotFound` if the folder does not exist; `Corrupted` on an illegal name.
    pub async fn absolute_path_for_scan(&self, folder_id: FolderId) -> Result<PathBuf, DbError> {
        let mut conn = self.db.pool().acquire().await?;
        let Some(folder) = load(&mut conn, folder_id).await? else {
            return Err(DbError::NotFound);
        };
        drop(conn);
        let library = LibraryRepo::new(self.db)
            .load_for_scan(folder.library_id)
            .await?;
        self.join_folder_path(library.root_path, folder_id).await
    }

    async fn join_folder_path(
        &self,
        mut path: PathBuf,
        folder_id: FolderId,
    ) -> Result<PathBuf, DbError> {
        let names: Vec<String> = sqlx::query_scalar(
            "WITH RECURSIVE up AS ( \
                 SELECT id, parent_id, name, 0 AS lvl FROM folders WHERE id = $1 \
                 UNION ALL \
                 SELECT p.id, p.parent_id, p.name, up.lvl + 1 \
                   FROM folders p JOIN up ON p.id = up.parent_id \
             ) \
             SELECT name FROM up WHERE name <> '' ORDER BY lvl DESC",
        )
        .bind(folder_id.as_uuid())
        .fetch_all(self.db.pool())
        .await?;
        for name in names {
            if !is_single_component(&name) {
                return Err(crate::row::corrupted("folder name", name));
            }
            path.push(name);
        }
        Ok(path)
    }

    /// Resolves visibility from the scope (folder prefixes), not just
    /// library ownership: a shared folder must be reachable. `Forbidden`
    /// takes priority over `NotFound`. Also returns the library, needed
    /// by `absolute_path`.
    async fn visible(&self, ctx: &AuthContext, id: FolderId) -> Result<(Folder, Library), DbError> {
        let mut conn = self.db.pool().acquire().await?;

        let Some(folder) = load(&mut conn, id).await? else {
            return Err(if ctx.is_admin() {
                DbError::NotFound
            } else {
                DbError::Forbidden
            });
        };
        drop(conn);

        let scope = VisibilityScope::resolve(self.db, ctx).await?;
        if !scope.allows(folder.library_id, folder.path.as_str()) {
            return Err(if ctx.is_admin() {
                DbError::NotFound
            } else {
                DbError::Forbidden
            });
        }

        let library = LibraryRepo::new(self.db)
            .load_for_scan(folder.library_id)
            .await?;
        Ok((folder, library))
    }
}

/// A folder name must be a single component: not empty, not `.`, not
/// `..`, and containing no separators.
fn is_single_component(name: &str) -> bool {
    let mut components = Path::new(name).components();
    matches!(
        (components.next(), components.next()),
        (Some(Component::Normal(_)), None)
    )
}

fn map_sibling_name_conflict(err: sqlx::Error) -> DbError {
    if let sqlx::Error::Database(ref db_err) = err
        && db_err.code().as_deref() == Some("23505")
    {
        return DbError::Conflict("the destination already has a folder with this name".to_owned());
    }
    DbError::Connection(err)
}

#[cfg(test)]
mod tests {
    use super::is_single_component;

    #[test]
    fn a_plain_name_is_a_single_component() {
        assert!(is_single_component("Matrimonio Rossi"));
        assert!(is_single_component("2024"));
    }

    #[test]
    fn traversal_and_separators_are_rejected() {
        // If any of these passed, `absolute_path` would escape the library.
        assert!(!is_single_component(""));
        assert!(!is_single_component("."));
        assert!(!is_single_component(".."));
        assert!(!is_single_component("../etc"));
        assert!(!is_single_component("a/b"));
        assert!(!is_single_component("/etc"));
    }
}
