//! Virtual albums. No dedicated storage: a photo can belong to N albums.
//! Manual ordering via `position`. Authorization: owner or admin; shared
//! users via a direct permission on the album.

use std::collections::HashSet;

use chrono::{DateTime, Utc};
use keeppix_domain::{AlbumId, Asset, AssetId, AuthContext, UserId};
use sqlx::Row;
use sqlx::types::Json as SqlxJson;
use uuid::Uuid;

use crate::assets::AssetRow;
use crate::search::{SearchBind, SearchNode, compile_for_sql};
use crate::visibility::VisibilityScope;
use crate::{AssetRepo, Db, DbError};

pub struct AlbumRepo<'a> {
    db: &'a Db,
}

#[derive(Debug, Clone)]
pub struct Album {
    pub id: AlbumId,
    pub name: String,
    pub description: String,
    pub owner_id: UserId,
    pub cover_asset_id: Option<AssetId>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// The `SearchNode` the album was created with, if any. An album
    /// without a `rule` is a purely manual album: it cannot be refreshed
    /// (we ended up with a "refresh album" action instead of live dynamic
    /// albums).
    pub rule: Option<SearchNode>,
    pub rule_run_at: Option<DateTime<Utc>>,
    pub is_shared: bool,
    pub cover_tint: Option<String>,
    pub monochrome: bool,
}

#[derive(sqlx::FromRow)]
struct AlbumRow {
    id: Uuid,
    name: String,
    description: String,
    owner_id: Uuid,
    cover_asset_id: Option<Uuid>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    rule: Option<SqlxJson<SearchNode>>,
    rule_run_at: Option<DateTime<Utc>>,
    is_shared: bool,
    cover_tint: Option<String>,
    monochrome: bool,
}

impl AlbumRow {
    fn into_domain(self) -> Album {
        Album {
            id: AlbumId::from_uuid(self.id),
            name: self.name,
            description: self.description,
            owner_id: UserId::from_uuid(self.owner_id),
            cover_asset_id: self.cover_asset_id.map(AssetId::from_uuid),
            created_at: self.created_at,
            updated_at: self.updated_at,
            rule: self.rule.map(|SqlxJson(rule)| rule),
            rule_run_at: self.rule_run_at,
            is_shared: self.is_shared,
            cover_tint: self.cover_tint,
            monochrome: self.monochrome,
        }
    }
}

pub struct NewAlbum {
    pub name: String,
    pub description: String,
    /// The filter the album is created with, if created from a search.
    /// `None` for a purely manual album.
    pub rule: Option<SearchNode>,
}

/// Outcome of [`AlbumRepo::refresh`]: the asset ids that entered and left
/// `album_assets` in this run. The caller (HTTP layer) translates this into
/// the partial-success wrapper (`BulkOutcome`).
#[derive(Debug, Clone, Default)]
pub struct AlbumRefresh {
    pub added: Vec<AssetId>,
    pub removed: Vec<AssetId>,
}

pub struct AlbumPatch {
    pub name: Option<String>,
    pub description: Option<String>,
    #[allow(clippy::option_option)]
    pub cover_asset_id: Option<Option<AssetId>>,
}

