//! Formula-based renaming: value resolution per asset, the three scopes,
//! co-renaming the side-by-side files of a stack, applying with an audit
//! trail for undo, and progress/cancellation of a long-running operation
//! (`OperationKind::BulkRename`). The actual engine
//! (`render_base`/`apply_base_to_filename`) is pure, in `keeppix-domain`.
//!
//! **`apply`/`undo` act as the "worker" of their own operation**, but not
//! always in the same way. Original design: unlike
//! `LibraryScan`/`AiAnalysis`/`FaceDetection` (driven by a `keeppix-jobs`
//! job, because they are slow and tied to model inference), renaming was
//! fast — each step just a `move_asset` — so it stayed synchronous inside
//! the HTTP request; a batch of thousands of photos on slow storage still
//! turned into a multi-minute block with no way to cancel it, discovered
//! when someone asked for a real cancellation. **`apply` now runs inside
//! `keeppix-jobs::rename_batch`** (the same shape as `LibraryScan`): the
//! HTTP caller (`routes/rename.rs::apply_batch`) does the fallible checks
//! upstream (permissions/visibility), creates the `Operation`, enqueues
//! the job, and responds `202` with the id right away — no early `Err`
//! can still leave an orphaned operation; the safety property is the
//! same, just moved from inside `apply` to the caller. `undo` stays
//! synchronous as before (out of scope for this change, same
//! `track_operation: bool`, untouched) — declared debt, not forgotten: if
//! an undoable batch can reach the same scale as `apply`, it deserves the
//! same treatment. From there on, both poll `is_cancel_requested` between
//! one asset and the next and close their own final state
//! (`finish_done`/`finish_cancelled`). The id (`Option<OperationId>`)
//! comes back to the caller inside `RenameBatchOutcome`/`RenameUndoOutcome`.
//!
//! **Collision-scope fix closed here**: the collision check runs against
//! the entire database — both preview and apply, not just within the
//! group being renamed.
//!
//! **Scope**: this module never resolves "all assets in a folder" or "all
//! assets in a batch" on its own — it always receives an already-explicit
//! `&[AssetId]`. This is the deliberate fix for "Rename folder...": no
//! silent narrowing happens inside this function, the caller decides and
//! declares the exact list before calling.
//!
//! **Declared deferral**: `resolve_place_label`'s middle candidate (the
//! folder's location) is never populated here — no location column exists
//! on folders in Keeppix's schema today (verified: no migration
//! introduces one). The fallback to the culling lot's name is not wired
//! up in this commit either — it would need the same logic as
//! `culling_lot_of` (private in `culling.rs`), not duplicated here
//! without a second real requirement in front of it. Both stay `None`:
//! the photo itself (`assets.place_id`/`asset_overrides.place_id`) is the
//! only source today. A narrow case (a photo with no location of its own,
//! in a freshly imported lot with nothing geotagged) would use an empty
//! `{place}` instead of the lot's name — not a silent defect, declared
//! here.

use std::collections::{BTreeMap, HashMap, HashSet};

use keeppix_domain::{
    Asset, AssetId, AssetName, AuthContext, BatchId, FolderId, OperationId, OperationKind,
    RenameValues, UserId, apply_base_to_filename, render_base,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{AssetRepo, Db, DbError, OperationsRepo, PermissionRepo};

/// State of an asset before a rename batch, keyed by `AssetId` as text
/// (same schema as `metadata_batches.previous`,
/// `overrides.rs::PreviousBatch` — a string-keyed `BTreeMap`, not a
/// `HashMap<Uuid, _>`, because `serde_json` requires object keys to be text).
type PreviousRenameState = BTreeMap<String, PreviousLocation>;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PreviousLocation {
    folder_id: Uuid,
    filename: String,
}

pub struct RenameRepo<'a> {
    db: &'a Db,
}

/// The result computed for **one physical file** (not per stack): two
/// side-by-side members of the same stack produce two entries with the
/// same base and each one's own extension.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenamePreviewItem {
    pub asset_id: AssetId,
    pub folder_id: FolderId,
    pub current_name: String,
    pub new_name: String,
    /// `true` if `new_name` matches, in the same folder, another asset —
    /// either from this batch **or already present on disk/database
    /// outside the batch**. An unchanged name (`new_name ==
    /// current_name`) never counts as a collision with itself.
    pub collides: bool,
}

