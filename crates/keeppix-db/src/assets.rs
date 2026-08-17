use chrono::{DateTime, Utc};
use keeppix_domain::{
    Asset, AssetId, AssetKind, AssetName, AssetStatus, AuthContext, ExifData, FolderId, LibraryId,
    NewAsset,
};

use crate::visibility::VisibilityScope;
use crate::{Db, DbError, FolderRepo};

pub struct AssetRepo<'a> {
    db: &'a Db,
}

#[derive(sqlx::FromRow)]
pub(crate) struct AssetRow {
    id: uuid::Uuid,
    folder_id: uuid::Uuid,
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
}

impl AssetRow {
    #[must_use]
    pub(crate) const fn id(&self) -> uuid::Uuid {
        self.id
    }

    pub(crate) fn into_domain(self) -> Result<Asset, DbError> {
        Ok(Asset {
            id: AssetId::from_uuid(self.id),
            folder_id: FolderId::from_uuid(self.folder_id),
            filename: AssetName::parse(&self.filename)
                .map_err(|e| crate::row::corrupted("filename", e))?,
            content_hash: match self.content_hash {
                None => None,
                Some(bytes) => Some(
                    <[u8; 32]>::try_from(bytes.as_slice())
                        .map_err(|_| crate::row::corrupted("content_hash", "not 32 bytes"))?,
                ),
            },
            size_bytes: self.size_bytes,
            mtime: self.mtime,
            inode: self.inode,
            kind: parse_kind(&self.kind)?,
            status: parse_status(&self.status)?,
            taken_at_utc: self.taken_at_utc,
            width: self.width,
            height: self.height,
            thumbhash: self.thumbhash,
            created_at: self.created_at,
        })
    }
}

const COLUMNS: &str = "id, folder_id, filename, content_hash, size_bytes, mtime, inode, \
                       kind, status, taken_at_utc, width, height, thumbhash, created_at";
pub(crate) const A_COLUMNS: &str = "a.id, a.folder_id, a.filename, a.content_hash, a.size_bytes, a.mtime, a.inode, \
                         a.kind, a.status, a.taken_at_utc, a.width, a.height, a.thumbhash, a.created_at";

const fn kind_str(kind: AssetKind) -> &'static str {
    match kind {
        AssetKind::Image => "image",
        AssetKind::RawImage => "raw_image",
        AssetKind::Video => "video",
        AssetKind::Unknown => "unknown",
    }
}

const fn status_str(status: AssetStatus) -> &'static str {
    match status {
        AssetStatus::Discovered => "discovered",
        AssetStatus::Indexed => "indexed",
        AssetStatus::Offline => "offline",
        AssetStatus::Error => "error",
        AssetStatus::Trashed => "trashed",
    }
}

fn parse_kind(raw: &str) -> Result<AssetKind, DbError> {
    match raw {
        "image" => Ok(AssetKind::Image),
        "raw_image" => Ok(AssetKind::RawImage),
        "video" => Ok(AssetKind::Video),
        "unknown" => Ok(AssetKind::Unknown),
        other => Err(crate::row::corrupted("asset kind", other)),
    }
}

fn parse_status(raw: &str) -> Result<AssetStatus, DbError> {
    match raw {
        "discovered" => Ok(AssetStatus::Discovered),
        "indexed" => Ok(AssetStatus::Indexed),
        "offline" => Ok(AssetStatus::Offline),
        "error" => Ok(AssetStatus::Error),
        "trashed" => Ok(AssetStatus::Trashed),
        other => Err(crate::row::corrupted("asset status", other)),
    }
}

