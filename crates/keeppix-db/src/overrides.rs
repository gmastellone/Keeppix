use std::collections::{BTreeMap, HashMap};

use chrono::{DateTime, Utc};
use keeppix_domain::{
    AssetId, AuthContext, BatchId, EffectiveMetadata, GeoPoint, JobKind, JobPriority,
    LocationSource, OverridePatch, Pick, Rating, UserId,
};
use serde::{Deserialize, Serialize};
use sqlx::PgConnection;

use crate::visibility::VisibilityScope;
use crate::{AssetRepo, Db, DbError, JobRepo};

pub struct OverrideRepo<'a> {
    db: &'a Db,
}

#[derive(sqlx::FromRow)]
struct OverrideRow {
    asset_id: uuid::Uuid,
    had_override: bool,
    title: Option<String>,
    description: Option<String>,
    taken_at: Option<DateTime<Utc>>,
    lon: Option<f64>,
    lat: Option<f64>,
    place_id: Option<i64>,
    orientation: Option<i16>,
    updated_by: Option<uuid::Uuid>,
    location_source: Option<String>,
}

#[derive(sqlx::FromRow)]
struct EffectiveRow {
    title: Option<String>,
    description: Option<String>,
    taken_at: Option<DateTime<Utc>>,
    lon: Option<f64>,
    lat: Option<f64>,
    place_id: Option<i64>,
    orientation: Option<i16>,
}

impl EffectiveRow {
    fn into_domain(self) -> EffectiveMetadata {
        EffectiveMetadata {
            title: self.title,
            description: self.description,
            taken_at: self.taken_at,
            location: point(self.lon, self.lat),
            place_id: self.place_id,
            orientation: self.orientation,
        }
    }
}

fn point(lon: Option<f64>, lat: Option<f64>) -> Option<GeoPoint> {
    match (lon, lat) {
        (Some(lon), Some(lat)) => Some(GeoPoint { lat, lon }),
        _ => None,
    }
}

fn wkt(point: Option<GeoPoint>) -> Option<String> {
    point.map(|p| format!("SRID=4326;POINT({} {})", p.lon, p.lat))
}

/// State of `asset_overrides` and `assets.location_source` before a batch.
/// `had_override` distinguishes a row with all fields `NULL` from the
/// absence of the row: in `undo_batch` these become an UPSERT and a
/// DELETE, respectively.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredOverride {
    /// `false` means that the asset had no `asset_overrides` row. Older batch
    /// payloads predate this field and only serialized `Some` for existing
    /// rows, so their correct default is `true`.
    #[serde(default = "default_true")]
    had_override: bool,
    title: Option<String>,
    description: Option<String>,
    taken_at: Option<DateTime<Utc>>,
    lon: Option<f64>,
    lat: Option<f64>,
    place_id: Option<i64>,
    orientation: Option<i16>,
    updated_by: Option<uuid::Uuid>,
    /// Compatibility flag for batches written before this field existed.
    /// `None` is a valid previous source, so presence cannot be
    /// represented by the source field alone.
    #[serde(default)]
    location_source_captured: bool,
    #[serde(default)]
    location_source: Option<String>,
}

/// Key: `asset_id` as a string (jsonb fields cannot have non-text keys).
/// Older payloads use `None` for an asset with no override; newer ones use
/// `StoredOverride::had_override` so they can also save the previous
/// `location_source`.
type PreviousBatch = BTreeMap<String, Option<StoredOverride>>;

const fn default_true() -> bool {
    true
}

#[allow(clippy::option_option)]
fn touched<T>(field: Option<Option<T>>) -> (bool, Option<T>) {
    match field {
        Some(inner) => (true, inner),
        None => (false, None),
    }
}

impl<'a> OverrideRepo<'a> {
    #[must_use]
    pub const fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// `COALESCE(override, exif)` field by field. A partial override does
    /// not clear the fields it does not touch.
    ///
    /// # Errors
    /// `Forbidden` if the caller cannot see the asset — even when the id
    /// does not exist. `NotFound` only for an admin requesting a
    /// nonexistent id.
    pub async fn effective(
        &self,
        ctx: &AuthContext,
        asset_id: AssetId,
    ) -> Result<EffectiveMetadata, DbError> {
        let scope = VisibilityScope::resolve(self.db, ctx).await?;
        let filter = scope.filter("f.path", "f.library_id", "a.id", 2);
        let row: Option<EffectiveRow> = sqlx::query_as(&format!(
            "SELECT o.title, o.description, \
                    COALESCE(o.taken_at, a.taken_at_utc) AS taken_at, \
                    ST_X(COALESCE(o.location, a.location)::geometry) AS lon, \
                    ST_Y(COALESCE(o.location, a.location)::geometry) AS lat, \
                    COALESCE(o.place_id, a.place_id) AS place_id, \
                    o.orientation \
             FROM assets a \
             JOIN folders f ON f.id = a.folder_id \
             LEFT JOIN asset_overrides o ON o.asset_id = a.id \
             WHERE a.id = $1 AND {}",
            filter.sql()
        ))
        .bind(asset_id.as_uuid())
        .bind(filter.bind())
        .bind(filter.holes())
        .bind(filter.assets())
        .fetch_optional(self.db.pool())
        .await?;

        match row {
            Some(row) => Ok(row.into_domain()),
            None if ctx.is_admin() => Err(DbError::NotFound),
            None => Err(DbError::Forbidden),
        }
    }