#[derive(Debug)]
pub struct RenameBatchOutcome {
    /// `None` if no asset was successfully renamed — nothing to undo, no
    /// row written to `rename_batches`.
    pub batch_id: Option<BatchId>,
    /// The same `operation_id` passed to [`RenameRepo::apply`] — just
    /// sent back for the caller's convenience (`apply` no longer creates
    /// it itself, see the comment on [`RenameRepo::apply`]).
    pub operation_id: Option<OperationId>,
    pub renamed: Vec<Asset>,
    pub failed: Vec<(AssetId, DbError)>,
}

#[derive(Debug)]
pub struct RenameUndoOutcome {
    /// `true` if the batch had already been undone — not an error
    /// (`OverrideRepo::undo_batch` treats a second undo the same way: no
    /// work to do, not a failure).
    pub already_undone: bool,
    /// Same as [`RenameBatchOutcome::operation_id`] — created after
    /// verifying batch ownership, so even the "already undone" case
    /// creates one that closes right away, never a phantom operation.
    pub operation_id: Option<OperationId>,
    pub restored: Vec<Asset>,
    pub failed: Vec<(AssetId, DbError)>,
}

#[derive(Clone)]
struct AssetRow {
    id: Uuid,
    folder_id: Uuid,
    filename: String,
    stack_id: Option<Uuid>,
}

/// Raw row shared by `load_rows`/`load_stack_members` — same shape as
/// [`AssetRow`], kept separate only because it must derive `sqlx::FromRow`
/// at module level (clippy's `items_after_statements` forbids declaring
/// it inside the functions that use it).
#[derive(sqlx::FromRow)]
struct AssetLookupRow {
    id: Uuid,
    folder_id: Uuid,
    filename: String,
    stack_id: Option<Uuid>,
}

impl From<AssetLookupRow> for AssetRow {
    fn from(r: AssetLookupRow) -> Self {
        Self {
            id: r.id,
            folder_id: r.folder_id,
            filename: r.filename,
            stack_id: r.stack_id,
        }
    }
}

#[derive(sqlx::FromRow)]
struct ValuesRow {
    id: Uuid,
    taken_at: Option<chrono::DateTime<chrono::Utc>>,
    title: Option<String>,
    camera_model: Option<String>,
    lens: Option<String>,
    place_name: Option<String>,
}

impl<'a> RenameRepo<'a> {
    #[must_use]
    pub const fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// Computes the resulting names without writing anything — disk and
    /// database untouched. `asset_ids` is already the explicit scope:
    /// this function neither widens nor narrows it, it only automatically
    /// expands any stack (RAW+JPEG) whose members were not all already
    /// present, because they always get renamed together.
    ///
    /// # Errors
    /// `Forbidden` if even one asset is not visible or not editable by
    /// the caller.
    pub async fn preview(
        &self,
        ctx: &AuthContext,
        asset_ids: &[AssetId],
        schema: &str,
    ) -> Result<Vec<RenamePreviewItem>, DbError> {
        let items = self.compute(ctx, asset_ids, schema).await?;
        Ok(items)
    }