impl<'a> AssetRepo<'a> {
    #[must_use]
    pub const fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// Inserisce il file trovato dal walker, o aggiorna size/mtime se c'è già.
    ///
    /// Restituisce `None` quando mtime e size sono identici a quelli già
    /// noti: il chiamante non deve riaccodare metadata/hash. `kind` si
    /// riazzera solo se il file è davvero cambiato — altrimenti una
    /// riscansione cancellerebbe `detect_kind`.
    ///
    /// Non prende un `AuthContext` perché la chiama lo scanner.
    ///
    /// # Errors
    /// `Connection` se l'inserimento fallisce; `Corrupted` se la riga
    /// restituita non passa la validazione di dominio.
    pub async fn upsert_discovered(&self, new: NewAsset) -> Result<Option<Asset>, DbError> {
        let row: Option<AssetRow> = sqlx::query_as(&format!(
            "INSERT INTO assets (id, folder_id, filename, size_bytes, mtime, inode, kind) \
             VALUES ($1, $2, $3, $4, $5, $6, $7) \
             ON CONFLICT (folder_id, filename) DO UPDATE SET \
                size_bytes = EXCLUDED.size_bytes, \
                mtime = EXCLUDED.mtime, \
                inode = EXCLUDED.inode, \
                kind = EXCLUDED.kind, \
                updated_at = now() \
             WHERE assets.mtime IS DISTINCT FROM EXCLUDED.mtime \
                OR assets.size_bytes IS DISTINCT FROM EXCLUDED.size_bytes \
             RETURNING {COLUMNS}"
        ))
        .bind(AssetId::new().as_uuid())
        .bind(new.folder_id.as_uuid())
        .bind(new.filename.as_str())
        .bind(new.size_bytes)
        .bind(new.mtime)
        .bind(new.inode)
        .bind(kind_str(new.kind))
        .fetch_optional(self.db.pool())
        .await?;

        row.map(AssetRow::into_domain).transpose()
    }

    /// Non prende un `AuthContext`: la chiama la pipeline di metadati.
    ///
    /// # Errors
    /// `Connection` se l'aggiornamento fallisce.
    pub async fn set_kind(&self, id: AssetId, kind: AssetKind) -> Result<(), DbError> {
        sqlx::query("UPDATE assets SET kind = $2, updated_at = now() WHERE id = $1")
            .bind(id.as_uuid())
            .bind(kind_str(kind))
            .execute(self.db.pool())
            .await?;
        Ok(())
    }

    /// Non prende un `AuthContext`: la chiama la pipeline di hashing.
    ///
    /// # Errors
    /// `Connection` se l'aggiornamento fallisce.
    pub async fn set_hash(&self, id: AssetId, hash: [u8; 32]) -> Result<(), DbError> {
        sqlx::query("UPDATE assets SET content_hash = $2, updated_at = now() WHERE id = $1")
            .bind(id.as_uuid())
            .bind(hash.as_slice())
            .execute(self.db.pool())
            .await?;
        Ok(())
    }

    /// Non prende un `AuthContext`: la chiama la pipeline di indicizzazione.
    ///
    /// # Errors
    /// `Connection` se l'aggiornamento fallisce.
    pub async fn set_indexed(
        &self,
        id: AssetId,
        taken_at_utc: DateTime<Utc>,
        width: i32,
        height: i32,
    ) -> Result<(), DbError> {
        sqlx::query(
            "UPDATE assets SET status = 'indexed', taken_at_utc = $2, width = $3, height = $4, \
                    updated_at = now() WHERE id = $1",
        )
        .bind(id.as_uuid())
        .bind(taken_at_utc)
        .bind(width)
        .bind(height)
        .execute(self.db.pool())
        .await?;
        Ok(())
    }

    /// Non prende un `AuthContext`: la chiama la pipeline.
    ///
    /// # Errors
    /// `Connection` se l'aggiornamento fallisce.
    pub async fn set_error(&self, id: AssetId, detail: &str) -> Result<(), DbError> {
        sqlx::query(
            "UPDATE assets SET status = 'error', error_detail = $2, updated_at = now() WHERE id = $1",
        )
        .bind(id.as_uuid())
        .bind(detail)
        .execute(self.db.pool())
        .await?;
        Ok(())
    }

