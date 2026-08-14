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
struct AssetRow {
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
    created_at: DateTime<Utc>,
}

impl AssetRow {
    fn into_domain(self) -> Result<Asset, DbError> {
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
            created_at: self.created_at,
        })
    }
}

const COLUMNS: &str = "id, folder_id, filename, content_hash, size_bytes, mtime, inode, \
                       kind, status, taken_at_utc, width, height, created_at";
const A_COLUMNS: &str = "a.id, a.folder_id, a.filename, a.content_hash, a.size_bytes, a.mtime, a.inode, \
                         a.kind, a.status, a.taken_at_utc, a.width, a.height, a.created_at";

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
    /// Non prende un `AuthContext` perché la chiama lo scanner.
    ///
    /// # Errors
    /// `Connection` se l'inserimento fallisce; `Corrupted` se la riga
    /// restituita non passa la validazione di dominio.
    pub async fn upsert_discovered(&self, new: NewAsset) -> Result<Asset, DbError> {
        let row: AssetRow = sqlx::query_as(&format!(
            "INSERT INTO assets (id, folder_id, filename, size_bytes, mtime, inode, kind) \
             VALUES ($1, $2, $3, $4, $5, $6, $7) \
             ON CONFLICT (folder_id, filename) DO UPDATE SET \
                size_bytes = EXCLUDED.size_bytes, \
                mtime = EXCLUDED.mtime, \
                inode = EXCLUDED.inode, \
                kind = EXCLUDED.kind, \
                updated_at = now() \
             RETURNING {COLUMNS}"
        ))
        .bind(AssetId::new().as_uuid())
        .bind(new.folder_id.as_uuid())
        .bind(new.filename.as_str())
        .bind(new.size_bytes)
        .bind(new.mtime)
        .bind(new.inode)
        .bind(kind_str(new.kind))
        .fetch_one(self.db.pool())
        .await?;

        row.into_domain()
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
            "SELECT {COLUMNS} FROM assets WHERE folder_id = $1 ORDER BY filename"
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
}