    /// Like [`Self::preview`], then actually renames via
    /// [`crate::AssetRepo::move_asset`]. Permission stays a **single gate
    /// for the whole call** (like `OverrideRepo::apply_batch`, not its
    /// `_partial` variant): if even one asset in scope is not editable,
    /// `compute` rejects everything before attempting the first
    /// `move_asset` — it makes no sense to rename half of a group the
    /// user selected because of a permission problem on another member.
    /// **Collisions**, on the other hand, are by nature only knowable at
    /// write time (a race between two concurrent calls, or two entries in
    /// the group destined for the same name): those end up in `failed`
    /// without blocking the rest — partial success on the data, not on
    /// permission. Records a `rename_batches` row **only** for
    /// successfully renamed assets — a partial failure does not leave
    /// undoable something that never changed.
    ///
    /// `operation_id` (`OperationKind::BulkRename`; the caller now creates
    /// it **before** invoking `apply`, no longer `apply` itself): unlike
    /// the original design (creating the operation in here, after
    /// `compute`, so as not to leave one orphaned if upstream checks
    /// fail), `apply` now runs inside a background job
    /// (`keeppix-jobs::rename_batch`) — the HTTP caller needs to know the
    /// id **before** enqueueing the job, to return it right away in the
    /// `202` response (the same need as `LibraryScan`). The "never a
    /// phantom operation" safety property still holds, moved upstream:
    /// the caller creates the operation only **after** already verifying
    /// permission/scope, exactly as `apply` used to do — see
    /// `routes/rename.rs::apply_batch`. From here on, `apply` still plays
    /// the worker role on the id it receives: total and phase set before
    /// the loop, `cancel_requested` polled between one asset and the next
    /// — cancelling midway is **not** a rollback: assets already renamed
    /// stay renamed, ones not yet attempted stay as they were, neither
    /// succeeded nor failed. This same function closes the final state
    /// (`Done`/`Cancelled`), not the caller. `None` for test callers that
    /// do not track progress.
    ///
    /// # Errors
    /// `Forbidden` if the caller is not authenticated. Per-asset errors
    /// end up in `failed`, not propagated.
    pub async fn apply(
        &self,
        ctx: &AuthContext,
        asset_ids: &[AssetId],
        schema: &str,
        operation_id: Option<OperationId>,
    ) -> Result<RenameBatchOutcome, DbError> {
        let Some(actor) = ctx.user_id() else {
            return Err(DbError::Forbidden);
        };
        let items = self.compute(ctx, asset_ids, schema).await?;

        let operations = OperationsRepo::new(self.db);
        if let Some(op_id) = operation_id {
            let total = items
                .iter()
                .filter(|item| item.current_name != item.new_name)
                .count();
            let total = i64::try_from(total)
                .map_err(|e| DbError::Corrupted(format!("rename batch total: {e}")))?;
            operations.set_total(op_id, Some(total)).await?;
            operations.set_phase(op_id, "renaming").await?;
        }

        let assets = AssetRepo::new(self.db);
        let mut renamed = Vec::new();
        let mut failed = Vec::new();
        let mut previous: PreviousRenameState = BTreeMap::new();
        let mut cancelled = false;

        for item in items {
            if item.current_name == item.new_name {
                continue;
            }
            if let Some(op_id) = operation_id
                && operations.is_cancel_requested(op_id).await?
            {
                cancelled = true;
                break;
            }
            let new_filename = match AssetName::parse(&item.new_name) {
                Ok(name) => name,
                Err(err) => {
                    failed.push((
                        item.asset_id,
                        DbError::Conflict(format!("computed filename is invalid: {err}")),
                    ));
                    continue;
                }
            };
            match assets
                .move_asset(ctx, item.asset_id, item.folder_id, new_filename)
                .await
            {
                Ok(asset) => {
                    previous.insert(
                        item.asset_id.to_string(),
                        PreviousLocation {
                            folder_id: item.folder_id.as_uuid(),
                            filename: item.current_name,
                        },
                    );
                    if let Some(op_id) = operation_id {
                        operations.record_success(op_id, item.asset_id).await?;
                    }
                    renamed.push(asset);
                }
                Err(err) => failed.push((item.asset_id, err)),
            }
        }

        if let Some(op_id) = operation_id {
            if cancelled {
                operations.finish_cancelled(op_id).await?;
            } else {
                operations.finish_done(op_id).await?;
            }
        }

        let batch_id = if renamed.is_empty() {
            None
        } else {
            let id = BatchId::new();
            sqlx::query("INSERT INTO rename_batches (id, actor_id, previous) VALUES ($1, $2, $3)")
                .bind(id.as_uuid())
                .bind(actor.as_uuid())
                .bind(
                    serde_json::to_value(&previous)
                        .map_err(|e| DbError::Corrupted(format!("rename batch state: {e}")))?,
                )
                .execute(self.db.pool())
                .await?;
            Some(id)
        };

        Ok(RenameBatchOutcome {
            batch_id,
            operation_id,
            renamed,
            failed,
        })
    }

