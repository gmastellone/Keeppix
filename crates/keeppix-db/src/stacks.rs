use std::collections::HashMap;

use chrono::{DateTime, Utc};
use keeppix_domain::{Asset, AssetId, AuthContext, FolderId, StackId};
use uuid::Uuid;

use crate::assets::{A_COLUMNS, AssetRow};
use crate::{AssetRepo, Db, DbError};

/// Matches the value of `assets.kind` as written by the migration: used
/// to prefer the RAW as primary without pulling in the domain
/// `AssetKind` just for a string comparison.
const RAW_IMAGE: &str = "raw_image";

/// Additive stack badge for the browse view: `stack_size == 1` for an
/// unstacked asset. `raw_kind` distinguishes the three compositions the
/// interface shows as a badge: `"raw"`, `"jpeg"`, `"raw+jpeg"` — `None`
/// only for kinds that are neither (video, unknown).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StackBadge {
    pub stack_size: u16,
    pub raw_kind: Option<String>,
}

/// An [`Asset`] with its [`StackBadge`]: used where the browse view shows
/// only the primary of each stack (timeline, search). The `Deref`
/// towards `Asset` leaves untouched the code that only reads asset
/// fields (id, `taken_at_utc`, cursors, ...).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetWithStack {
    pub asset: Asset,
    pub stack: StackBadge,
}

impl std::ops::Deref for AssetWithStack {
    type Target = Asset;

    fn deref(&self) -> &Asset {
        &self.asset
    }
}

/// `LEFT JOIN` that brings in the stack's primary (for filtering) for
/// every `assets a` row. Standalone (without [`STACK_BADGE_JOIN_SQL`])
/// for queries that only need to exclude non-primaries without showing
/// the badge (geometry): one fewer join to plan.
pub(crate) const STACK_PRIMARY_JOIN_SQL: &str = "LEFT JOIN stacks s ON s.id = a.stack_id";

/// A row is excluded if it is a non-primary member of a stack. Requires
/// [`STACK_PRIMARY_JOIN_SQL`] (or [`STACK_BADGE_JOIN_SQL`], which
/// includes it) in the query.
pub(crate) const STACK_PRIMARY_ONLY_SQL: &str = "(a.stack_id IS NULL OR a.id = s.primary_asset_id)";

/// Like [`STACK_PRIMARY_JOIN_SQL`], plus a lateral aggregate over the
/// stack's members to compute `stack_size`/`raw_kind` — run only for rows
/// that pass `WHERE`/`LIMIT` (not for every candidate), so the cost stays
/// tied to the number of tiles returned, not to the scan.
pub(crate) const STACK_BADGE_JOIN_SQL: &str = "LEFT JOIN stacks s ON s.id = a.stack_id \
     LEFT JOIN LATERAL ( \
         SELECT count(*)::int2 AS stack_size, \
                CASE WHEN bool_or(m.kind = 'raw_image') AND bool_or(m.kind = 'image') \
                     THEN 'raw+jpeg' \
                     WHEN bool_or(m.kind = 'raw_image') THEN 'raw' \
                     WHEN bool_or(m.kind = 'image') THEN 'jpeg' \
                     ELSE NULL END AS raw_kind \
           FROM assets m WHERE m.stack_id = a.stack_id \
     ) si ON a.stack_id IS NOT NULL";

/// Extra columns to add alongside [`A_COLUMNS`] to get
/// `stack_size`/`raw_kind` in the same row. An unstacked asset derives
/// the badge from its own `kind` (no aggregate to read); a stacked one
/// reads the aggregate computed by [`STACK_BADGE_JOIN_SQL`].
pub(crate) const STACK_BADGE_COLUMNS_SQL: &str = "CASE WHEN a.stack_id IS NULL THEN 1::int2 ELSE si.stack_size END AS stack_size, \
     CASE WHEN a.stack_id IS NULL THEN \
         CASE a.kind WHEN 'raw_image' THEN 'raw' WHEN 'image' THEN 'jpeg' ELSE NULL END \
     ELSE si.raw_kind END AS raw_kind";

/// Raw row for a `{A_COLUMNS}, {STACK_BADGE_COLUMNS_SQL}` query: same
/// pattern as `AlbumAssetRow` in `albums.rs` — a second type with all of
/// `AssetRow`'s fields plus the extra columns, converted by going through
/// `AssetRow::from_raw` because `AssetRow`'s fields are private to the
/// `assets` module.
#[derive(sqlx::FromRow)]
pub(crate) struct AssetStackRow {
    id: Uuid,
    folder_id: Uuid,
    filename: String,
    content_hash: Option<Vec<u8>>,
    size_bytes: i64,
    mtime: DateTime<Utc>,
    inode: Option<i64>,
    kind: String,
    status: String,
    taken_at_utc: Option<DateTime<Utc>>,
    width: Option<i32>,
    height: Option<i32>,
    thumbhash: Option<Vec<u8>>,
    created_at: DateTime<Utc>,
    stack_size: i16,
    raw_kind: Option<String>,
}

