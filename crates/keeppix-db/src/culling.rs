//! Folder-based culling: the designated root, the lots underneath it, and
//! the physical move that accompanies pick/reject.

use keeppix_domain::{
    Asset, AssetId, AuthContext, CullingLot, CullingRole, DiskAction, Folder, FolderId, LibraryId,
    Pick,
};

use crate::{AssetRepo, Db, DbError, FlagRepo, FolderRepo, LibraryRepo, TrashRepo};

pub struct CullingRepo<'a> {
    db: &'a Db,
}

#[derive(sqlx::FromRow)]
struct LotRow {
    id: uuid::Uuid,
    name: String,
    created_at: chrono::DateTime<chrono::Utc>,
    pending: i64,
    taken: i64,
    skipped: i64,
}

impl LotRow {
    fn into_domain(self) -> CullingLot {
        CullingLot {
            folder_id: FolderId::from_uuid(self.id),
            name: self.name,
            created_at: self.created_at,
            pending: self.pending,
            taken: self.taken,
            skipped: self.skipped,
        }
    }
}

impl<'a> CullingRepo<'a> {
    #[must_use]
    pub const fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// The lots under the library's culling root, most recent first —
    /// empty if no root has been designated yet (without a root, culling
    /// behaves exactly as before, no new behavior is forced).
    ///
    /// Scope is **owner/admin**, not the general folder-visibility scope:
    /// `LibraryRepo::find_by_id` (which resolves `culling_root_folder_id`)
    /// is already owner-or-admin by construction, and culling is a
    /// personal workflow of the owner (no notion of sharing the area). If
    /// sharing a lot with an editor is ever needed, that decision can be
    /// made then — it is not anticipated here without a real requirement.
    ///
    /// The three counts are independent per-lot subqueries, not a `JOIN` +
    /// `COUNT(DISTINCT ..)`: with three `LEFT JOIN`s (root/`_taken`/
    /// `_skipped`) the cartesian product across the three asset sets would
    /// needlessly bloat the intermediate rows — cheap does not mean "one
    /// query at all costs," it means "per lot, not per library": each
    /// subquery stays an indexed access on `assets.folder_id`.
    ///
    /// # Errors
    /// Same as `LibraryRepo::find_by_id`.
    pub async fn list_lots(
        &self,
        ctx: &AuthContext,
        library_id: LibraryId,
    ) -> Result<Vec<CullingLot>, DbError> {
        let library = LibraryRepo::new(self.db)
            .find_by_id(ctx, library_id)
            .await?;
        let Some(root_id) = library.culling_root_folder_id else {
            return Ok(Vec::new());
        };

        let rows: Vec<LotRow> = sqlx::query_as(
            "SELECT \
                lot.id, lot.name, lot.created_at, \
                (SELECT COUNT(*) FROM assets a \
                  WHERE a.folder_id = lot.id AND a.status = 'indexed') AS pending, \
                (SELECT COUNT(*) FROM assets a JOIN folders tf ON tf.id = a.folder_id \
                  WHERE tf.parent_id = lot.id AND tf.culling_role = 'taken' \
                    AND a.status = 'indexed') AS taken, \
                (SELECT COUNT(*) FROM assets a JOIN folders sf ON sf.id = a.folder_id \
                  WHERE sf.parent_id = lot.id AND sf.culling_role = 'skipped' \
                    AND a.status = 'indexed') AS skipped \
             FROM folders lot \
             WHERE lot.parent_id = $1 \
             ORDER BY lot.created_at DESC",
        )
        .bind(root_id.as_uuid())
        .fetch_all(self.db.pool())
        .await?;

        Ok(rows.into_iter().map(LotRow::into_domain).collect())
    }

    /// Pick/reject/clear. The flag always changes; **if and only if** the
    /// asset is already inside a culling lot (a direct descendant of the
    /// designated root: in the lot itself, or already in
    /// `_taken`/`_skipped`) does the physical move accompany the change.
    ///
    /// Permission: **not** a single gate for the whole call. Outside a lot
    /// the flag remains settable by anyone who can see the asset — the
    /// same permission as today (`FlagRepo::set`), unchanged (outside a
    /// lot it stays just a flag, as before). Inside a lot the physical
    /// move goes through `AssetRepo::move_asset`, which requires `editor`
    /// on both folders on its own — if the caller is not, the whole call
    /// fails with `Forbidden` before touching the flag: a move the
    /// interface claims happened but did not occur on disk would be worse
    /// than a rejection.
    ///
    /// The physical move happens **before** the flag write: a failure
    /// (permission, name collision) leaves the flag untouched instead of
    /// lying about where the file is.
    ///
    /// # Errors
    /// Same as `AssetRepo::find_by_id`, then same as `AssetRepo::move_asset`
    /// if the asset is inside a lot, then same as `FlagRepo::set`.
    pub async fn set_pick(
        &self,
        ctx: &AuthContext,
        asset_id: AssetId,
        pick: Pick,
    ) -> Result<Asset, DbError> {
        let assets = AssetRepo::new(self.db);
        let folders = FolderRepo::new(self.db);
        let flags = FlagRepo::new(self.db);

        let asset = assets.find_by_id(ctx, asset_id).await?;
        let (folder, library) = folders.find_with_library(ctx, asset.folder_id).await?;

        if let Some(root_id) = library.culling_root_folder_id
            && let Some(lot) = culling_lot_of(&folders, ctx, &folder, root_id).await?
        {
            let target = match pick {
                Pick::Pick => {
                    provision_culling_child(&folders, ctx, &lot, CullingRole::Taken).await?
                }
                Pick::Reject => {
                    provision_culling_child(&folders, ctx, &lot, CullingRole::Skipped).await?
                }
                Pick::None => lot,
            };
            assets
                .move_asset(ctx, asset_id, target.id, asset.filename.clone())
                .await?;
        }

        let mut current = flags.get(ctx, asset_id).await?;
        current.pick = pick;
        flags.set(ctx, asset_id, &current).await?;

        assets.find_by_id(ctx, asset_id).await
    }

