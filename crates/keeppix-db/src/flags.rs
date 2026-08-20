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
        })
    }
}

impl<'a> FlagRepo<'a> {
    #[must_use]
    pub const fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// Scrive i flag del chiamante su un asset. Sono per utente: due
    /// chiamanti sullo stesso asset non si sovrascrivono (spec §4.1).
    ///
    /// # Errors
    /// `Forbidden` se il chiamante non vede l'asset — anche quando l'id non
    /// esiste, per non offrire un oracolo di esistenza.
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
            "INSERT INTO asset_flags (asset_id, user_id, rating, pick, color_label, updated_at) \
             VALUES ($1, $2, $3, $4, $5, now()) \
             ON CONFLICT (asset_id, user_id) DO UPDATE SET \
                rating = EXCLUDED.rating, \
                pick = EXCLUDED.pick, \
                color_label = EXCLUDED.color_label, \
                updated_at = now()",
        )
        .bind(asset_id.as_uuid())
        .bind(user_id.as_uuid())
        .bind(flags.rating.map(|r| i16::from(r.value())))
        .bind(flags.pick.as_str())
        .bind(&flags.color_label)
        .execute(self.db.pool())
        .await?;
        Ok(())
    }

    /// Flag del chiamante su un asset, o i valori di default se non ha
    /// ancora votato.
    ///
    /// # Errors
    /// Come [`Self::set`].
    pub async fn get(&self, ctx: &AuthContext, asset_id: AssetId) -> Result<AssetFlags, DbError> {
        AssetRepo::new(self.db)
            .assert_visible(ctx, std::slice::from_ref(&asset_id))
            .await?;
        let Some(user_id) = ctx.user_id() else {
            return Ok(AssetFlags::default());
        };

        let row: Option<FlagRow> = sqlx::query_as(
            "SELECT rating, pick, color_label FROM asset_flags \
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

    /// Applica gli stessi flag a molti asset in un'unica operazione — non un
    /// round-trip per asset.
    ///
    /// # Errors
    /// `Forbidden` se anche uno solo degli asset non è visibile.
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
            "INSERT INTO asset_flags (asset_id, user_id, rating, pick, color_label, updated_at) \
             SELECT aid, $2, $3, $4, $5, now() FROM unnest($1::uuid[]) AS aid \
             ON CONFLICT (asset_id, user_id) DO UPDATE SET \
                rating = EXCLUDED.rating, \
                pick = EXCLUDED.pick, \
                color_label = EXCLUDED.color_label, \
                updated_at = now()",
        )
        .bind(&ids)
        .bind(user_id.as_uuid())
        .bind(flags.rating.map(|r| i16::from(r.value())))
        .bind(flags.pick.as_str())
        .bind(&flags.color_label)
        .execute(self.db.pool())
        .await?;
        Ok(())
    }

    /// Come [`Self::batch_set`], ma a riuscita parziale: gli asset non
    /// visibili finiscono in `failed` invece di abortire l'intero lotto.
    ///
    /// # Errors
    /// `Forbidden` se il contesto non ha un utente; `Connection` sul DB.
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
                "INSERT INTO asset_flags (asset_id, user_id, rating, pick, color_label, updated_at) \
                 SELECT aid, $2, $3, $4, $5, now() FROM unnest($1::uuid[]) AS aid \
                 ON CONFLICT (asset_id, user_id) DO UPDATE SET \
                    rating = EXCLUDED.rating, \
                    pick = EXCLUDED.pick, \
                    color_label = EXCLUDED.color_label, \
                    updated_at = now()",
            )
            .bind(&ids)
            .bind(user_id.as_uuid())
            .bind(flags.rating.map(|r| i16::from(r.value())))
            .bind(flags.pick.as_str())
            .bind(&flags.color_label)
            .execute(self.db.pool())
            .await?;
        }

        Ok((succeeded, failed))
    }
}