#[derive(Debug, Clone)]
pub struct AlbumAsset {
    pub asset: Asset,
    pub position: i64,
    pub added_by: UserId,
    pub added_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct AlbumAssetRow {
    asset_id: Uuid,
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
    position: i64,
    added_by: Uuid,
    added_at: DateTime<Utc>,
}

const ALBUM_COLUMNS: &str = "id, name, description, owner_id, cover_asset_id, created_at, \
     updated_at, rule, rule_run_at, is_shared, cover_tint, monochrome";

/// An album that an asset is a member of, as returned by
/// [`AlbumRepo::for_asset`] — just id and name, enough for a non-clickable
/// chip (no visual distinction between manual and dynamic albums).
#[derive(Debug, Clone, PartialEq)]
pub struct AlbumBadge {
    pub id: AlbumId,
    pub name: String,
}

#[derive(sqlx::FromRow)]
struct AlbumBadgeRow {
    id: Uuid,
    name: String,
}

impl AlbumBadgeRow {
    fn into_domain(self) -> AlbumBadge {
        AlbumBadge {
            id: AlbumId::from_uuid(self.id),
            name: self.name,
        }
    }
}

impl<'a> AlbumRepo<'a> {
    #[must_use]
    pub const fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// Checks that the caller can access the album (owner, admin, or a
    /// direct permission). Returns `Forbidden` — not `NotFound` — so as not
    /// to offer an existence oracle.
    async fn assert_visible(&self, ctx: &AuthContext, album_id: AlbumId) -> Result<(), DbError> {
        if ctx.is_admin() {
            let exists: bool =
                sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM albums WHERE id = $1)")
                    .bind(album_id.as_uuid())
                    .fetch_one(self.db.pool())
                    .await?;
            return if exists {
                Ok(())
            } else {
                Err(DbError::Forbidden)
            };
        }
        if let keeppix_domain::Actor::ShareLink {
            object_type,
            object_id,
            ..
        } = &ctx.actor
        {
            return if object_type == "album" && *object_id == album_id.as_uuid() {
                Ok(())
            } else {
                Err(DbError::Forbidden)
            };
        }
        let Some(user_id) = ctx.user_id() else {
            return Err(DbError::Forbidden);
        };
        let ok: bool = sqlx::query_scalar(
            "SELECT EXISTS( \
               SELECT 1 FROM albums \
                WHERE id = $1 \
                  AND ( \
                       owner_id = $2 \
                    OR EXISTS ( \
                         SELECT 1 FROM permissions \
                          WHERE object_type = 'album' \
                            AND object_id = $1 \
                            AND ( \
                                 (subject_type = 'user'  AND subject_id = $2) \
                              OR (subject_type = 'group' AND subject_id IN ( \
                                    SELECT group_id FROM group_members WHERE user_id = $2 \
                                 )) \
                            ) \
                       ) \
                  ) \
             )",
        )
        .bind(album_id.as_uuid())
        .bind(user_id.as_uuid())
        .fetch_one(self.db.pool())
        .await?;

        if ok { Ok(()) } else { Err(DbError::Forbidden) }
    }