    /// "Empty rejected": **permanently** deletes from disk every asset
    /// currently in `_skipped` inside this lot. Reuses `TrashRepo::choose`
    /// with `DiskAction::Purged` instead of duplicating its logic: same
    /// owner/admin gate ("an editor cannot destroy files"), same
    /// row-then-file ordering, same audit trail in `trash_entries`.
    /// Confirmation is the caller's responsibility (API/UI), not this
    /// method's — there is nothing to confirm twice here.
    ///
    /// **Partial** success, never a silent all-or-nothing block — same
    /// principle already applied to bulk operations: an asset whose purge
    /// fails does not prevent the others from being deleted. The returned
    /// vector carries the outcome per asset, in the stable order of
    /// `find_by_folder` (by filename); the HTTP caller translates this
    /// into a `BulkOutcome`.
    ///
    /// **Authorization**, however, stays all-or-nothing, as in
    /// `TrashRepo::batch_delete` for `Purged`: a caller who cannot destroy
    /// even one asset in the lot never gets to touch any of them — no
    /// file disappears while the request is being rejected halfway
    /// through.
    ///
    /// # Errors
    /// Same as `FolderRepo::find_by_id` on the lot, or
    /// `ensure_culling_child` if the lot itself cannot be resolved, then
    /// same as `TrashRepo::assert_batch_purge_authorized` if the caller is
    /// not owner/admin. Failures on individual assets during the actual
    /// purge are carried in the `Result` of each returned tuple, not here.
    pub async fn empty_skipped(
        &self,
        ctx: &AuthContext,
        lot_folder_id: FolderId,
    ) -> Result<Vec<(AssetId, Result<(), DbError>)>, DbError> {
        let folders = FolderRepo::new(self.db);
        let assets = AssetRepo::new(self.db);
        let trash = TrashRepo::new(self.db);

        let lot = folders.find_by_id(ctx, lot_folder_id).await?;
        let skipped = folders
            .ensure_culling_child(&lot, CullingRole::Skipped)
            .await?;
        let victims = assets.find_by_folder(ctx, skipped.id).await?;
        let victim_ids: Vec<AssetId> = victims.iter().map(|a| a.id).collect();
        trash
            .assert_batch_purge_authorized(ctx, &victim_ids)
            .await?;

        let mut results = Vec::with_capacity(victims.len());
        for victim in victims {
            let outcome = trash
                .choose(ctx, victim.id, DiskAction::Purged)
                .await
                .map(|_| ());
            results.push((victim.id, outcome));
        }
        Ok(results)
    }
}

/// `FolderRepo::ensure_culling_child` only creates the row: no repository
/// in `keeppix-db` touches the filesystem to create folders, except where
/// the operation itself is physical (`TrashRepo`, `UploadSessionRepo`) —
/// and moving a file into `_taken`/`_skipped`, which may never have
/// existed before now, is exactly that case. Same ordering as
/// `dav::write::mkcol`: directory on disk **before** the row, never the
/// other way around — if the `INSERT` failed after a successful
/// `create_dir_all`, a ghost folder would be left on disk without a
/// matching row, not the silent reverse (a row with no folder, where
/// `rename` fails on the next attempt). Best-effort rollback of only the
/// directory created by this call, never of a pre-existing one.
async fn provision_culling_child(
    folders: &FolderRepo<'_>,
    ctx: &AuthContext,
    lot: &Folder,
    role: CullingRole,
) -> Result<Folder, DbError> {
    let name = match role {
        CullingRole::Taken => "_taken",
        CullingRole::Skipped => "_skipped",
    };
    let target_dir = folders.absolute_path(ctx, lot.id).await?.join(name);
    let already_on_disk = tokio::fs::metadata(&target_dir).await.is_ok();
    tokio::fs::create_dir_all(&target_dir)
        .await
        .map_err(|err| DbError::Io(format!("creating {}: {err}", target_dir.display())))?;

    match folders.ensure_culling_child(lot, role).await {
        Ok(child) => Ok(child),
        Err(err) => {
            if !already_on_disk {
                let _ = tokio::fs::remove_dir(&target_dir).await;
            }
            Err(err)
        }
    }
}

/// The lot that contains `folder`, if `folder` is a direct descendant of
/// the designated culling root — the lot itself (pending assets), or one
/// of its two children `_taken`/`_skipped`. `None` for any other folder,
/// including the root itself: only the two-level structure built for
/// culling counts, never recognized by name.
async fn culling_lot_of(
    folders: &FolderRepo<'_>,
    ctx: &AuthContext,
    folder: &Folder,
    root_id: FolderId,
) -> Result<Option<Folder>, DbError> {
    if folder.parent_id == Some(root_id) {
        return Ok(Some(folder.clone()));
    }
    if folder.culling_role.is_none() {
        return Ok(None);
    }
    let Some(lot_id) = folder.parent_id else {
        return Ok(None);
    };
    let lot = folders.find_by_id(ctx, lot_id).await?;
    Ok((lot.parent_id == Some(root_id)).then_some(lot))
}