    /// Applies a change to a single asset, without recording it for undo
    /// — only [`Self::apply_batch`] does that.
    ///
    /// # Errors
    /// `Forbidden` if the caller cannot see the asset, or can see it only
    /// as a viewer (editing metadata requires editor+).
    pub async fn apply(
        &self,
        ctx: &AuthContext,
        asset_id: AssetId,
        patch: &OverridePatch,
    ) -> Result<(), DbError> {
        AssetRepo::new(self.db)
            .assert_visible(ctx, std::slice::from_ref(&asset_id))
            .await?;
        crate::PermissionRepo::new(self.db)
            .assert_can_edit_assets(ctx, std::slice::from_ref(&asset_id))
            .await?;
        let mut conn = self.db.pool().acquire().await?;
        apply_patch(
            &mut conn,
            &[asset_id.as_uuid()],
            patch,
            ctx.user_id().map(|id| id.as_uuid()),
        )
        .await?;
        enqueue_sidecar_sweep(self.db).await
    }

    /// Applies the **same** change to many assets in a single transaction
    /// — not 500 round trips — and records the previous values for
    /// [`Self::undo_batch`].
    ///
    /// # Errors
    /// `Forbidden` if even one asset is not visible to the caller, or is
    /// visible only as a viewer.
    pub async fn apply_batch(
        &self,
        ctx: &AuthContext,
        asset_ids: &[AssetId],
        patch: &OverridePatch,
    ) -> Result<BatchId, DbError> {
        self.apply_batch_inner(ctx, asset_ids, patch, None).await
    }

    /// Like [`Self::apply_batch`], with partial success: non-writable
    /// assets end up in `failed` and do not enter the undo batch.
    ///
    /// # Errors
    /// `Forbidden` without an authenticated user; `Connection` on DB error.
    pub async fn apply_batch_partial(
        &self,
        ctx: &AuthContext,
        asset_ids: &[AssetId],
        patch: &OverridePatch,
    ) -> Result<(Option<BatchId>, Vec<AssetId>, Vec<(AssetId, DbError)>), DbError> {
        self.apply_batch_partial_inner(ctx, asset_ids, patch, None)
            .await
    }

    /// Applies a uniform location and records on `assets.location_source`
    /// who assigned it. The source is part of the same transaction and
    /// the same undo snapshot as the overrides.
    ///
    /// # Errors
    /// Same as [`Self::apply_batch`].
    pub async fn apply_location_batch(
        &self,
        ctx: &AuthContext,
        asset_ids: &[AssetId],
        patch: &OverridePatch,
        source: LocationSource,
    ) -> Result<BatchId, DbError> {
        self.apply_batch_inner(ctx, asset_ids, patch, Some(source))
            .await
    }

    /// Partial-success variant of [`Self::apply_location_batch`].
    ///
    /// # Errors
    /// Same as [`Self::apply_batch_partial`].
    pub async fn apply_location_batch_partial(
        &self,
        ctx: &AuthContext,
        asset_ids: &[AssetId],
        patch: &OverridePatch,
        source: LocationSource,
    ) -> Result<(Option<BatchId>, Vec<AssetId>, Vec<(AssetId, DbError)>), DbError> {
        self.apply_batch_partial_inner(ctx, asset_ids, patch, Some(source))
            .await
    }

    async fn apply_batch_partial_inner(
        &self,
        ctx: &AuthContext,
        asset_ids: &[AssetId],
        patch: &OverridePatch,
        source: Option<LocationSource>,
    ) -> Result<(Option<BatchId>, Vec<AssetId>, Vec<(AssetId, DbError)>), DbError> {
        let (editable, failed) = crate::PermissionRepo::new(self.db)
            .partition_editable_assets(ctx, asset_ids)
            .await?;
        if editable.is_empty() {
            return Ok((None, editable, failed));
        }
        let batch_id = self
            .apply_batch_inner(ctx, &editable, patch, source)
            .await?;
        Ok((Some(batch_id), editable, failed))
    }

    async fn apply_batch_inner(
        &self,
        ctx: &AuthContext,
        asset_ids: &[AssetId],
        patch: &OverridePatch,
        source: Option<LocationSource>,
    ) -> Result<BatchId, DbError> {
        AssetRepo::new(self.db)
            .assert_visible(ctx, asset_ids)
            .await?;
        crate::PermissionRepo::new(self.db)
            .assert_can_edit_assets(ctx, asset_ids)
            .await?;
        let Some(actor) = ctx.user_id() else {
            return Err(DbError::Forbidden);
        };

        let ids: Vec<uuid::Uuid> = asset_ids.iter().map(AssetId::as_uuid).collect();
        let mut tx = self.db.pool().begin().await?;

        let captures_location_source =
            source.is_some() || patch.location.is_some() || patch.place_id.is_some();
        let previous = load_previous(&mut tx, &ids, captures_location_source).await?;
        let batch_id = BatchId::new();
        sqlx::query("INSERT INTO metadata_batches (id, actor_id, previous) VALUES ($1, $2, $3)")
            .bind(batch_id.as_uuid())
            .bind(actor.as_uuid())
            .bind(
                serde_json::to_value(&previous)
                    .map_err(|e| DbError::Corrupted(format!("previous batch state: {e}")))?,
            )
            .execute(&mut *tx)
            .await?;

        apply_patch(&mut tx, &ids, patch, Some(actor.as_uuid())).await?;
        if let Some(source) = source {
            apply_location_source(&mut tx, &ids, source).await?;
        }

        tx.commit().await?;
        enqueue_sidecar_sweep(self.db).await?;
        Ok(batch_id)
    }