    /// Non prende un `AuthContext`: la chiama lo scanner quando il file sparisce.
    ///
    /// # Errors
    /// `Connection` se l'aggiornamento fallisce.
    pub async fn mark_offline(&self, id: AssetId) -> Result<(), DbError> {
        sqlx::query("UPDATE assets SET status = 'offline', updated_at = now() WHERE id = $1")
            .bind(id.as_uuid())
            .execute(self.db.pool())
            .await?;
        Ok(())
    }

    /// # Errors
    /// `Forbidden` se il chiamante non vede l'asset — anche quando l'id non
    /// esiste. `NotFound` solo a un admin che chiede un id inesistente.
    pub async fn find_by_id(&self, ctx: &AuthContext, id: AssetId) -> Result<Asset, DbError> {
        let scope = VisibilityScope::resolve(self.db, ctx).await?;
        let filter = scope.filter("f.library_id", 2);
        let row: Option<AssetRow> = sqlx::query_as(&format!(
            "SELECT {A_COLUMNS} FROM assets a \
             JOIN folders f ON f.id = a.folder_id \
             WHERE a.id = $1 AND {}",
            filter.sql()
        ))
        .bind(id.as_uuid())
        .bind(filter.bind())
        .fetch_optional(self.db.pool())
        .await?;

        match row {
            Some(row) => row.into_domain(),
            None if ctx.is_admin() => Err(DbError::NotFound),
            None => Err(DbError::Forbidden),
        }
    }

    /// # Errors
    /// Come `FolderRepo::find_by_id` sulla cartella, poi gli asset al suo interno.
    pub async fn find_by_folder(
        &self,
        ctx: &AuthContext,
        folder_id: FolderId,
    ) -> Result<Vec<Asset>, DbError> {
        FolderRepo::new(self.db).find_by_id(ctx, folder_id).await?;

        let rows: Vec<AssetRow> = sqlx::query_as(&format!(
            "SELECT {COLUMNS} FROM assets WHERE folder_id = $1 AND status <> 'trashed' ORDER BY filename"
        ))
        .bind(folder_id.as_uuid())
        .fetch_all(self.db.pool())
        .await?;

        rows.into_iter().map(AssetRow::into_domain).collect()
    }

    /// # Errors
    /// `Connection` se la query fallisce.
    pub async fn find_by_hash(
        &self,
        ctx: &AuthContext,
        hash: &[u8; 32],
    ) -> Result<Vec<Asset>, DbError> {
        let scope = VisibilityScope::resolve(self.db, ctx).await?;
        let filter = scope.filter("f.library_id", 2);
        let rows: Vec<AssetRow> = sqlx::query_as(&format!(
            "SELECT {A_COLUMNS} FROM assets a \
             JOIN folders f ON f.id = a.folder_id \
             WHERE a.content_hash = $1 AND {} \
             ORDER BY a.filename",
            filter.sql()
        ))
        .bind(hash.as_slice())
        .bind(filter.bind())
        .fetch_all(self.db.pool())
        .await?;

        rows.into_iter().map(AssetRow::into_domain).collect()
    }

    /// # Errors
    /// `Connection` se la query fallisce.
    pub async fn count_by_status(
        &self,
        ctx: &AuthContext,
        status: AssetStatus,
    ) -> Result<i64, DbError> {
        let scope = VisibilityScope::resolve(self.db, ctx).await?;
        let filter = scope.filter("f.library_id", 2);
        let n: i64 = sqlx::query_scalar(&format!(
            "SELECT count(*) FROM assets a \
             JOIN folders f ON f.id = a.folder_id \
             WHERE a.status = $1 AND {}",
            filter.sql()
        ))
        .bind(status_str(status))
        .bind(filter.bind())
        .fetch_one(self.db.pool())
        .await?;
        Ok(n)
    }

    /// Non prende un `AuthContext`: la chiama lo scanner sul job reclamato.
    ///
    /// # Errors
    /// `NotFound` se l'id non esiste.
    pub async fn get_for_scan(&self, id: AssetId) -> Result<Asset, DbError> {
        let row: Option<AssetRow> =
            sqlx::query_as(&format!("SELECT {COLUMNS} FROM assets WHERE id = $1"))
                .bind(id.as_uuid())
                .fetch_optional(self.db.pool())
                .await?;
        row.map(AssetRow::into_domain)
            .transpose()?
            .ok_or(DbError::NotFound)
    }

