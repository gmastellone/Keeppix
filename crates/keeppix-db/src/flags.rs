use std::collections::HashSet;

use keeppix_domain::{AssetFlags, AssetId, AuthContext, Pick, Rating};

use crate::{AssetRepo, Db, DbError};

pub struct FlagRepo<'a> {
    db: &'a Db,
}

#[derive(sqlx::FromRow)]
struct FlagRow {
    rating: Option<i16>,
    pick: Option<String>,
    color_label: Option<String>,
    favorite: bool,
}

impl FlagRow {
    fn into_domain(self) -> Result<AssetFlags, DbError> {
        let rating = self
            .rating
            .map(|raw| {
                u8::try_from(raw)
                    .map_err(|e| crate::row::corrupted("asset_flags.rating", e))
                    .and_then(|raw| {
                        Rating::parse(raw)
                            .map_err(|e| crate::row::corrupted("asset_flags.rating", e))
                    })
            })
            .transpose()?;
        let pick = self
            .pick
            .as_deref()
            .map(|raw| Pick::parse(raw).map_err(|e| crate::row::corrupted("asset_flags.pick", e)))
            .transpose()?
            .unwrap_or_default();
        Ok(AssetFlags {
            rating,
            pick,
            color_label: self.color_label,
            favorite: self.favorite,
        })
    }
}

impl<'a> FlagRepo<'a> {
    #[must_use]
    pub const fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// Writes the caller's flags on an asset. These are per-user: two
    /// callers on the same asset do not overwrite each other.
    ///
    /// # Errors
    /// `Forbidden` if the caller cannot see the asset — including when the
    /// id does not exist, so as not to offer an existence oracle.
    pub async fn set(
        &self,
        ctx: &AuthContext,
        asset_id: AssetId,
        flags: &AssetFlags,
    ) -> Result<(), DbError> {
        AssetRepo::new(self.db)
            .assert_visible(ctx, std::slice::from_ref(&asset_id))
            .await?;
        let Some(user_id) = ctx.user_id() else {
            return Err(DbError::Forbidden);
        };

        sqlx::query(
            "INSERT INTO asset_flags (asset_id, user_id, rating, pick, color_label, favorite, updated_at) \
             VALUES ($1, $2, $3, $4, $5, $6, now()) \
             ON CONFLICT (asset_id, user_id) DO UPDATE SET \
                rating = EXCLUDED.rating, \
                pick = EXCLUDED.pick, \
                color_label = EXCLUDED.color_label, \
                favorite = EXCLUDED.favorite, \
                updated_at = now()",
        )
        .bind(asset_id.as_uuid())
        .bind(user_id.as_uuid())
        .bind(flags.rating.map(|r| i16::from(r.value())))
        .bind(flags.pick.as_str())
        .bind(&flags.color_label)
        .bind(flags.favorite)
        .execute(self.db.pool())
        .await?;
        Ok(())
    }

    /// The caller's flags on an asset, or the default values if they have
    /// not voted yet.
    ///
    /// # Errors
    /// Same as [`Self::set`].
    pub async fn get(&self, ctx: &AuthContext, asset_id: AssetId) -> Result<AssetFlags, DbError> {
        AssetRepo::new(self.db)
            .assert_visible(ctx, std::slice::from_ref(&asset_id))
            .await?;
        let Some(user_id) = ctx.user_id() else {
            return Ok(AssetFlags::default());
        };

        let row: Option<FlagRow> = sqlx::query_as(
            "SELECT rating, pick, color_label, favorite FROM asset_flags \
              WHERE asset_id = $1 AND user_id = $2",
        )
        .bind(asset_id.as_uuid())
        .bind(user_id.as_uuid())
        .fetch_optional(self.db.pool())
        .await?;

        match row {
            Some(row) => row.into_domain(),
            None => Ok(AssetFlags::default()),
        }
    }