    /// Reads, in a single round trip, the effective timestamp of assets to
    /// match against a GPX track. Assets with no date are omitted from
    /// the result.
    ///
    /// # Errors
    /// `Forbidden` if even one asset is not visible or editable.
    pub async fn effective_taken_at(
        &self,
        ctx: &AuthContext,
        asset_ids: &[AssetId],
    ) -> Result<Vec<(AssetId, DateTime<Utc>)>, DbError> {
        AssetRepo::new(self.db)
            .assert_visible(ctx, asset_ids)
            .await?;
        crate::PermissionRepo::new(self.db)
            .assert_can_edit_assets(ctx, asset_ids)
            .await?;
        if asset_ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids: Vec<uuid::Uuid> = asset_ids.iter().map(AssetId::as_uuid).collect();
        let rows: Vec<(uuid::Uuid, DateTime<Utc>)> = sqlx::query_as(
            "SELECT a.id, COALESCE(o.taken_at, a.taken_at_utc) \
               FROM assets a \
               LEFT JOIN asset_overrides o ON o.asset_id = a.id \
              WHERE a.id = ANY($1) \
                AND COALESCE(o.taken_at, a.taken_at_utc) IS NOT NULL",
        )
        .bind(&ids)
        .fetch_all(self.db.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|(id, taken_at)| (AssetId::from_uuid(id), taken_at))
            .collect())
    }

    /// Applies different coordinates per asset with a single `UNNEST`,
    /// recording a single undoable batch. This is the parametric writer
    /// used by GPX matching; it does not run one `apply()` per photo.
    ///
    /// # Errors
    /// `Forbidden` if even one asset is not visible or editable.
    pub async fn apply_geotag_points(
        &self,
        ctx: &AuthContext,
        assignments: &[(AssetId, GeoPoint)],
        source: LocationSource,
    ) -> Result<BatchId, DbError> {
        let asset_ids: Vec<AssetId> = assignments.iter().map(|(id, _)| *id).collect();
        AssetRepo::new(self.db)
            .assert_visible(ctx, &asset_ids)
            .await?;
        crate::PermissionRepo::new(self.db)
            .assert_can_edit_assets(ctx, &asset_ids)
            .await?;
        let Some(actor) = ctx.user_id() else {
            return Err(DbError::Forbidden);
        };

        let ids: Vec<uuid::Uuid> = assignments.iter().map(|(id, _)| id.as_uuid()).collect();
        let mut tx = self.db.pool().begin().await?;
        let previous = load_previous(&mut tx, &ids, true).await?;
        let batch_id = BatchId::new();
        sqlx::query("INSERT INTO metadata_batches (id, actor_id, previous) VALUES ($1, $2, $3)")
            .bind(batch_id.as_uuid())
            .bind(actor.as_uuid())
            .bind(
                serde_json::to_value(&previous)
                    .map_err(|e| DbError::Corrupted(format!("previous batch state: {e}")))?,
            )
            .execute(&mut *tx)
            .await?;

        apply_points(&mut tx, assignments, actor.as_uuid()).await?;
        apply_location_source(&mut tx, &ids, source).await?;
        tx.commit().await?;
        enqueue_sidecar_sweep(self.db).await?;
        Ok(batch_id)
    }

    /// Applies different timestamps per asset in a single undoable batch.
    ///
    /// This is the parametric writer for timezone recalculation: it
    /// preserves every other override field and reuses
    /// `metadata_batches`/`undo_batch`. An empty list is a no-op and does
    /// not create an empty batch row.
    ///
    /// # Errors
    /// `Forbidden` if even one asset is not visible or editable.
    pub async fn apply_taken_at_batch(
        &self,
        ctx: &AuthContext,
        assignments: &[(AssetId, DateTime<Utc>)],
    ) -> Result<Option<BatchId>, DbError> {
        if assignments.is_empty() {
            return Ok(None);
        }
        let asset_ids: Vec<AssetId> = assignments.iter().map(|(id, _)| *id).collect();
        AssetRepo::new(self.db)
            .assert_visible(ctx, &asset_ids)
            .await?;
        crate::PermissionRepo::new(self.db)
            .assert_can_edit_assets(ctx, &asset_ids)
            .await?;
        let Some(actor) = ctx.user_id() else {
            return Err(DbError::Forbidden);
        };

        let mut tx = self.db.pool().begin().await?;
        let (changed_count, batch_id) =
            apply_taken_at_assignments_in_transaction(&mut tx, assignments, actor.as_uuid())
                .await?;
        tx.commit().await?;
        if changed_count != 0 {
            enqueue_sidecar_sweep(self.db).await?;
        }
        Ok(batch_id)
    }

    /// Restores exactly the previous values of a batch — even when the
    /// previous value was `NULL`, and even when the asset had no override
    /// at all yet (in that case the row is deleted, not left with all
    /// fields `NULL`).
    ///
    /// Idempotent: undoing an already-undone batch does nothing.
    ///
    /// **Undo window** ("undo restores the previous values until the
    /// sidecar has been written"): if the sidecar of **even one** asset
    /// in the batch has already been written with this batch's values
    /// (`xmp_written_at >= metadata_batches.applied_at`), the undo is
    /// rejected with `Conflict` instead of rolling the database back
    /// while leaving the file — already delivered, perhaps exported
    /// elsewhere — with a value the database no longer remembers as
    /// "current". Before that moment, undo is always allowed: the file
    /// has not yet seen the wrong value.
    ///
    /// # Errors
    /// `Forbidden` if the batch does not belong to the caller — even when
    /// the id does not exist. `NotFound` only for an admin requesting a
    /// nonexistent id. `Conflict` if the sidecar has already been written
    /// for this batch.
    pub async fn undo_batch(&self, ctx: &AuthContext, batch_id: BatchId) -> Result<(), DbError> {
        let mut tx = self.db.pool().begin().await?;

        let row: Option<BatchRow> = sqlx::query_as(
            "SELECT actor_id, applied_at, undone_at, previous FROM metadata_batches \
              WHERE id = $1 FOR UPDATE",
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

        if row.undone_at.is_some() {
            // Already undone: nothing to do, not an error.
            tx.commit().await?;
            return Ok(());
        }

        let previous: PreviousBatch = serde_json::from_value(row.previous)
            .map_err(|e| crate::row::corrupted("metadata_batches.previous", e))?;
        let asset_ids = previous_asset_ids(&previous)?;

        let already_synced: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM asset_overrides \
               WHERE asset_id = ANY($1) AND xmp_written_at IS NOT NULL \
                 AND xmp_written_at >= $2)",
        )
        .bind(&asset_ids)
        .bind(row.applied_at)
        .fetch_one(&mut *tx)
        .await?;
        if already_synced {
            return Err(DbError::Conflict(
                "the sidecar has already been written with this batch's values; \
                 undo is only available before that"
                    .to_owned(),
            ));
        }

        restore_previous(&mut tx, &previous).await?;

        sqlx::query("UPDATE metadata_batches SET undone_at = now() WHERE id = $1")
            .bind(batch_id.as_uuid())
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;
        Ok(())
    }

    /// **Shift of N hours** on the shot date: the fix for a camera clock
    /// that drifted after a trip, offered as its own operation — not
    /// computed by the client by subtracting two absolute dates — because
    /// each asset in the batch can have a different starting `taken_at`.
    /// A single statement computes `COALESCE(override, exif) + N hours`
    /// per row, so it works the same for a single asset or 5,000.
    ///
    /// Records an undo batch exactly like [`Self::apply_batch`]: the same
    /// [`Self::undo_batch`] restores it.
    ///
    /// # Errors
    /// `Forbidden` if even one asset is not visible to the caller, or is
    /// visible only as a viewer.
    pub async fn shift_taken_at(
        &self,
        ctx: &AuthContext,
        asset_ids: &[AssetId],
        hours: i32,
    ) -> Result<BatchId, DbError> {
        AssetRepo::new(self.db)
            .assert_visible(ctx, asset_ids)
            .await?;
        crate::PermissionRepo::new(self.db)
            .assert_can_edit_assets(ctx, asset_ids)
            .await?;
        let Some(actor) = ctx.user_id() else {
            return Err(DbError::Forbidden);
        };

        let ids: Vec<uuid::Uuid> = asset_ids.iter().map(AssetId::as_uuid).collect();
        let mut tx = self.db.pool().begin().await?;

        let previous = load_previous(&mut tx, &ids, false).await?;
        let batch_id = BatchId::new();
        sqlx::query("INSERT INTO metadata_batches (id, actor_id, previous) VALUES ($1, $2, $3)")
            .bind(batch_id.as_uuid())
            .bind(actor.as_uuid())
            .bind(
                serde_json::to_value(&previous)
                    .map_err(|e| DbError::Corrupted(format!("previous batch state: {e}")))?,
            )
            .execute(&mut *tx)
            .await?;

        apply_shift(&mut tx, &ids, hours, Some(actor.as_uuid())).await?;

        tx.commit().await?;
        enqueue_sidecar_sweep(self.db).await?;
        Ok(batch_id)
    }

    /// Partial-success variant of [`Self::shift_taken_at`]: only the
    /// editable assets enter the undo batch.
    ///
    /// # Errors
    /// `Forbidden` without a user; `Connection` on DB error.
    pub async fn shift_taken_at_partial(
        &self,
        ctx: &AuthContext,
        asset_ids: &[AssetId],
        hours: i32,
    ) -> Result<(Option<BatchId>, Vec<AssetId>, Vec<(AssetId, DbError)>), DbError> {
        let (editable, failed) = crate::PermissionRepo::new(self.db)
            .partition_editable_assets(ctx, asset_ids)
            .await?;
        if editable.is_empty() {
            return Ok((None, editable, failed));
        }
        let batch_id = self.shift_taken_at(ctx, &editable, hours).await?;
        Ok((Some(batch_id), editable, failed))
    }

    /// Assets with overrides not yet written to file:
    /// `updated_at > COALESCE(xmp_written_at, '-infinity')`.
    ///
    /// Does not take an `AuthContext`: the `WriteSidecar` job calls this,
    /// sweeping all libraries in the background — like
    /// `LibraryRepo::mark_scanned`.
    ///
    /// # Errors
    /// `Connection` if the query fails.
    pub async fn pending_sidecars(&self, limit: i64) -> Result<Vec<AssetId>, DbError> {
        let rows: Vec<uuid::Uuid> = sqlx::query_scalar(
            "SELECT asset_id FROM asset_overrides \
              WHERE updated_at > COALESCE(xmp_written_at, '-infinity') \
              ORDER BY updated_at \
              LIMIT $1",
        )
        .bind(limit)
        .fetch_all(self.db.pool())
        .await?;
        Ok(rows.into_iter().map(AssetId::from_uuid).collect())
    }

    /// What the `WriteSidecar` job needs to sync an asset: the effective
    /// values (`COALESCE(override, exif)`) plus the **library owner's**
    /// vote — `xmp:Rating`/`xmp:Label` are single-valued, so only the
    /// owner's vote ends up on the file; everyone else's stays in Keeppix
    /// only.
    ///
    /// Does not take an `AuthContext`: the background job calls this
    /// across all libraries, same justification as
    /// [`Self::pending_sidecars`].
    ///
    /// # Errors
    /// `NotFound` if the asset no longer exists — deleted between the
    /// job's enqueue and its execution, not a bug.
    pub async fn sidecar_source(&self, asset_id: AssetId) -> Result<SidecarSource, DbError> {
        let row: Option<SidecarRow> = sqlx::query_as(
            "SELECT o.title, o.description, \
                    COALESCE(o.taken_at, a.taken_at_utc) AS taken_at, \
                    ST_X(COALESCE(o.location, a.location)::geometry) AS lon, \
                    ST_Y(COALESCE(o.location, a.location)::geometry) AS lat, \
                    COALESCE(o.place_id, a.place_id) AS place_id, \
                    o.orientation, \
                    fl.rating AS owner_rating, fl.pick AS owner_pick \
             FROM assets a \
             JOIN folders f ON f.id = a.folder_id \
             JOIN libraries l ON l.id = f.library_id \
             LEFT JOIN asset_overrides o ON o.asset_id = a.id \
             LEFT JOIN asset_flags fl ON fl.asset_id = a.id AND fl.user_id = l.owner_id \
             WHERE a.id = $1",
        )
        .bind(asset_id.as_uuid())
        .fetch_optional(self.db.pool())
        .await?;

        row.ok_or(DbError::NotFound)?.into_domain()
    }

    /// Records that the sidecar has been written **and verified** — never
    /// before: if the process dies between the write and this call, the
    /// next round of [`Self::pending_sidecars`] retries it, instead of
    /// assuming a file is synced when it might not be.
    ///
    /// Does not take an `AuthContext`, same justification as
    /// [`Self::sidecar_source`].
    ///
    /// # Errors
    /// `Connection` if the update fails.
    pub async fn mark_sidecar_written(&self, asset_id: AssetId) -> Result<(), DbError> {
        sqlx::query("UPDATE asset_overrides SET xmp_written_at = now() WHERE asset_id = $1")
            .bind(asset_id.as_uuid())
            .execute(self.db.pool())
            .await?;
        Ok(())
    }
}