    /// Conteggio nella libreria, per le soglie di sparizione di massa.
    ///
    /// Non prende un `AuthContext`: la chiama lo scanner.
    ///
    /// # Errors
    /// `Connection` se la query fallisce.
    pub async fn count_in_library(&self, library_id: LibraryId) -> Result<i64, DbError> {
        let n: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM assets a \
             JOIN folders f ON f.id = a.folder_id \
             WHERE f.library_id = $1 AND a.status <> 'trashed'",
        )
        .bind(library_id.as_uuid())
        .fetch_one(self.db.pool())
        .await?;
        Ok(n)
    }

    /// I metadati originali sono immutabili: un secondo insert non sovrascrive.
    ///
    /// Non prende un `AuthContext`: la chiama la pipeline.
    ///
    /// # Errors
    /// `Connection` se l'inserimento fallisce.
    pub async fn insert_exif(&self, asset_id: AssetId, exif: &ExifData) -> Result<(), DbError> {
        sqlx::query(
            "INSERT INTO asset_exif \
                (asset_id, raw, camera_make, camera_model, lens, iso, f_number, exposure, focal_length) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) \
             ON CONFLICT (asset_id) DO NOTHING",
        )
        .bind(asset_id.as_uuid())
        .bind(&exif.raw)
        .bind(&exif.camera_make)
        .bind(&exif.camera_model)
        .bind(&exif.lens)
        .bind(exif.iso)
        .bind(exif.f_number)
        .bind(&exif.exposure)
        .bind(exif.focal_length)
        .execute(self.db.pool())
        .await?;
        Ok(())
    }

    /// Non prende un `AuthContext`: la chiama la pipeline dei derivati.
    ///
    /// # Errors
    /// `Connection` se l'aggiornamento fallisce.
    pub async fn set_thumbhash_for_hash(
        &self,
        hash: &[u8; 32],
        thumbhash: &[u8],
    ) -> Result<u64, DbError> {
        let result = sqlx::query(
            "UPDATE assets SET \
                 thumbhash = $2, \
                 error_detail = NULL, \
                 status = CASE \
                     WHEN status = 'error' AND taken_at_utc IS NOT NULL THEN 'indexed' \
                     WHEN status = 'error' THEN 'discovered' \
                     ELSE status \
                 END, \
                 updated_at = now() \
              WHERE content_hash = $1",
        )
        .bind(hash.as_slice())
        .bind(thumbhash)
        .execute(self.db.pool())
        .await?;
        Ok(result.rows_affected())
    }

    /// Copia il thumbhash già noto sugli asset con lo stesso `content_hash`
    /// che ancora non ce l'hanno. Lo chiama `DeriveRaw` sul ramo
    /// idempotente: il file derivato esiste, ma un duplicato hashed dopo
    /// la prima derivazione resterebbe senza placeholder.
    ///
    /// Non prende un `AuthContext`: è la pipeline dei derivati, come
    /// [`Self::set_thumbhash_for_hash`].
    ///
    /// # Errors
    /// `Connection` se l'aggiornamento fallisce.
    pub async fn propagate_thumbhash_for_hash(&self, hash: &[u8; 32]) -> Result<u64, DbError> {
        let result = sqlx::query(
            "UPDATE assets SET thumbhash = src.thumbhash, updated_at = now() \
             FROM (SELECT thumbhash FROM assets \
                    WHERE content_hash = $1 AND thumbhash IS NOT NULL LIMIT 1) src \
             WHERE assets.content_hash = $1 AND assets.thumbhash IS NULL",
        )
        .bind(hash.as_slice())
        .execute(self.db.pool())
        .await?;
        Ok(result.rows_affected())
    }

    /// Non prende un `AuthContext`: la chiama la pipeline dei derivati.
    ///
    /// # Errors
    /// `Connection` se la query fallisce.
    pub async fn ids_with_hash(&self, hash: &[u8; 32]) -> Result<Vec<AssetId>, DbError> {
        let rows: Vec<uuid::Uuid> =
            sqlx::query_scalar("SELECT id FROM assets WHERE content_hash = $1")
                .bind(hash.as_slice())
                .fetch_all(self.db.pool())
                .await?;
        Ok(rows.into_iter().map(AssetId::from_uuid).collect())
    }

    /// Hash degli asset in `error` da ritentare. Lo chiama il job di
    /// manutenzione, non un utente: niente `AuthContext`.
    ///
    /// # Errors
    /// `Connection` se la query fallisce.
    pub async fn error_hashes_for_retry(&self) -> Result<Vec<([u8; 32], AssetKind)>, DbError> {
        let rows: Vec<(Vec<u8>, String)> = sqlx::query_as(
            "SELECT DISTINCT ON (content_hash) content_hash, kind FROM assets \
              WHERE status = 'error' AND content_hash IS NOT NULL \
              ORDER BY content_hash, id",
        )
        .fetch_all(self.db.pool())
        .await?;

        let mut out = Vec::with_capacity(rows.len());
        for (bytes, kind) in rows {
            let Ok(hash) = <[u8; 32]>::try_from(bytes.as_slice()) else {
                continue;
            };
            let Ok(kind) = parse_kind(&kind) else {
                continue;
            };
            out.push((hash, kind));
        }
        Ok(out)
    }

    /// Verifica in una sola query che il chiamante veda **tutti** gli id
    /// dati. La usano gli override e i flag per non pagare un round-trip per
    /// asset su un batch di 500: chi sonda anche un solo id fuori dalla
    /// propria visibilità riceve `Forbidden`, mai un errore parziale che
    /// riveli quali id esistono.
    ///
    /// # Errors
    /// `Forbidden` se anche un solo id non è visibile — compreso il caso in
    /// cui non esiste affatto. `NotFound` solo a un admin quando un id non
    /// esiste per niente.
    pub async fn assert_visible(&self, ctx: &AuthContext, ids: &[AssetId]) -> Result<(), DbError> {
        if ids.is_empty() {
            return Ok(());
        }
        let uuids: Vec<uuid::Uuid> = ids.iter().map(AssetId::as_uuid).collect();
        let scope = VisibilityScope::resolve(self.db, ctx).await?;
        let filter = scope.filter("f.library_id", 2);
        let visible: i64 = sqlx::query_scalar(&format!(
            "SELECT count(DISTINCT a.id) FROM assets a \
             JOIN folders f ON f.id = a.folder_id \
             WHERE a.id = ANY($1) AND {}",
            filter.sql()
        ))
        .bind(&uuids)
        .bind(filter.bind())
        .fetch_one(self.db.pool())
        .await?;

        let mut distinct = uuids;
        distinct.sort_unstable();
        distinct.dedup();

        if visible == i64::try_from(distinct.len()).unwrap_or(i64::MAX) {
            Ok(())
        } else if ctx.is_admin() {
            Err(DbError::NotFound)
        } else {
            Err(DbError::Forbidden)
        }
    }

    /// Copia gli EXIF originali su un nuovo asset. `ON CONFLICT DO NOTHING`.
    ///
    /// Non prende un `AuthContext`: la chiama il rilevatore di spostamenti.
    ///
    /// # Errors
    /// `Connection` se la query fallisce.
    pub async fn copy_exif(&self, from: AssetId, to: AssetId) -> Result<(), DbError> {
        sqlx::query(
            "INSERT INTO asset_exif \
                (asset_id, raw, camera_make, camera_model, lens, iso, f_number, exposure, focal_length) \
             SELECT $2, raw, camera_make, camera_model, lens, iso, f_number, exposure, focal_length \
               FROM asset_exif WHERE asset_id = $1 \
             ON CONFLICT (asset_id) DO NOTHING",
        )
        .bind(from.as_uuid())
        .bind(to.as_uuid())
        .execute(self.db.pool())
        .await?;
        Ok(())
    }
}