    /// Undoes a rename batch: calls [`crate::AssetRepo::move_asset`]
    /// "backward" for every recorded asset — not a simple column restore
    /// like `OverrideRepo::undo_batch`, because `filename`/`folder_id`
    /// live on `assets` and require real physical moves, not rows to
    /// rewrite.
    ///
    /// **No "already synced" guard** equivalent to the XMP one in
    /// `OverrideRepo::undo_batch` (`xmp_written_at >= applied_at`): that
    /// one exists because an async job can consume the batch's value
    /// before the undo — for renaming there is no comparable async
    /// consumer, the physical move is the entire effect, and it happened
    /// synchronously at `apply` time. An attempt at an analogous guard on
    /// `assets.updated_at > applied_at` was discarded: that column is
    /// touched by operations with no relation to the name (scan state,
    /// `thumbhash`, `stack_id`, `location_source`...), so it would block
    /// the undo for reasons almost always unrelated to the file. If the
    /// asset has been renamed again since then, `move_asset` still moves
    /// it back to the name in `previous` — the same behavior as undoing
    /// any single step of a linear history, independent of what happened
    /// afterward.
    ///
    /// Partial success like [`Self::apply`]: a collision at the previous
    /// path (someone else already occupies that name) ends up in `failed`
    /// without blocking the other assets in the batch.
    ///
    /// `track_operation`: same worker role as [`Self::apply`] — the
    /// operation is created **here**, after the batch ownership check,
    /// never before: even the "already undone" case creates one normally,
    /// closed as `Done` right away (nothing to iterate), instead of
    /// leaving a phantom one if the call had failed earlier.
    ///
    /// # Errors
    /// `NotFound`/`Forbidden` if the batch does not exist or does not
    /// belong to the caller (non-admin). Per-asset errors end up in
    /// `failed`, not propagated.
    pub async fn undo(
        &self,
        ctx: &AuthContext,
        batch_id: BatchId,
        track_operation: bool,
    ) -> Result<RenameUndoOutcome, DbError> {
        #[derive(sqlx::FromRow)]
        struct BatchRow {
            actor_id: Uuid,
            undone_at: Option<chrono::DateTime<chrono::Utc>>,
            previous: serde_json::Value,
        }

        let mut tx = self.db.pool().begin().await?;
        let row: Option<BatchRow> = sqlx::query_as(
            "SELECT actor_id, undone_at, previous FROM rename_batches WHERE id = $1 FOR UPDATE",
        )
        .bind(batch_id.as_uuid())
        .fetch_optional(&mut *tx)
        .await?;

        let Some(row) = row else {
            return Err(if ctx.is_admin() {
                DbError::NotFound
            } else {
                DbError::Forbidden
            });
        };

        let owner = UserId::from_uuid(row.actor_id);
        if !ctx.is_admin() && Some(owner) != ctx.user_id() {
            return Err(DbError::Forbidden);
        }

        let operations = OperationsRepo::new(self.db);
        let operation_id = if track_operation {
            Some(operations.create(ctx, OperationKind::BulkRename).await?.id)
        } else {
            None
        };

        if row.undone_at.is_some() {
            tx.commit().await?;
            // Nothing to iterate, but a freshly created operation still
            // needs to be closed: otherwise it would stay `running` forever.
            if let Some(op_id) = operation_id {
                operations.finish_done(op_id).await?;
            }
            return Ok(RenameUndoOutcome {
                already_undone: true,
                operation_id,
                restored: Vec::new(),
                failed: Vec::new(),
            });
        }

        // Mark it undone right away, before any move_asset: a second
        // concurrent undo on the same batch finds undone_at already set
        // instead of racing with this one. The physical move that
        // follows is no longer protected by the row lock (move_asset
        // opens its own connection, which cannot be nested inside this
        // transaction) — acceptable: the remaining risk is the same as
        // apply(), a collision at write time, already handled per-asset
        // in `failed`.
        sqlx::query("UPDATE rename_batches SET undone_at = now() WHERE id = $1")
            .bind(batch_id.as_uuid())
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;

        let previous: PreviousRenameState = serde_json::from_value(row.previous)
            .map_err(|e| crate::row::corrupted("rename_batches.previous", e))?;

        if let Some(op_id) = operation_id {
            let total = i64::try_from(previous.len())
                .map_err(|e| DbError::Corrupted(format!("rename batch total: {e}")))?;
            operations.set_total(op_id, Some(total)).await?;
            operations.set_phase(op_id, "undoing").await?;
        }

        let (restored, failed, cancelled) = self
            .restore_previous_locations(ctx, previous, &operations, operation_id)
            .await?;

        if let Some(op_id) = operation_id {
            if cancelled {
                operations.finish_cancelled(op_id).await?;
            } else {
                operations.finish_done(op_id).await?;
            }
        }

        Ok(RenameUndoOutcome {
            already_undone: false,
            operation_id,
            restored,
            failed,
        })
    }