/// Data that the `WriteSidecar` job (keeppix-jobs) translates into a
/// `keeppix_media::xmp::SidecarData`. Lives here, not in keeppix-media,
/// because it carries domain types (`Rating`, `Pick`) that the media
/// crate should not know about any more than it should know about the
/// database.
#[derive(Debug, Clone, PartialEq)]
pub struct SidecarSource {
    pub effective: EffectiveMetadata,
    pub owner_rating: Option<Rating>,
    pub owner_pick: Pick,
}

#[derive(sqlx::FromRow)]
struct SidecarRow {
    title: Option<String>,
    description: Option<String>,
    taken_at: Option<DateTime<Utc>>,
    lon: Option<f64>,
    lat: Option<f64>,
    place_id: Option<i64>,
    orientation: Option<i16>,
    owner_rating: Option<i16>,
    owner_pick: Option<String>,
}

impl SidecarRow {
    fn into_domain(self) -> Result<SidecarSource, DbError> {
        let owner_rating = self
            .owner_rating
            .map(|raw| {
                u8::try_from(raw)
                    .map_err(|e| crate::row::corrupted("asset_flags.rating", e))
                    .and_then(|raw| {
                        Rating::parse(raw)
                            .map_err(|e| crate::row::corrupted("asset_flags.rating", e))
                    })
            })
            .transpose()?;
        let owner_pick = self
            .owner_pick
            .as_deref()
            .map(|raw| Pick::parse(raw).map_err(|e| crate::row::corrupted("asset_flags.pick", e)))
            .transpose()?
            .unwrap_or_default();

        Ok(SidecarSource {
            effective: EffectiveMetadata {
                title: self.title,
                description: self.description,
                taken_at: self.taken_at,
                location: point(self.lon, self.lat),
                place_id: self.place_id,
                orientation: self.orientation,
            },
            owner_rating,
            owner_pick,
        })
    }
}