    /// Checks that the caller is owner or admin (can modify/delete).
    async fn assert_owner(&self, ctx: &AuthContext, album_id: AlbumId) -> Result<(), DbError> {
        if ctx.is_admin() {
            let exists: bool =
                sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM albums WHERE id = $1)")
                    .bind(album_id.as_uuid())
                    .fetch_one(self.db.pool())
                    .await?;
            return if exists {
                Ok(())
            } else {
                Err(DbError::Forbidden)
            };
        }
        let Some(user_id) = ctx.user_id() else {
            return Err(DbError::Forbidden);
        };
        let ok: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM albums WHERE id = $1 AND owner_id = $2)",
        )
        .bind(album_id.as_uuid())
        .bind(user_id.as_uuid())
        .fetch_one(self.db.pool())
        .await?;

        if ok { Ok(()) } else { Err(DbError::Forbidden) }
    }

    /// Creates a new empty album. The caller becomes owner.
    ///
    /// # Errors
    /// `Forbidden` without an authenticated user; `Connection` on DB error.
    pub async fn create(&self, ctx: &AuthContext, album: NewAlbum) -> Result<Album, DbError> {
        let owner = ctx.user_id().ok_or(DbError::Forbidden)?;
        let id = Uuid::now_v7();
        let row: AlbumRow = sqlx::query_as(&format!(
            "INSERT INTO albums (id, name, description, owner_id, rule) \
             VALUES ($1, $2, $3, $4, $5) \
             RETURNING {ALBUM_COLUMNS}"
        ))
        .bind(id)
        .bind(&album.name)
        .bind(&album.description)
        .bind(owner.as_uuid())
        .bind(album.rule.as_ref().map(SqlxJson))
        .fetch_one(self.db.pool())
        .await?;
        Ok(row.into_domain())
    }

    /// Lists the albums visible to the caller (own + shared).
    ///
    /// # Errors
    /// `Forbidden` without a user; `Connection` on DB error.
    pub async fn list(&self, ctx: &AuthContext) -> Result<Vec<Album>, DbError> {
        if ctx.is_admin() {
            let rows: Vec<AlbumRow> = sqlx::query_as(&format!(
                "SELECT {ALBUM_COLUMNS} FROM albums ORDER BY created_at DESC"
            ))
            .fetch_all(self.db.pool())
            .await?;
            return Ok(rows.into_iter().map(AlbumRow::into_domain).collect());
        }

        let Some(user_id) = ctx.user_id() else {
            return Err(DbError::Forbidden);
        };

        let rows: Vec<AlbumRow> = sqlx::query_as(&format!(
            "SELECT {ALBUM_COLUMNS} FROM albums \
              WHERE owner_id = $1 \
                 OR EXISTS ( \
                      SELECT 1 FROM permissions \
                       WHERE object_type = 'album' AND object_id = albums.id \
                         AND ( \
                              (subject_type = 'user'  AND subject_id = $1) \
                           OR (subject_type = 'group' AND subject_id IN ( \
                                 SELECT group_id FROM group_members WHERE user_id = $1 \
                              )) \
                         ) \
                    ) \
             ORDER BY created_at DESC"
        ))
        .bind(user_id.as_uuid())
        .fetch_all(self.db.pool())
        .await?;
        Ok(rows.into_iter().map(AlbumRow::into_domain).collect())
    }

    /// Fetches an album by id.
    ///
    /// # Errors
    /// `Forbidden` if it does not exist or is not visible (never `NotFound`).
    pub async fn get(&self, ctx: &AuthContext, album_id: AlbumId) -> Result<Album, DbError> {
        self.assert_visible(ctx, album_id).await?;
        let row: AlbumRow =
            sqlx::query_as(&format!("SELECT {ALBUM_COLUMNS} FROM albums WHERE id = $1"))
                .bind(album_id.as_uuid())
                .fetch_one(self.db.pool())
                .await?;
        Ok(row.into_domain())
    }

    /// Updates name, description, and/or cover. Owner or admin only.
    ///
    /// # Errors
    /// `Forbidden` if not owner/admin; `Connection` on DB error.
    pub async fn update(
        &self,
        ctx: &AuthContext,
        album_id: AlbumId,
        patch: AlbumPatch,
    ) -> Result<Album, DbError> {
        self.assert_owner(ctx, album_id).await?;

        let row: AlbumRow = sqlx::query_as(&format!(
            "UPDATE albums SET \
               name           = COALESCE($2, name), \
               description    = COALESCE($3, description), \
               cover_asset_id = CASE WHEN $4 THEN $5 ELSE cover_asset_id END, \
               updated_at     = now() \
             WHERE id = $1 \
             RETURNING {ALBUM_COLUMNS}"
        ))
        .bind(album_id.as_uuid())
        .bind(patch.name.as_deref())
        .bind(patch.description.as_deref())
        .bind(patch.cover_asset_id.is_some())
        .bind(patch.cover_asset_id.flatten().map(|id| id.as_uuid()))
        .fetch_one(self.db.pool())
        .await?;
        Ok(row.into_domain())
    }

    /// Deletes the album. Assets are not touched. Owner or admin only.
    ///
    /// # Errors
    /// `Forbidden` if not owner/admin; `Connection` on DB error.
    pub async fn delete(&self, ctx: &AuthContext, album_id: AlbumId) -> Result<(), DbError> {
        self.assert_owner(ctx, album_id).await?;
        sqlx::query("DELETE FROM albums WHERE id = $1")
            .bind(album_id.as_uuid())
            .execute(self.db.pool())
            .await?;
        Ok(())
    }

    /// Adds an asset to the album. The position is assigned at the end
    /// (MAX + 1000, or 1000 if the album is empty). Owner or admin only.
    ///
    /// # Errors
    /// `Forbidden` if not owner/admin or the asset is not visible;
    /// `Connection` on DB error.
    pub async fn add_asset(
        &self,
        ctx: &AuthContext,
        album_id: AlbumId,
        asset_id: AssetId,
    ) -> Result<(), DbError> {
        self.assert_owner(ctx, album_id).await?;
        // Check that the asset exists and is visible to the caller.
        // find_by_id returns Forbidden if not visible — never NotFound for regular users.
        AssetRepo::new(self.db).find_by_id(ctx, asset_id).await?;

        let added_by = ctx.user_id().ok_or(DbError::Forbidden)?;
        sqlx::query(
            "INSERT INTO album_assets (album_id, asset_id, position, added_by) \
             VALUES ($1, $2, \
               COALESCE((SELECT MAX(position) FROM album_assets WHERE album_id = $1), 0) + 1000, \
               $3) \
             ON CONFLICT (album_id, asset_id) DO NOTHING",
        )
        .bind(album_id.as_uuid())
        .bind(asset_id.as_uuid())
        .bind(added_by.as_uuid())
        .execute(self.db.pool())
        .await?;
        Ok(())
    }

    /// Removes an asset from the album. The asset itself is **not**
    /// deleted. Owner or admin only.
    ///
    /// # Errors
    /// `Forbidden` if not owner/admin; `Connection` on DB error.
    pub async fn remove_asset(
        &self,
        ctx: &AuthContext,
        album_id: AlbumId,
        asset_id: AssetId,
    ) -> Result<(), DbError> {
        self.assert_owner(ctx, album_id).await?;
        sqlx::query("DELETE FROM album_assets WHERE album_id = $1 AND asset_id = $2")
            .bind(album_id.as_uuid())
            .bind(asset_id.as_uuid())
            .execute(self.db.pool())
            .await?;
        Ok(())
    }

    /// Reorders an asset within the album by assigning it a new position.
    /// Owner or admin only.
    ///
    /// # Errors
    /// `Forbidden` if not owner/admin; `Connection` on DB error.
    pub async fn reorder(
        &self,
        ctx: &AuthContext,
        album_id: AlbumId,
        asset_id: AssetId,
        position: i64,
    ) -> Result<(), DbError> {
        self.assert_owner(ctx, album_id).await?;
        sqlx::query(
            "UPDATE album_assets SET position = $3 \
             WHERE album_id = $1 AND asset_id = $2",
        )
        .bind(album_id.as_uuid())
        .bind(asset_id.as_uuid())
        .bind(position)
        .execute(self.db.pool())
        .await?;
        Ok(())
    }

    /// Lists the album's assets in manual order. Visible to anyone who can
    /// see the album (owner, admin, users with a permission on the album).
    ///
    /// # Errors
    /// `Forbidden` if the album is not visible; `Connection` on DB error.
    pub async fn list_assets(
        &self,
        ctx: &AuthContext,
        album_id: AlbumId,
    ) -> Result<Vec<AlbumAsset>, DbError> {
        self.assert_visible(ctx, album_id).await?;

        let rows: Vec<AlbumAssetRow> = sqlx::query_as(
            "SELECT a.id AS asset_id, a.folder_id, a.filename, a.content_hash, \
                    a.size_bytes, a.mtime, a.inode, a.kind, a.status, a.taken_at_utc, \
                    a.width, a.height, a.thumbhash, a.created_at, \
                    aa.position, aa.added_by, aa.added_at \
               FROM album_assets aa \
               JOIN assets a ON a.id = aa.asset_id \
              WHERE aa.album_id = $1 \
              ORDER BY aa.position ASC, a.id ASC",
        )
        .bind(album_id.as_uuid())
        .fetch_all(self.db.pool())
        .await?;

        rows.into_iter()
            .map(|r| {
                let asset_row = AssetRow::from_raw(
                    r.asset_id,
                    r.folder_id,
                    r.filename,
                    r.content_hash,
                    r.size_bytes,
                    r.mtime,
                    r.inode,
                    r.kind,
                    r.status,
                    r.taken_at_utc,
                    r.width,
                    r.height,
                    r.thumbhash,
                    r.created_at,
                );
                Ok(AlbumAsset {
                    asset: asset_row.into_domain()?,
                    position: r.position,
                    added_by: UserId::from_uuid(r.added_by),
                    added_at: r.added_at,
                })
            })
            .collect()
    }

    /// Albums (manual and dynamic together) that an asset already belongs
    /// to — used for the ALBUMS section of the lightbox info panel. No
    /// dedicated query existed for this direction before:
    /// [`Self::list_assets`] goes the other way (album -> asset). Dynamic
    /// albums are not re-evaluated here: their membership is already
    /// **materialized** in `album_assets` by [`Self::refresh`] (see the
    /// comment on its definition), so the same join used by
    /// [`Self::list_assets`] covers both types without needing to
    /// recompute any `rule`. Same visibility rule as [`Self::list`]: admin
    /// sees everything, otherwise only albums the caller owns or has a
    /// shared permission on.
    ///
    /// # Errors
    /// `Forbidden` without an authenticated user, or if the asset itself
    /// is not visible to the caller — otherwise membership alone in an
    /// album the caller **owns** would reveal the existence of an asset
    /// they could not otherwise see. `Connection` if the query fails.
    pub async fn for_asset(
        &self,
        ctx: &AuthContext,
        asset_id: AssetId,
    ) -> Result<Vec<AlbumBadge>, DbError> {
        AssetRepo::new(self.db)
            .assert_visible(ctx, std::slice::from_ref(&asset_id))
            .await?;
        if ctx.is_admin() {
            let rows: Vec<AlbumBadgeRow> = sqlx::query_as(
                "SELECT al.id, al.name FROM album_assets aa JOIN albums al ON al.id = aa.album_id \
                  WHERE aa.asset_id = $1 ORDER BY al.name ASC",
            )
            .bind(asset_id.as_uuid())
            .fetch_all(self.db.pool())
            .await?;
            return Ok(rows.into_iter().map(AlbumBadgeRow::into_domain).collect());
        }

        let Some(user_id) = ctx.user_id() else {
            return Err(DbError::Forbidden);
        };
        let rows: Vec<AlbumBadgeRow> = sqlx::query_as(
            "SELECT al.id, al.name FROM album_assets aa JOIN albums al ON al.id = aa.album_id \
              WHERE aa.asset_id = $1 \
                AND (al.owner_id = $2 \
                     OR EXISTS ( \
                          SELECT 1 FROM permissions \
                           WHERE object_type = 'album' AND object_id = al.id \
                             AND ( \
                                  (subject_type = 'user'  AND subject_id = $2) \
                               OR (subject_type = 'group' AND subject_id IN ( \
                                     SELECT group_id FROM group_members WHERE user_id = $2 \
                                  )) \
                             ) \
                        )) \
              ORDER BY al.name ASC",
        )
        .bind(asset_id.as_uuid())
        .bind(user_id.as_uuid())
        .fetch_all(self.db.pool())
        .await?;
        Ok(rows.into_iter().map(AlbumBadgeRow::into_domain).collect())
    }

    /// Recomputes the album's membership from the `rule` it was created
    /// with and writes the difference to `album_assets` (this is why we
    /// ended up with a "refresh album" action instead of a dynamic album
    /// that would recompute on every grid open). Updates `rule_run_at`.
    /// Owner or admin only.
    ///
    /// Returns `None` if the album has no `rule`: that is not an
    /// authorization failure, there is simply nothing to rerun — the HTTP
    /// caller translates this into a `400`, not a `403`.
    ///
    /// # Errors
    /// `Forbidden` if not owner/admin; `Conflict` if the `rule` is nested
    /// too deeply; `Connection` on DB error.
    pub async fn refresh(
        &self,
        ctx: &AuthContext,
        album_id: AlbumId,
    ) -> Result<Option<AlbumRefresh>, DbError> {
        self.assert_owner(ctx, album_id).await?;
        let actor = ctx.user_id().ok_or(DbError::Forbidden)?;

        let rule: Option<SqlxJson<SearchNode>> =
            sqlx::query_scalar("SELECT rule FROM albums WHERE id = $1")
                .bind(album_id.as_uuid())
                .fetch_one(self.db.pool())
                .await?;
        let Some(SqlxJson(rule)) = rule else {
            return Ok(None);
        };

        let scope = VisibilityScope::resolve(self.db, ctx).await?;
        let filter = scope.filter("f.path", "f.library_id", "a.id", 1);
        // Same $1,$2,$3 reused in the Semantic subquery: we want the top K
        // among assets visible to this owner, not K globally and then filtered.
        let semantic_vis = scope.filter("vf.path", "vf.library_id", "va.id", 1);
        let mut param = 4_usize;
        let (clause, binds) = compile_for_sql(
            &rule,
            &mut param,
            0,
            "a.location",
            Some(actor.as_uuid()),
            Some(semantic_vis.sql()),
        )?;
        let sql = format!(
            "SELECT a.id AS id FROM assets a \
             JOIN folders f ON f.id = a.folder_id \
             LEFT JOIN asset_exif e ON e.asset_id = a.id \
             WHERE {} AND a.status = 'indexed' AND ({clause})",
            filter.sql()
        );
        let mut q = sqlx::query(&sql)
            .bind(filter.bind())
            .bind(filter.holes())
            .bind(filter.assets());
        for b in &binds {
            q = bind_search(q, b);
        }
        let matched: HashSet<Uuid> = q
            .fetch_all(self.db.pool())
            .await?
            .into_iter()
            .map(|row| row.try_get::<Uuid, _>("id"))
            .collect::<Result<_, _>>()?;

        let current: HashSet<Uuid> =
            sqlx::query_scalar::<_, Uuid>("SELECT asset_id FROM album_assets WHERE album_id = $1")
                .bind(album_id.as_uuid())
                .fetch_all(self.db.pool())
                .await?
                .into_iter()
                .collect();

        let to_add: Vec<Uuid> = matched.difference(&current).copied().collect();
        let to_remove: Vec<Uuid> = current.difference(&matched).copied().collect();

        let mut tx = self.db.pool().begin().await?;
        for asset_id in &to_add {
            sqlx::query(
                "INSERT INTO album_assets (album_id, asset_id, position, added_by) \
                 VALUES ($1, $2, \
                   COALESCE((SELECT MAX(position) FROM album_assets WHERE album_id = $1), 0) \
                     + 1000, \
                   $3) \
                 ON CONFLICT (album_id, asset_id) DO NOTHING",
            )
            .bind(album_id.as_uuid())
            .bind(asset_id)
            .bind(actor.as_uuid())
            .execute(&mut *tx)
            .await?;
        }
        if !to_remove.is_empty() {
            sqlx::query("DELETE FROM album_assets WHERE album_id = $1 AND asset_id = ANY($2)")
                .bind(album_id.as_uuid())
                .bind(&to_remove)
                .execute(&mut *tx)
                .await?;
        }
        sqlx::query("UPDATE albums SET rule_run_at = now() WHERE id = $1")
            .bind(album_id.as_uuid())
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;

        Ok(Some(AlbumRefresh {
            added: to_add.into_iter().map(AssetId::from_uuid).collect(),
            removed: to_remove.into_iter().map(AssetId::from_uuid).collect(),
        }))
    }
}

fn bind_search<'q>(
    q: sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>,
    b: &'q SearchBind,
) -> sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments> {
    match b {
        SearchBind::Text(s) => q.bind(s),
        SearchBind::I32(n) => q.bind(n),
        SearchBind::F32(n) => q.bind(n),
        SearchBind::Uuid(u) => q.bind(u),
        SearchBind::Ts(t) => q.bind(t),
        SearchBind::I64(n) => q.bind(n),
    }
}