    /// The per-asset loop of [`Self::undo`]: calls `move_asset`
    /// "backward" for every entry in `previous`, with the same
    /// partial-success and cancellation-interruption pattern as
    /// [`Self::apply`]. Extracted only to stay under clippy's line limit
    /// — no logic of its own beyond what is already described on `undo`.
    async fn restore_previous_locations(
        &self,
        ctx: &AuthContext,
        previous: PreviousRenameState,
        operations: &OperationsRepo<'_>,
        operation_id: Option<OperationId>,
    ) -> Result<(Vec<Asset>, Vec<(AssetId, DbError)>, bool), DbError> {
        let assets = AssetRepo::new(self.db);
        let mut restored = Vec::new();
        let mut failed = Vec::new();
        let mut cancelled = false;
        for (raw_id, location) in previous {
            let asset_id = match raw_id.parse::<Uuid>() {
                Ok(id) => AssetId::from_uuid(id),
                Err(_) => continue, // written only by apply(): a malformed id cannot exist.
            };
            if let Some(op_id) = operation_id
                && operations.is_cancel_requested(op_id).await?
            {
                cancelled = true;
                break;
            }
            let filename = match AssetName::parse(&location.filename) {
                Ok(name) => name,
                Err(err) => {
                    failed.push((
                        asset_id,
                        DbError::Conflict(format!("stored filename is invalid: {err}")),
                    ));
                    continue;
                }
            };
            match assets
                .move_asset(
                    ctx,
                    asset_id,
                    FolderId::from_uuid(location.folder_id),
                    filename,
                )
                .await
            {
                Ok(asset) => {
                    if let Some(op_id) = operation_id {
                        operations.record_success(op_id, asset_id).await?;
                    }
                    restored.push(asset);
                }
                Err(err) => failed.push((asset_id, err)),
            }
        }
        Ok((restored, failed, cancelled))
    }

    /// The computation shared by `preview`/`apply`: permission gate,
    /// stack expansion, value resolution, the per-stack base, and the
    /// collision check (within the group **and** against the rest of the
    /// database).
    async fn compute(
        &self,
        ctx: &AuthContext,
        asset_ids: &[AssetId],
        schema: &str,
    ) -> Result<Vec<RenamePreviewItem>, DbError> {
        if asset_ids.is_empty() {
            return Ok(Vec::new());
        }
        AssetRepo::new(self.db)
            .assert_visible(ctx, asset_ids)
            .await?;
        PermissionRepo::new(self.db)
            .assert_can_edit_assets(ctx, asset_ids)
            .await?;

        let ids: Vec<Uuid> = asset_ids.iter().map(AssetId::as_uuid).collect();
        let requested = self.load_rows(&ids).await?;
        let requested_by_id: HashMap<Uuid, &AssetRow> =
            requested.iter().map(|r| (r.id, r)).collect();

        let stack_ids: Vec<Uuid> = requested
            .iter()
            .filter_map(|r| r.stack_id)
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        let primaries = self.load_stack_primaries(&stack_ids).await?;
        let members_by_stack = self.load_stack_members(&stack_ids).await?;

        // Logical groups in the order of `asset_ids` (the counter follows
        // the order of the scope's photo array): an asset in a stack
        // represents the whole group at its first appearance, later
        // appearances (its own or a sibling's) are ignored.
        let mut seen: HashSet<Uuid> = HashSet::new();
        let mut groups: Vec<(Uuid, Vec<AssetRow>)> = Vec::new();
        for &id in &ids {
            if seen.contains(&id) {
                continue;
            }
            let Some(row) = requested_by_id.get(&id) else {
                continue; // not visible/trashed: assert_visible above would already have rejected this for a non-admin user.
            };
            if let Some(stack_id) = row.stack_id {
                let members = members_by_stack.get(&stack_id).cloned().unwrap_or_default();
                for m in &members {
                    seen.insert(m.id);
                }
                let representative = primaries.get(&stack_id).copied().unwrap_or(id);
                groups.push((representative, members));
            } else {
                seen.insert(id);
                groups.push((id, vec![(*row).clone()]));
            }
        }

        let representative_ids: Vec<Uuid> = groups.iter().map(|(rep, _)| *rep).collect();
        let values_by_id = self.load_values(&representative_ids).await?;

        let mut items = Vec::new();
        for (index, (representative, members)) in groups.into_iter().enumerate() {
            let values = values_by_id
                .get(&representative)
                .cloned()
                .unwrap_or_default();
            let base = render_base(schema, &values, index + 1);
            for member in members {
                let new_name = apply_base_to_filename(&base, &member.filename);
                items.push(RenamePreviewItem {
                    asset_id: AssetId::from_uuid(member.id),
                    folder_id: FolderId::from_uuid(member.folder_id),
                    current_name: member.filename.clone(),
                    new_name,
                    collides: false,
                });
            }
        }

        self.flag_collisions(&ids, &mut items).await?;
        Ok(items)
    }