/// Wakes up the `WriteSidecar` job after an override has made some asset
/// "pending" ("DB first, file second"). A single dedup key
/// (`write_sidecar`) means 500 assets in one `apply_batch` produce a
/// single job, not 500: the job itself re-reads `pending_sidecars` and
/// processes everything it finds, re-queueing itself if there is more
/// than one batch's worth.
///
/// # Errors
/// `Connection` if the enqueue fails.
pub(crate) async fn enqueue_sidecar_sweep(db: &Db) -> Result<(), DbError> {
    JobRepo::new(db)
        .enqueue(
            JobKind::WriteSidecar,
            serde_json::json!({}),
            JobPriority::Background,
            Some("write_sidecar"),
        )
        .await?;
    Ok(())
}

#[derive(sqlx::FromRow)]
struct BatchRow {
    actor_id: uuid::Uuid,
    applied_at: DateTime<Utc>,
    undone_at: Option<DateTime<Utc>>,
    previous: serde_json::Value,
}

/// The keys of [`PreviousBatch`] are `asset_id` as a string (a JSONB
/// constraint); here we convert back to `Uuid` to query `asset_overrides`.
fn previous_asset_ids(previous: &PreviousBatch) -> Result<Vec<uuid::Uuid>, DbError> {
    previous
        .keys()
        .map(|key| {
            uuid::Uuid::parse_str(key)
                .map_err(|e| crate::row::corrupted("metadata_batches.previous key", e))
        })
        .collect()
}

