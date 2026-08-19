use chrono::{DateTime, Utc};
use keeppix_domain::AuthContext;

use crate::{Db, DbError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegionStatus {
    Available,
    Downloading,
    Error,
}

impl RegionStatus {
    fn parse(raw: &str) -> Result<Self, DbError> {
        match raw {
            "available" => Ok(Self::Available),
            "downloading" => Ok(Self::Downloading),
            "error" => Ok(Self::Error),
            other => Err(crate::row::corrupted("map region status", other)),
        }
    }
}

#[derive(Debug, Clone)]
pub struct MapRegion {
    pub id: String,
    pub label: String,
    pub file_path: String,
    pub size_bytes: i64,
    pub version: String,
    pub downloaded_at: Option<DateTime<Utc>>,
    pub status: RegionStatus,
    pub source_url: String,
    pub checksum_sha256: String,
    pub downloaded_bytes: i64,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NewMapRegion {
    pub id: String,
    pub label: String,
    pub size_bytes: i64,
    pub version: String,
    pub source_url: String,
    pub checksum_sha256: String,
}

#[derive(Debug, Clone)]
pub struct RegionDownloadSource {
    pub source_url: String,
    pub size_bytes: i64,
    pub checksum_sha256: String,
    pub cancel_requested: bool,
}

#[derive(sqlx::FromRow)]
struct RegionRow {
    id: String,
    label: String,
    file_path: String,
    size_bytes: i64,
    version: String,
    downloaded_at: Option<DateTime<Utc>>,
    status: String,
    source_url: String,
    checksum_sha256: String,
    downloaded_bytes: i64,
    last_error: Option<String>,
}

impl RegionRow {
    fn into_domain(self) -> Result<MapRegion, DbError> {
        Ok(MapRegion {
            id: self.id,
            label: self.label,
            file_path: self.file_path,
            size_bytes: self.size_bytes,
            version: self.version,
            downloaded_at: self.downloaded_at,
            status: RegionStatus::parse(&self.status)?,
            source_url: self.source_url,
            checksum_sha256: self.checksum_sha256,
            downloaded_bytes: self.downloaded_bytes,
            last_error: self.last_error,
        })
    }
}

const COLUMNS: &str = "id, label, file_path, size_bytes, version, downloaded_at, status, \
                       source_url, checksum_sha256, downloaded_bytes, last_error";

pub struct RegionRepo<'a> {
    db: &'a Db,
}

impl<'a> RegionRepo<'a> {
    #[must_use]
    pub const fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// Registra un download globale. Solo un admin può avviarlo e una riga già
    /// in download non viene sostituita.
    ///
    /// # Errors
    /// `Forbidden` per non-admin, `Conflict` se la regione è già in download.
    pub async fn begin_download(
        &self,
        ctx: &AuthContext,
        region: NewMapRegion,
    ) -> Result<MapRegion, DbError> {
        if !ctx.is_admin() {
            return Err(DbError::Forbidden);
        }
        let file_path = format!("maps/{}.pmtiles", region.id);
        let row: Option<RegionRow> = sqlx::query_as(&format!(
            "INSERT INTO map_regions \
                 (id, label, file_path, size_bytes, version, status, source_url, \
                  checksum_sha256, downloaded_bytes, last_error, downloaded_at, \
                  cancel_requested) \
             VALUES ($1, $2, $3, $4, $5, 'downloading', $6, $7, 0, NULL, NULL, false) \
             ON CONFLICT (id) DO UPDATE SET \
                 label = EXCLUDED.label, file_path = EXCLUDED.file_path, \
                 size_bytes = EXCLUDED.size_bytes, version = EXCLUDED.version, \
                 status = 'downloading', source_url = EXCLUDED.source_url, \
                 checksum_sha256 = EXCLUDED.checksum_sha256, \
                 downloaded_bytes = 0, last_error = NULL, downloaded_at = NULL, \
                 cancel_requested = false \
             WHERE map_regions.status <> 'downloading' \
             RETURNING {COLUMNS}"
        ))
        .bind(&region.id)
        .bind(&region.label)
        .bind(file_path)
        .bind(region.size_bytes)
        .bind(&region.version)
        .bind(&region.source_url)
        .bind(&region.checksum_sha256)
        .fetch_optional(self.db.pool())
        .await?;
        row.ok_or_else(|| DbError::Conflict("region download already in progress".to_owned()))?
            .into_domain()
    }

    /// Elenca lo stato globale delle regioni a un utente autenticato.
    ///
    /// # Errors
    /// `Forbidden` senza utente; `Connection` su errore DB.
    pub async fn list(&self, ctx: &AuthContext) -> Result<Vec<MapRegion>, DbError> {
        if ctx.user_id().is_none() {
            return Err(DbError::Forbidden);
        }
        let rows: Vec<RegionRow> = sqlx::query_as(&format!(
            "SELECT {COLUMNS} FROM map_regions ORDER BY label, id"
        ))
        .fetch_all(self.db.pool())
        .await?;
        rows.into_iter().map(RegionRow::into_domain).collect()
    }

    /// Legge una regione globale per id.
    ///
    /// # Errors
    /// `Forbidden` senza utente; `NotFound` se assente.
    pub async fn find(&self, ctx: &AuthContext, id: &str) -> Result<MapRegion, DbError> {
        if ctx.user_id().is_none() {
            return Err(DbError::Forbidden);
        }
        let row: Option<RegionRow> =
            sqlx::query_as(&format!("SELECT {COLUMNS} FROM map_regions WHERE id = $1"))
                .bind(id)
                .fetch_optional(self.db.pool())
                .await?;
        row.ok_or(DbError::NotFound)?.into_domain()
    }

    /// Come [`Self::find`], ma una regione non disponibile equivale ad assente
    /// per il tile reader.
    ///
    /// # Errors
    /// `Forbidden`, `NotFound` o `Connection`.
    pub async fn find_available(&self, ctx: &AuthContext, id: &str) -> Result<MapRegion, DbError> {
        let region = self.find(ctx, id).await?;
        if region.status == RegionStatus::Available {
            Ok(region)
        } else {
            Err(DbError::NotFound)
        }
    }

    /// Richiede l'interruzione lasciando la regione ritentabile finché i file
    /// non sono stati rimossi.
    ///
    /// # Errors
    /// `Forbidden` per non-admin, `NotFound` se assente, `Conflict` se non è
    /// in download.
    pub async fn request_cancel(&self, ctx: &AuthContext, id: &str) -> Result<(), DbError> {
        if !ctx.is_admin() {
            return Err(DbError::Forbidden);
        }
        let result = sqlx::query(
            "UPDATE map_regions \
                SET cancel_requested = true \
              WHERE id = $1 AND status = 'downloading'",
        )
        .bind(id)
        .execute(self.db.pool())
        .await?;
        if result.rows_affected() == 1 {
            return Ok(());
        }
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM map_regions WHERE id = $1)")
                .bind(id)
                .fetch_one(self.db.pool())
                .await?;
        if exists {
            Err(DbError::Conflict("region is not downloading".to_owned()))
        } else {
            Err(DbError::NotFound)
        }
    }

    /// Completa una cancellazione solo dopo il cleanup dei file.
    ///
    /// Non prende `AuthContext`: metodo interno della pipeline.
    ///
    /// # Errors
    /// `Connection` su errore DB.
    pub async fn finish_cancel(&self, id: &str) -> Result<bool, DbError> {
        let result = sqlx::query(
            "UPDATE map_regions \
                SET status = 'error', downloaded_bytes = 0, \
                    last_error = 'Download cancelled', cancel_requested = false \
              WHERE id = $1 AND status = 'downloading' AND cancel_requested",
        )
        .bind(id)
        .execute(self.db.pool())
        .await?;
        Ok(result.rows_affected() == 1)
    }

    /// Elimina la riga globale. I file vengono rimossi dal livello API, che
    /// possiede `data_dir`.
    ///
    /// # Errors
    /// `Forbidden` per non-admin, `NotFound` se assente.
    pub async fn delete(&self, ctx: &AuthContext, id: &str) -> Result<(), DbError> {
        if !ctx.is_admin() {
            return Err(DbError::Forbidden);
        }
        let result = sqlx::query("DELETE FROM map_regions WHERE id = $1")
            .bind(id)
            .execute(self.db.pool())
            .await?;
        if result.rows_affected() == 0 {
            return Err(DbError::NotFound);
        }
        Ok(())
    }

    /// Dati di download letti dal worker. Non prende `AuthContext` perché non
    /// espone dati utente ed è chiamato solo dalla pipeline.
    ///
    /// # Errors
    /// `Connection` su errore DB.
    pub async fn source_for_download(
        &self,
        id: &str,
    ) -> Result<Option<RegionDownloadSource>, DbError> {
        let row: Option<(String, i64, String, bool)> = sqlx::query_as(
            "SELECT source_url, size_bytes, checksum_sha256, cancel_requested \
               FROM map_regions WHERE id = $1 AND status = 'downloading'",
        )
        .bind(id)
        .fetch_optional(self.db.pool())
        .await?;
        Ok(row.map(
            |(source_url, size_bytes, checksum_sha256, cancel_requested)| RegionDownloadSource {
                source_url,
                size_bytes,
                checksum_sha256,
                cancel_requested,
            },
        ))
    }

    /// Persiste l'offset osservato dal worker.
    ///
    /// Non prende `AuthContext`: metodo interno della pipeline.
    ///
    /// # Errors
    /// `Connection` su errore DB.
    pub async fn record_progress(&self, id: &str, bytes: i64) -> Result<bool, DbError> {
        let result = sqlx::query(
            "UPDATE map_regions SET downloaded_bytes = $2 \
              WHERE id = $1 AND status = 'downloading' AND NOT cancel_requested",
        )
        .bind(id)
        .bind(bytes)
        .execute(self.db.pool())
        .await?;
        Ok(result.rows_affected() == 1)
    }

    /// Marca disponibile solo se nessuno ha cancellato nel frattempo.
    ///
    /// Non prende `AuthContext`: metodo interno della pipeline.
    ///
    /// # Errors
    /// `Connection` su errore DB.
    pub async fn mark_available(&self, id: &str) -> Result<bool, DbError> {
        let result = sqlx::query(
            "UPDATE map_regions \
                SET status = 'available', downloaded_bytes = size_bytes, \
                    downloaded_at = now(), last_error = NULL, cancel_requested = false \
              WHERE id = $1 AND status = 'downloading' AND NOT cancel_requested",
        )
        .bind(id)
        .execute(self.db.pool())
        .await?;
        Ok(result.rows_affected() == 1)
    }

    /// Marca un errore leggibile e azzera l'offset perché il parziale viene
    /// rimosso.
    ///
    /// Non prende `AuthContext`: metodo interno della pipeline.
    ///
    /// # Errors
    /// `Connection` su errore DB.
    pub async fn mark_error(&self, id: &str, error: &str) -> Result<bool, DbError> {
        let result = sqlx::query(
            "UPDATE map_regions \
                SET status = 'error', downloaded_bytes = 0, last_error = $2, \
                    cancel_requested = false \
              WHERE id = $1 AND status = 'downloading'",
        )
        .bind(id)
        .bind(error)
        .execute(self.db.pool())
        .await?;
        Ok(result.rows_affected() == 1)
    }
}
