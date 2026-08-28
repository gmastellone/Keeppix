use chrono::{DateTime, Utc};
use keeppix_domain::AuthContext;
use uuid::Uuid;

use crate::{Db, DbError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegionStatus {
    Available,
    Downloading,
    Error,
}

impl RegionStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Available => "available",
            Self::Downloading => "downloading",
            Self::Error => "error",
        }
    }

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
    pub download_generation: Uuid,
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
    pub file_path: String,
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
    download_generation: Uuid,
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
            download_generation: self.download_generation,
        })
    }
}

const COLUMNS: &str = "id, label, file_path, size_bytes, version, downloaded_at, status, \
                       source_url, checksum_sha256, downloaded_bytes, last_error, \
                       download_generation";

pub struct RegionRepo<'a> {
    db: &'a Db,
}

impl<'a> RegionRepo<'a> {
    #[must_use]
    pub const fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// Registers a global download. Only an admin can start one, and a
    /// row already downloading is not replaced.
    ///
    /// # Errors
    /// `Forbidden` for non-admins, `Conflict` if the region is already downloading.
    pub async fn begin_download(
        &self,
        ctx: &AuthContext,
        region: NewMapRegion,
    ) -> Result<MapRegion, DbError> {
        if !ctx.is_admin() {
            return Err(DbError::Forbidden);
        }
        let download_generation = Uuid::now_v7();
        let file_path = format!("maps/{}-{download_generation}.pmtiles", region.id);
        let row: Option<RegionRow> = sqlx::query_as(&format!(
            "INSERT INTO map_regions \
                 (id, label, file_path, size_bytes, version, status, source_url, \
                  checksum_sha256, downloaded_bytes, last_error, downloaded_at, \
                  cancel_requested, download_generation) \
             VALUES ($1, $2, $3, $4, $5, 'downloading', $6, $7, 0, NULL, NULL, false, $8) \
             ON CONFLICT (id) DO UPDATE SET \
                 label = EXCLUDED.label, file_path = EXCLUDED.file_path, \
                 size_bytes = EXCLUDED.size_bytes, version = EXCLUDED.version, \
                 status = 'downloading', source_url = EXCLUDED.source_url, \
                 checksum_sha256 = EXCLUDED.checksum_sha256, \
                 downloaded_bytes = 0, last_error = NULL, downloaded_at = NULL, \
                 cancel_requested = false, download_generation = EXCLUDED.download_generation \
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
        .bind(download_generation)
        .fetch_optional(self.db.pool())
        .await?;
        row.ok_or_else(|| DbError::Conflict("region download already in progress".to_owned()))?
            .into_domain()
    }

    /// Lists the global status of regions for an authenticated user.
    ///
    /// # Errors
    /// `Forbidden` without a user; `Connection` on DB error.
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

    /// Inventory for backup `maps.json`. Pipeline-only — no `AuthContext`
    /// (same class as `SessionRepo::purge_expired`: system maintenance, not
    /// per-user data). Map regions are instance-global.
    ///
    /// # Errors
    /// `Connection` on database failure.
    pub async fn list_all_for_jobs(&self) -> Result<Vec<MapRegion>, DbError> {
        let rows: Vec<RegionRow> = sqlx::query_as(&format!(
            "SELECT {COLUMNS} FROM map_regions ORDER BY label, id"
        ))
        .fetch_all(self.db.pool())
        .await?;
        rows.into_iter().map(RegionRow::into_domain).collect()
    }

    /// Reads a global region by id.
    ///
    /// # Errors
    /// `Forbidden` without a user; `NotFound` if absent.
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

    /// Same as [`Self::find`], but a region that is not available is
    /// treated as absent by the tile reader.
    ///
    /// # Errors
    /// `Forbidden`, `NotFound`, or `Connection`.
    pub async fn find_available(&self, ctx: &AuthContext, id: &str) -> Result<MapRegion, DbError> {
        let region = self.find(ctx, id).await?;
        if region.status == RegionStatus::Available {
            Ok(region)
        } else {
            Err(DbError::NotFound)
        }
    }

    /// Requests cancellation, leaving the region retryable until the
    /// files have been removed.
    ///
    /// # Errors
    /// `Forbidden` for non-admins, `NotFound` if absent, `Conflict` if it
    /// is not downloading.
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

    /// Completes a cancellation only after the file cleanup.
    ///
    /// Does not take an `AuthContext`: internal pipeline method.
    ///
    /// # Errors
    /// `Connection` on DB error.
    pub async fn finish_cancel(&self, id: &str, generation: Uuid) -> Result<bool, DbError> {
        let result = sqlx::query(
            "UPDATE map_regions \
                SET status = 'error', downloaded_bytes = 0, \
                    last_error = 'Download cancelled', cancel_requested = false \
              WHERE id = $1 AND download_generation = $2 \
                AND status = 'downloading' AND cancel_requested",
        )
        .bind(id)
        .bind(generation)
        .execute(self.db.pool())
        .await?;
        Ok(result.rows_affected() == 1)
    }

    /// Deletes the global row. The files are removed by the API layer,
    /// which owns `data_dir`.
    ///
    /// # Errors
    /// `Forbidden` for non-admins, `NotFound` if absent.
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

    /// Download data read by the worker. Does not take an `AuthContext`
    /// because it does not expose user data and is only called by the
    /// pipeline.
    ///
    /// # Errors
    /// `Connection` on DB error.
    pub async fn source_for_download(
        &self,
        id: &str,
        generation: Uuid,
    ) -> Result<Option<RegionDownloadSource>, DbError> {
        let row: Option<(String, i64, String, bool, String)> = sqlx::query_as(
            "SELECT source_url, size_bytes, checksum_sha256, cancel_requested, file_path \
               FROM map_regions \
              WHERE id = $1 AND download_generation = $2 AND status = 'downloading'",
        )
        .bind(id)
        .bind(generation)
        .fetch_optional(self.db.pool())
        .await?;
        Ok(row.map(
            |(source_url, size_bytes, checksum_sha256, cancel_requested, file_path)| {
                RegionDownloadSource {
                    source_url,
                    size_bytes,
                    checksum_sha256,
                    cancel_requested,
                    file_path,
                }
            },
        ))
    }

    /// Persists the offset observed by the worker.
    ///
    /// Does not take an `AuthContext`: internal pipeline method.
    ///
    /// # Errors
    /// `Connection` on DB error.
    pub async fn record_progress(
        &self,
        id: &str,
        generation: Uuid,
        bytes: i64,
    ) -> Result<bool, DbError> {
        let result = sqlx::query(
            "UPDATE map_regions SET downloaded_bytes = $3 \
              WHERE id = $1 AND download_generation = $2 \
                AND status = 'downloading' AND NOT cancel_requested",
        )
        .bind(id)
        .bind(generation)
        .bind(bytes)
        .execute(self.db.pool())
        .await?;
        Ok(result.rows_affected() == 1)
    }

    /// Marks it available only if nobody cancelled in the meantime.
    ///
    /// Does not take an `AuthContext`: internal pipeline method.
    ///
    /// # Errors
    /// `Connection` on DB error.
    pub async fn mark_available(&self, id: &str, generation: Uuid) -> Result<bool, DbError> {
        let result = sqlx::query(
            "UPDATE map_regions \
                SET status = 'available', downloaded_bytes = size_bytes, \
                    downloaded_at = now(), last_error = NULL, cancel_requested = false \
              WHERE id = $1 AND download_generation = $2 \
                AND status = 'downloading' AND NOT cancel_requested",
        )
        .bind(id)
        .bind(generation)
        .execute(self.db.pool())
        .await?;
        Ok(result.rows_affected() == 1)
    }

    /// Marks a readable error and resets the offset because the partial
    /// download is removed.
    ///
    /// Does not take an `AuthContext`: internal pipeline method.
    ///
    /// # Errors
    /// `Connection` on DB error.
    pub async fn mark_error(
        &self,
        id: &str,
        generation: Uuid,
        error: &str,
    ) -> Result<bool, DbError> {
        let result = sqlx::query(
            "UPDATE map_regions \
                SET status = 'error', downloaded_bytes = 0, last_error = $3, \
                    cancel_requested = false \
              WHERE id = $1 AND download_generation = $2 \
                AND status = 'downloading' AND NOT cancel_requested",
        )
        .bind(id)
        .bind(generation)
        .bind(error)
        .execute(self.db.pool())
        .await?;
        Ok(result.rows_affected() == 1)
    }
}