    /// Applies the same flags to many assets in a single operation — not
    /// one round trip per asset.
    ///
    /// # Errors
    /// `Forbidden` if even one asset is not visible.
    pub async fn batch_set(
        &self,
        ctx: &AuthContext,
        asset_ids: &[AssetId],
        flags: &AssetFlags,
    ) -> Result<(), DbError> {
        AssetRepo::new(self.db)
            .assert_visible(ctx, asset_ids)
            .await?;
        let Some(user_id) = ctx.user_id() else {
            return Err(DbError::Forbidden);
        };
        if asset_ids.is_empty() {
            return Ok(());
        }

        let ids: Vec<uuid::Uuid> = asset_ids.iter().map(AssetId::as_uuid).collect();
        sqlx::query(
            "INSERT INTO asset_flags (asset_id, user_id, rating, pick, color_label, favorite, updated_at) \
             SELECT aid, $2, $3, $4, $5, $6, now() FROM unnest($1::uuid[]) AS aid \
             ON CONFLICT (asset_id, user_id) DO UPDATE SET \
                rating = EXCLUDED.rating, \
                pick = EXCLUDED.pick, \
                color_label = EXCLUDED.color_label, \
                favorite = EXCLUDED.favorite, \
                updated_at = now()",
        )
        .bind(&ids)
        .bind(user_id.as_uuid())
        .bind(flags.rating.map(|r| i16::from(r.value())))
        .bind(flags.pick.as_str())
        .bind(&flags.color_label)
        .bind(flags.favorite)
        .execute(self.db.pool())
        .await?;
        Ok(())
    }

    /// Like [`Self::batch_set`], but partial-success: assets that are not
    /// visible end up in `failed` instead of aborting the whole batch.
    ///
    /// # Errors
    /// `Forbidden` if the context has no user; `Connection` on DB error.
    pub async fn batch_set_partial(
        &self,
        ctx: &AuthContext,
        asset_ids: &[AssetId],
        flags: &AssetFlags,
    ) -> Result<(Vec<AssetId>, Vec<(AssetId, DbError)>), DbError> {
        let Some(user_id) = ctx.user_id() else {
            return Err(DbError::Forbidden);
        };

        let visible = AssetRepo::new(self.db)
            .filter_visible(ctx, asset_ids)
            .await?;
        let visible_set: std::collections::HashSet<uuid::Uuid> =
            visible.iter().map(AssetId::as_uuid).collect();

        let mut succeeded = Vec::new();
        let mut failed = Vec::new();
        for &id in asset_ids {
            if visible_set.contains(&id.as_uuid()) {
                succeeded.push(id);
            } else if ctx.is_admin() {
                failed.push((id, DbError::NotFound));
            } else {
                failed.push((id, DbError::Forbidden));
            }
        }

        if !succeeded.is_empty() {
            let ids: Vec<uuid::Uuid> = succeeded.iter().map(AssetId::as_uuid).collect();
            sqlx::query(
                "INSERT INTO asset_flags (asset_id, user_id, rating, pick, color_label, favorite, updated_at) \
                 SELECT aid, $2, $3, $4, $5, $6, now() FROM unnest($1::uuid[]) AS aid \
                 ON CONFLICT (asset_id, user_id) DO UPDATE SET \
                    rating = EXCLUDED.rating, \
                    pick = EXCLUDED.pick, \
                    color_label = EXCLUDED.color_label, \
                    favorite = EXCLUDED.favorite, \
                    updated_at = now()",
            )
            .bind(&ids)
            .bind(user_id.as_uuid())
            .bind(flags.rating.map(|r| i16::from(r.value())))
            .bind(flags.pick.as_str())
            .bind(&flags.color_label)
            .bind(flags.favorite)
            .execute(self.db.pool())
            .await?;
        }

        Ok((succeeded, failed))
    }

    /// The set of the caller's favorites among the given ids — used by the
    /// browse views (timeline, search) to decorate `AssetView.favorite`
    /// with **one** query instead of N. Does not re-check visibility: the
    /// ids arrive already filtered by whoever built the page
    /// (`TimelineRepo`/`SearchRepo`), and the `user_id = $1` filter never
    /// leaks another user's favorite.
    ///
    /// # Errors
    /// `Connection` on DB error. A context with no user (a public link) is
    /// not an error: it simply returns the empty set, because "favorite"
    /// makes no sense for a caller that cannot vote.
    pub async fn favorites_among(
        &self,
        ctx: &AuthContext,
        asset_ids: &[AssetId],
    ) -> Result<HashSet<AssetId>, DbError> {
        let Some(user_id) = ctx.user_id() else {
            return Ok(HashSet::new());
        };
        if asset_ids.is_empty() {
            return Ok(HashSet::new());
        }

        let ids: Vec<uuid::Uuid> = asset_ids.iter().map(AssetId::as_uuid).collect();
        let rows: Vec<(uuid::Uuid,)> = sqlx::query_as(
            "SELECT asset_id FROM asset_flags \
              WHERE user_id = $1 AND asset_id = ANY($2) AND favorite",
        )
        .bind(user_id.as_uuid())
        .bind(&ids)
        .fetch_all(self.db.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|(id,)| AssetId::from_uuid(id))
            .collect())
    }
}