/// Upserts `patch` onto `asset_ids`, preserving the untouched fields of
/// each existing row. A single statement: `CASE WHEN <touched> THEN <new
/// value> ELSE <existing column> END` per field, so the same round trip
/// serves both a single asset (`apply`) and 500 (`apply_batch`).
async fn apply_patch(
    conn: &mut PgConnection,
    asset_ids: &[uuid::Uuid],
    patch: &OverridePatch,
    updated_by: Option<uuid::Uuid>,
) -> Result<(), DbError> {
    if asset_ids.is_empty() {
        return Ok(());
    }
    let (title_touched, title) = touched(patch.title.clone());
    let (description_touched, description) = touched(patch.description.clone());
    let (taken_at_touched, taken_at) = touched(patch.taken_at);
    let (location_touched, location) = touched(patch.location);
    let location_wkt = wkt(location);
    let (place_id_touched, place_id) = touched(patch.place_id);
    let (orientation_touched, orientation) = touched(patch.orientation);

    sqlx::query(
        "INSERT INTO asset_overrides \
            (asset_id, title, description, taken_at, location, place_id, orientation, \
             updated_by, updated_at) \
         SELECT aid, \
                CASE WHEN $2 THEN $3 ELSE NULL END, \
                CASE WHEN $4 THEN $5 ELSE NULL END, \
                CASE WHEN $6 THEN $7 ELSE NULL END, \
                CASE WHEN $8 THEN $9::geography ELSE NULL END, \
                CASE WHEN $10 THEN $11 ELSE NULL END, \
                CASE WHEN $12 THEN $13 ELSE NULL END, \
                $14, now() \
           FROM unnest($1::uuid[]) AS aid \
         ON CONFLICT (asset_id) DO UPDATE SET \
                title = CASE WHEN $2 THEN EXCLUDED.title ELSE asset_overrides.title END, \
                description = CASE WHEN $4 THEN EXCLUDED.description \
                                    ELSE asset_overrides.description END, \
                taken_at = CASE WHEN $6 THEN EXCLUDED.taken_at ELSE asset_overrides.taken_at END, \
                location = CASE WHEN $8 THEN EXCLUDED.location ELSE asset_overrides.location END, \
                place_id = CASE WHEN $10 THEN EXCLUDED.place_id ELSE asset_overrides.place_id END, \
                orientation = CASE WHEN $12 THEN EXCLUDED.orientation \
                                   ELSE asset_overrides.orientation END, \
                updated_by = $14, \
                updated_at = now()",
    )
    .bind(asset_ids)
    .bind(title_touched)
    .bind(title)
    .bind(description_touched)
    .bind(description)
    .bind(taken_at_touched)
    .bind(taken_at)
    .bind(location_touched)
    .bind(location_wkt)
    .bind(place_id_touched)
    .bind(place_id)
    .bind(orientation_touched)
    .bind(orientation)
    .bind(updated_by)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

async fn apply_points(
    conn: &mut PgConnection,
    assignments: &[(AssetId, GeoPoint)],
    updated_by: uuid::Uuid,
) -> Result<(), DbError> {
    if assignments.is_empty() {
        return Ok(());
    }
    let ids: Vec<uuid::Uuid> = assignments.iter().map(|(id, _)| id.as_uuid()).collect();
    let lons: Vec<f64> = assignments.iter().map(|(_, point)| point.lon).collect();
    let lats: Vec<f64> = assignments.iter().map(|(_, point)| point.lat).collect();
    sqlx::query(
        "INSERT INTO asset_overrides \
            (asset_id, location, place_id, updated_by, updated_at) \
         SELECT aid, ST_SetSRID(ST_MakePoint(lon, lat), 4326)::geography, NULL, $4, now() \
           FROM unnest($1::uuid[], $2::float8[], $3::float8[]) AS t(aid, lon, lat) \
         ON CONFLICT (asset_id) DO UPDATE SET \
            location = EXCLUDED.location, \
            place_id = NULL, \
            updated_by = EXCLUDED.updated_by, \
            updated_at = now()",
    )
    .bind(&ids)
    .bind(&lons)
    .bind(&lats)
    .bind(updated_by)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

pub(crate) async fn apply_taken_at_assignments_in_transaction(
    conn: &mut PgConnection,
    assignments: &[(AssetId, DateTime<Utc>)],
    updated_by: uuid::Uuid,
) -> Result<(usize, Option<BatchId>), DbError> {
    if assignments.is_empty() {
        return Ok((0, None));
    }
    let ids: Vec<uuid::Uuid> = assignments.iter().map(|(id, _)| id.as_uuid()).collect();
    let mut previous = load_previous(conn, &ids, false).await?;
    let applied_ids = apply_taken_at_values(conn, assignments, updated_by).await?;
    if applied_ids.is_empty() {
        return Ok((0, None));
    }
    let applied_keys: std::collections::HashSet<String> =
        applied_ids.iter().map(uuid::Uuid::to_string).collect();
    previous.retain(|key, _| applied_keys.contains(key));

    let batch_id = BatchId::new();
    sqlx::query("INSERT INTO metadata_batches (id, actor_id, previous) VALUES ($1, $2, $3)")
        .bind(batch_id.as_uuid())
        .bind(updated_by)
        .bind(
            serde_json::to_value(&previous)
                .map_err(|error| DbError::Corrupted(format!("previous batch state: {error}")))?,
        )
        .execute(&mut *conn)
        .await?;
    Ok((applied_ids.len(), Some(batch_id)))
}

async fn apply_taken_at_values(
    conn: &mut PgConnection,
    assignments: &[(AssetId, DateTime<Utc>)],
    updated_by: uuid::Uuid,
) -> Result<Vec<uuid::Uuid>, DbError> {
    let ids: Vec<uuid::Uuid> = assignments.iter().map(|(id, _)| id.as_uuid()).collect();
    let values: Vec<DateTime<Utc>> = assignments.iter().map(|(_, value)| *value).collect();
    let applied_ids: Vec<uuid::Uuid> = sqlx::query_scalar(
        "INSERT INTO asset_overrides (asset_id, taken_at, updated_by, updated_at) \
         SELECT asset_id, taken_at, $3, now() \
           FROM unnest($1::uuid[], $2::timestamptz[]) AS input(asset_id, taken_at) \
         ON CONFLICT (asset_id) DO UPDATE SET \
            taken_at = EXCLUDED.taken_at, \
            updated_by = EXCLUDED.updated_by, \
            updated_at = now() \
          WHERE asset_overrides.taken_at IS NULL \
         RETURNING asset_id",
    )
    .bind(&ids)
    .bind(&values)
    .bind(updated_by)
    .fetch_all(&mut *conn)
    .await?;
    Ok(applied_ids)
}

async fn apply_location_source(
    conn: &mut PgConnection,
    asset_ids: &[uuid::Uuid],
    source: LocationSource,
) -> Result<(), DbError> {
    if asset_ids.is_empty() {
        return Ok(());
    }
    sqlx::query("UPDATE assets SET location_source = $2, updated_at = now() WHERE id = ANY($1)")
        .bind(asset_ids)
        .bind(source.as_str())
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// Applies a shift of `hours` hours to the effective `taken_at`
/// (`COALESCE(override, exif)`) of each asset, in a single statement. An
/// asset with no known shot date at all (neither override nor exif) stays
/// undated: a shift cannot invent an origin.
async fn apply_shift(
    conn: &mut PgConnection,
    asset_ids: &[uuid::Uuid],
    hours: i32,
    updated_by: Option<uuid::Uuid>,
) -> Result<(), DbError> {
    if asset_ids.is_empty() {
        return Ok(());
    }
    sqlx::query(
        "INSERT INTO asset_overrides \
            (asset_id, title, description, taken_at, location, place_id, orientation, \
             updated_by, updated_at) \
         SELECT a.id, o.title, o.description, \
                COALESCE(o.taken_at, a.taken_at_utc) + make_interval(hours => $2), \
                o.location, o.place_id, o.orientation, \
                $3, now() \
           FROM assets a \
           LEFT JOIN asset_overrides o ON o.asset_id = a.id \
          WHERE a.id = ANY($1) \
         ON CONFLICT (asset_id) DO UPDATE SET \
                taken_at = EXCLUDED.taken_at, \
                updated_by = EXCLUDED.updated_by, \
                updated_at = now()",
    )
    .bind(asset_ids)
    .bind(hours)
    .bind(updated_by)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Reads the current state of `asset_ids` before overwriting it, to
/// populate `metadata_batches.previous`.
async fn load_previous(
    conn: &mut PgConnection,
    asset_ids: &[uuid::Uuid],
    capture_location_source: bool,
) -> Result<PreviousBatch, DbError> {
    let rows: Vec<OverrideRow> = sqlx::query_as(
        "SELECT a.id AS asset_id, o.asset_id IS NOT NULL AS had_override, \
                o.title, o.description, o.taken_at, \
                ST_X(o.location::geometry) AS lon, ST_Y(o.location::geometry) AS lat, \
                o.place_id, o.orientation, o.updated_by, \
                CASE WHEN $2 THEN a.location_source ELSE NULL END AS location_source \
           FROM assets a \
           LEFT JOIN asset_overrides o ON o.asset_id = a.id \
          WHERE a.id = ANY($1)",
    )
    .bind(asset_ids)
    .bind(capture_location_source)
    .fetch_all(&mut *conn)
    .await?;

    let mut existing: HashMap<uuid::Uuid, StoredOverride> = HashMap::new();
    for row in rows {
        existing.insert(
            row.asset_id,
            StoredOverride {
                had_override: row.had_override,
                title: row.title,
                description: row.description,
                taken_at: row.taken_at,
                lon: row.lon,
                lat: row.lat,
                place_id: row.place_id,
                orientation: row.orientation,
                updated_by: row.updated_by,
                location_source_captured: capture_location_source,
                location_source: row.location_source,
            },
        );
    }

    Ok(asset_ids
        .iter()
        .map(|id| (id.to_string(), existing.get(id).cloned()))
        .collect())
}

/// The reverse of [`apply_patch`]: for assets with no previous override,
/// deletes the row; for the others, rewrites exactly the captured values
/// — including `NULL` ones, which a downstream `COALESCE` would confuse
/// with "do not touch" if the rewrite were skipped.
async fn restore_previous(
    conn: &mut PgConnection,
    previous: &PreviousBatch,
) -> Result<(), DbError> {
    let mut delete_ids = Vec::new();
    let mut restore_ids = Vec::new();
    let mut titles = Vec::new();
    let mut descriptions = Vec::new();
    let mut taken_ats = Vec::new();
    let mut locations = Vec::new();
    let mut place_ids = Vec::new();
    let mut orientations = Vec::new();
    let mut updated_bys = Vec::new();
    let mut source_ids = Vec::new();
    let mut location_sources = Vec::new();

    for (key, value) in previous {
        let id = uuid::Uuid::parse_str(key)
            .map_err(|e| crate::row::corrupted("metadata_batches.previous key", e))?;
        match value {
            None => delete_ids.push(id),
            Some(stored) => {
                if stored.had_override {
                    restore_ids.push(id);
                    titles.push(stored.title.clone());
                    descriptions.push(stored.description.clone());
                    taken_ats.push(stored.taken_at);
                    locations.push(wkt(point(stored.lon, stored.lat)));
                    place_ids.push(stored.place_id);
                    orientations.push(stored.orientation);
                    updated_bys.push(stored.updated_by);
                } else {
                    delete_ids.push(id);
                }
                if stored.location_source_captured {
                    source_ids.push(id);
                    location_sources.push(stored.location_source.clone());
                }
            }
        }
    }

    if !delete_ids.is_empty() {
        sqlx::query("DELETE FROM asset_overrides WHERE asset_id = ANY($1)")
            .bind(&delete_ids)
            .execute(&mut *conn)
            .await?;
    }

    if !restore_ids.is_empty() {
        sqlx::query(
            "INSERT INTO asset_overrides \
                (asset_id, title, description, taken_at, location, place_id, orientation, \
                 updated_by, updated_at) \
             SELECT aid, title, description, taken_at, loc::geography, place_id, orientation, \
                    updated_by, now() \
               FROM unnest($1::uuid[], $2::text[], $3::text[], $4::timestamptz[], $5::text[], \
                           $6::bigint[], $7::smallint[], $8::uuid[]) \
                 AS t(aid, title, description, taken_at, loc, place_id, orientation, updated_by) \
             ON CONFLICT (asset_id) DO UPDATE SET \
                title = EXCLUDED.title, \
                description = EXCLUDED.description, \
                taken_at = EXCLUDED.taken_at, \
                location = EXCLUDED.location, \
                place_id = EXCLUDED.place_id, \
                orientation = EXCLUDED.orientation, \
                updated_by = EXCLUDED.updated_by, \
                updated_at = now()",
        )
        .bind(&restore_ids)
        .bind(&titles)
        .bind(&descriptions)
        .bind(&taken_ats)
        .bind(&locations)
        .bind(&place_ids)
        .bind(&orientations)
        .bind(&updated_bys)
        .execute(&mut *conn)
        .await?;
    }

    if !source_ids.is_empty() {
        sqlx::query(
            "UPDATE assets AS a \
                SET location_source = previous.location_source, updated_at = now() \
               FROM unnest($1::uuid[], $2::text[]) AS previous(asset_id, location_source) \
              WHERE a.id = previous.asset_id",
        )
        .bind(&source_ids)
        .bind(&location_sources)
        .execute(&mut *conn)
        .await?;
    }

    Ok(())
}
