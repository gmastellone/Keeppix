//! Album virtuali (spec §5). Nessuno storage: una foto può stare in N album.
//! Ordine manuale via `position`. Autorizzazione: owner o admin; utenti
//! condivisi tramite permesso diretto sull'album.

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
    /// Il `SearchNode` con cui l'album è stato creato, se presente. Un album
    /// senza `rule` è un album puramente manuale: non può essere aggiornato
    /// (Task 5, «Aggiorna album» invece di album dinamici).
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
    /// Il filtro con cui l'album nasce, se creato da una ricerca (spec
    /// fase-10 §5.2). `None` per un album puramente manuale.
    pub rule: Option<SearchNode>,
}

/// Esito di [`AlbumRepo::refresh`]: gli asset id entrati e usciti da
/// `album_assets` in questa esecuzione. Il chiamante (livello HTTP) lo
/// traduce nell'involucro di riuscita parziale (`BulkOutcome`, Task 1).
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

impl<'a> AlbumRepo<'a> {
    #[must_use]
    pub const fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// Verifica che il chiamante possa accedere all'album (owner, admin, o
    /// permesso diretto). Ritorna `Forbidden` — non `NotFound` — per non
    /// offrire un oracolo di esistenza.
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

    /// Verifica che il chiamante sia owner o admin (può modificare/eliminare).
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

    /// Crea un nuovo album vuoto. Il chiamante diventa owner.
    ///
    /// # Errors
    /// `Forbidden` senza utente autenticato; `Connection` su errore DB.
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

    /// Elenca gli album visibili al chiamante (propri + condivisi).
    ///
    /// # Errors
    /// `Forbidden` senza utente; `Connection` su errore DB.
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

    /// Recupera un album per id.
    ///
    /// # Errors
    /// `Forbidden` se non esiste o non visibile (mai `NotFound`).
    pub async fn get(&self, ctx: &AuthContext, album_id: AlbumId) -> Result<Album, DbError> {
        self.assert_visible(ctx, album_id).await?;
        let row: AlbumRow =
            sqlx::query_as(&format!("SELECT {ALBUM_COLUMNS} FROM albums WHERE id = $1"))
                .bind(album_id.as_uuid())
                .fetch_one(self.db.pool())
                .await?;
        Ok(row.into_domain())
    }

    /// Aggiorna nome, descrizione e/o copertina. Solo owner o admin.
    ///
    /// # Errors
    /// `Forbidden` se non owner/admin; `Connection` su errore DB.
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

    /// Elimina l'album. Gli asset non vengono toccati. Solo owner o admin.
    ///
    /// # Errors
    /// `Forbidden` se non owner/admin; `Connection` su errore DB.
    pub async fn delete(&self, ctx: &AuthContext, album_id: AlbumId) -> Result<(), DbError> {
        self.assert_owner(ctx, album_id).await?;
        sqlx::query("DELETE FROM albums WHERE id = $1")
            .bind(album_id.as_uuid())
            .execute(self.db.pool())
            .await?;
        Ok(())
    }

    /// Aggiunge un asset all'album. La position viene assegnata in fondo
    /// (MAX + 1000, o 1000 se l'album è vuoto). Solo owner o admin.
    ///
    /// # Errors
    /// `Forbidden` se non owner/admin o asset non visibile; `Connection` DB.
    pub async fn add_asset(
        &self,
        ctx: &AuthContext,
        album_id: AlbumId,
        asset_id: AssetId,
    ) -> Result<(), DbError> {
        self.assert_owner(ctx, album_id).await?;
        // Verifica che l'asset esista ed è visibile al chiamante.
        // find_by_id ritorna Forbidden se non visibile — mai NotFound per utenti normali.
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

    /// Rimuove un asset dall'album. L'asset **non** viene eliminato.
    /// Solo owner o admin.
    ///
    /// # Errors
    /// `Forbidden` se non owner/admin; `Connection` DB.
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

    /// Riordina un asset nell'album assegnandogli una nuova position.
    /// Solo owner o admin.
    ///
    /// # Errors
    /// `Forbidden` se non owner/admin; `Connection` DB.
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

    /// Elenca gli asset dell'album nell'ordine manuale. Visibile a chi vede
    /// l'album (owner, admin, utenti con permesso sull'album).
    ///
    /// # Errors
    /// `Forbidden` se l'album non è visibile; `Connection` DB.
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

    /// Ricalcola l'appartenenza dell'album dalla `rule` con cui è nato e
    /// scrive la differenza in `album_assets` (Task 5, «Aggiorna album» al
    /// posto di un album dinamico che ricalcolerebbe a ogni apertura della
    /// griglia). Aggiorna `rule_run_at`. Solo owner o admin.
    ///
    /// Ritorna `None` se l'album non ha una `rule`: non è un difetto di
    /// autorizzazione, è che non c'è nulla da rilanciare — il chiamante HTTP
    /// lo traduce in un `400`, non in un `403`.
    ///
    /// # Errors
    /// `Forbidden` se non owner/admin; `Conflict` se la `rule` è troppo
    /// annidata; `Connection` su errore DB.
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
        let mut param = 4_usize;
        let (clause, binds) = compile_for_sql(&rule, &mut param, 0, "a.location")?;
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
        SearchBind::Uuid(u) => q.bind(u),
        SearchBind::Ts(t) => q.bind(t),
    }
}