impl AssetStackRow {
    pub(crate) fn into_domain(self) -> Result<AssetWithStack, DbError> {
        let stack_size = u16::try_from(self.stack_size).unwrap_or(1).max(1);
        let raw_kind = self.raw_kind;
        let asset = AssetRow::from_raw(
            self.id,
            self.folder_id,
            self.filename,
            self.content_hash,
            self.size_bytes,
            self.mtime,
            self.inode,
            self.kind,
            self.status,
            self.taken_at_utc,
            self.width,
            self.height,
            self.thumbhash,
            self.created_at,
        )
        .into_domain()?;
        Ok(AssetWithStack {
            asset,
            stack: StackBadge {
                stack_size,
                raw_kind,
            },
        })
    }
}

pub struct StackRepo<'a> {
    db: &'a Db,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StackMember {
    pub asset: Asset,
    pub is_primary: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StackDetails {
    pub stack_id: StackId,
    pub primary_asset_id: AssetId,
    pub members: Vec<StackMember>,
}

#[derive(sqlx::FromRow)]
struct MemberRow {
    id: Uuid,
    filename: String,
    kind: String,
    stack_id: Option<Uuid>,
}

impl<'a> StackRepo<'a> {
    #[must_use]
    pub const fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// Groups a folder's non-trashed assets by base name: `DSC_0042.ARW`
    /// and `DSC_0042.JPG` end up in the same stack, with the RAW as
    /// primary when present. A lone file, however unique its base name
    /// is in the folder, never forms a stack.
    ///
    /// Idempotent: re-running it on the same files reuses the existing
    /// stack instead of creating a new one — the critical property for
    /// rescans (without it, every scan would produce a new stack). The
    /// deletion or removal from the stack of a member — including the
    /// primary — is handled by the `assets_promote_stack_primary` trigger,
    /// not by this method: a `DELETE` done elsewhere (trash, scanner)
    /// must keep the invariant without needing to call back into
    /// `StackRepo`.
    ///
    /// Does not take an `AuthContext`: the scanner calls this on an
    /// entire folder after writing its assets, like
    /// `LibraryRepo::mark_scanned`.
    ///
    /// # Errors
    /// `Connection` if a query fails.
    pub async fn regroup_folder(&self, folder_id: FolderId) -> Result<(), DbError> {
        let members: Vec<MemberRow> = sqlx::query_as(
            "SELECT id, filename, kind, stack_id FROM assets \
              WHERE folder_id = $1 AND status <> 'trashed'",
        )
        .bind(folder_id.as_uuid())
        .fetch_all(self.db.pool())
        .await?;

        let mut groups: HashMap<String, Vec<MemberRow>> = HashMap::new();
        for member in members {
            groups
                .entry(basename_key(&member.filename))
                .or_default()
                .push(member);
        }

        let mut tx = self.db.pool().begin().await?;
        for mut group in groups.into_values() {
            // Deterministic order: decides the fallback primary when
            // there is no RAW, and makes grouping reproducible.
            group.sort_by(|a, b| a.filename.cmp(&b.filename));

            if group.len() < 2 {
                unstack_lone_member(&mut tx, &group).await?;
                continue;
            }

            let primary = group
                .iter()
                .find(|m| m.kind == RAW_IMAGE)
                .unwrap_or(&group[0])
                .id;

            let mut existing_ids: Vec<Uuid> = group.iter().filter_map(|m| m.stack_id).collect();
            existing_ids.sort_unstable();
            existing_ids.dedup();

            // A single pre-existing id among the members: this is the
            // same group from a previous grouping pass, it must be
            // reused — not recreated. Zero or more than one: a new stack
            // (more than one is an anomalous case this method does not
            // attempt to reconcile).
            let stack_id = if let [only] = existing_ids.as_slice() {
                *only
            } else {
                let id = StackId::new().as_uuid();
                sqlx::query("INSERT INTO stacks (id, primary_asset_id) VALUES ($1, $2)")
                    .bind(id)
                    .bind(primary)
                    .execute(&mut *tx)
                    .await?;
                id
            };

            sqlx::query(
                "UPDATE stacks SET primary_asset_id = $2 \
                  WHERE id = $1 AND primary_asset_id IS DISTINCT FROM $2",
            )
            .bind(stack_id)
            .bind(primary)
            .execute(&mut *tx)
            .await?;

            let member_ids: Vec<Uuid> = group.iter().map(|m| m.id).collect();
            sqlx::query(
                "UPDATE assets SET stack_id = $2, updated_at = now() \
                  WHERE id = ANY($1) AND stack_id IS DISTINCT FROM $2",
            )
            .bind(&member_ids)
            .bind(stack_id)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Members of the stack `asset_id` belongs to. `None` if the asset is
    /// not in a stack.
    ///
    /// # Errors
    /// `Forbidden` if the asset is not visible to the caller (including
    /// when it does not exist). `Connection` if a query fails.
    pub async fn members(
        &self,
        ctx: &AuthContext,
        asset_id: AssetId,
    ) -> Result<Option<StackDetails>, DbError> {
        AssetRepo::new(self.db)
            .assert_visible(ctx, std::slice::from_ref(&asset_id))
            .await?;

        let stack_id: Option<Uuid> =
            sqlx::query_scalar("SELECT stack_id FROM assets WHERE id = $1")
                .bind(asset_id.as_uuid())
                .fetch_optional(self.db.pool())
                .await?;

        let Some(stack_id) = stack_id else {
            return Ok(None);
        };

        let primary: Uuid = sqlx::query_scalar("SELECT primary_asset_id FROM stacks WHERE id = $1")
            .bind(stack_id)
            .fetch_one(self.db.pool())
            .await?;

        let sql = format!(
            "SELECT {A_COLUMNS} FROM assets a \
              WHERE a.stack_id = $1 AND a.status <> 'trashed' \
              ORDER BY a.filename"
        );
        let rows: Vec<AssetRow> = sqlx::query_as(&sql)
            .bind(stack_id)
            .fetch_all(self.db.pool())
            .await?;

        let mut members = Vec::with_capacity(rows.len());
        for row in rows {
            let row_id = row.id();
            let is_primary = row_id == primary;
            members.push(StackMember {
                asset: row.into_domain()?,
                is_primary,
            });
        }

        Ok(Some(StackDetails {
            stack_id: StackId::from_uuid(stack_id),
            primary_asset_id: AssetId::from_uuid(primary),
            members,
        }))
    }

    /// Sets `asset_id` as the primary of its stack.
    ///
    /// # Errors
    /// `Forbidden` same as [`Self::members`]. `Conflict` if the asset is
    /// not in a stack. `Connection` if a query fails.
    pub async fn set_primary(&self, ctx: &AuthContext, asset_id: AssetId) -> Result<(), DbError> {
        AssetRepo::new(self.db)
            .assert_visible(ctx, std::slice::from_ref(&asset_id))
            .await?;

        let stack_id: Option<Uuid> =
            sqlx::query_scalar("SELECT stack_id FROM assets WHERE id = $1 AND status <> 'trashed'")
                .bind(asset_id.as_uuid())
                .fetch_optional(self.db.pool())
                .await?;

        let Some(stack_id) = stack_id else {
            return Err(DbError::Conflict("asset is not in a stack".to_owned()));
        };

        sqlx::query("UPDATE stacks SET primary_asset_id = $2 WHERE id = $1")
            .bind(stack_id)
            .bind(asset_id.as_uuid())
            .execute(self.db.pool())
            .await?;
        Ok(())
    }
}

/// A base name that is now unique in the folder (it was in a stack, but
/// no longer justifies one — the last other member is gone) gets
/// unlinked. The `assets_promote_stack_primary` trigger does the rest: if
/// this member was the primary and none remain, it deletes the `stacks`
/// row instead of leaving it orphaned.
async fn unstack_lone_member(
    tx: &mut sqlx::PgConnection,
    group: &[MemberRow],
) -> Result<(), DbError> {
    for member in group {
        if member.stack_id.is_some() {
            sqlx::query("UPDATE assets SET stack_id = NULL, updated_at = now() WHERE id = $1")
                .bind(member.id)
                .execute(&mut *tx)
                .await?;
        }
    }
    Ok(())
}

/// Base name for grouping: the filename without its last extension,
/// case-insensitive. `DSC_0042.ARW` and `dsc_0042.jpg` are the same shot
/// even if the casing differs between the camera and the software that
/// wrote the JPEG.
fn basename_key(filename: &str) -> String {
    filename
        .rsplit_once('.')
        .map_or(filename, |(base, _ext)| base)
        .to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::basename_key;

    #[test]
    fn strips_the_last_extension_case_insensitively() {
        assert_eq!(basename_key("DSC_0042.ARW"), "dsc_0042");
        assert_eq!(basename_key("DSC_0042.JPG"), "dsc_0042");
    }

    #[test]
    fn a_filename_without_an_extension_is_its_own_basename() {
        assert_eq!(basename_key("README"), "readme");
    }
}