    async fn load_rows(&self, ids: &[Uuid]) -> Result<Vec<AssetRow>, DbError> {
        let rows: Vec<AssetLookupRow> = sqlx::query_as(
            "SELECT id, folder_id, filename, stack_id FROM assets \
              WHERE id = ANY($1) AND status <> 'trashed'",
        )
        .bind(ids)
        .fetch_all(self.db.pool())
        .await?;
        Ok(rows.into_iter().map(AssetRow::from).collect())
    }

    async fn load_stack_primaries(
        &self,
        stack_ids: &[Uuid],
    ) -> Result<HashMap<Uuid, Uuid>, DbError> {
        if stack_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let rows: Vec<(Uuid, Uuid)> =
            sqlx::query_as("SELECT id, primary_asset_id FROM stacks WHERE id = ANY($1)")
                .bind(stack_ids)
                .fetch_all(self.db.pool())
                .await?;
        Ok(rows.into_iter().collect())
    }

    async fn load_stack_members(
        &self,
        stack_ids: &[Uuid],
    ) -> Result<HashMap<Uuid, Vec<AssetRow>>, DbError> {
        if stack_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let rows: Vec<AssetLookupRow> = sqlx::query_as(
            "SELECT id, folder_id, filename, stack_id FROM assets \
              WHERE stack_id = ANY($1) AND status <> 'trashed'",
        )
        .bind(stack_ids)
        .fetch_all(self.db.pool())
        .await?;
        let mut by_stack: HashMap<Uuid, Vec<AssetRow>> = HashMap::new();
        for r in rows {
            let Some(stack_id) = r.stack_id else {
                continue;
            };
            by_stack.entry(stack_id).or_default().push(r.into());
        }
        Ok(by_stack)
    }

    /// The resolved values (date/camera/lens/place/title) for each
    /// representative asset of a group — never for non-representative
    /// members, which inherit their representative's base.
    async fn load_values(&self, ids: &[Uuid]) -> Result<HashMap<Uuid, RenameValues>, DbError> {
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        let rows: Vec<ValuesRow> = sqlx::query_as(
            "SELECT a.id, \
                    COALESCE(o.taken_at, a.taken_at_utc) AS taken_at, \
                    o.title, \
                    e.camera_model, e.lens, \
                    p.name AS place_name \
             FROM assets a \
             LEFT JOIN asset_overrides o ON o.asset_id = a.id \
             LEFT JOIN asset_exif e ON e.asset_id = a.id \
             LEFT JOIN places p ON p.id = COALESCE(o.place_id, a.place_id) \
             WHERE a.id = ANY($1)",
        )
        .bind(ids)
        .fetch_all(self.db.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| {
                let values = RenameValues {
                    date: r.taken_at.map(|ts| ts.date_naive()),
                    camera: r.camera_model,
                    lens: r.lens,
                    place: r.place_name,
                    title: r.title,
                };
                (r.id, values)
            })
            .collect())
    }

    /// Flags `collides` on every entry: within the group (two members of
    /// this same call point to the same name in the same folder) **and**
    /// against the rest of the database — assets not included in this
    /// call that already occupy that name (this needs the list of
    /// involved assets, which this module is the first to have).
    async fn flag_collisions(
        &self,
        batch_ids: &[Uuid],
        items: &mut [RenamePreviewItem],
    ) -> Result<(), DbError> {
        let mut within_group: HashMap<(Uuid, String), usize> = HashMap::new();
        for item in items.iter() {
            *within_group
                .entry((item.folder_id.as_uuid(), item.new_name.clone()))
                .or_insert(0) += 1;
        }

        let folder_ids: Vec<Uuid> = items
            .iter()
            .map(|i| i.folder_id.as_uuid())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        let existing: HashSet<(Uuid, String)> = if folder_ids.is_empty() {
            HashSet::new()
        } else {
            let rows: Vec<(Uuid, String)> = sqlx::query_as(
                "SELECT folder_id, filename FROM assets \
                  WHERE folder_id = ANY($1) AND status <> 'trashed' AND NOT (id = ANY($2))",
            )
            .bind(&folder_ids)
            .bind(batch_ids)
            .fetch_all(self.db.pool())
            .await?;
            rows.into_iter().collect()
        };

        for item in items.iter_mut() {
            let key = (item.folder_id.as_uuid(), item.new_name.clone());
            let no_op = item.new_name == item.current_name;
            let duplicated_in_group = within_group.get(&key).copied().unwrap_or(0) > 1;
            let occupied_outside_group = !no_op && existing.contains(&key);
            item.collides = duplicated_in_group || occupied_outside_group;
        }
        Ok(())
    }
}
