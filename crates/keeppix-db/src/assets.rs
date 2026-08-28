use std::collections::HashMap;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use keeppix_domain::{
    Asset, AssetId, AssetKind, AssetName, AssetStatus, AuthContext, CollisionOutcome, ExifData,
    FolderId, GeoPoint, LibraryId, LocationSource, NewAsset,
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

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_raw(
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
    ) -> Self {
        Self {
            id,
            folder_id,
            filename,
            content_hash,
            size_bytes,
            mtime,
            inode,
            kind,
            status,
            taken_at_utc,
            width,
            height,
            thumbhash,
            created_at,
        }
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

/// Outcome of a direct insertion (`WebDAV PUT`): same shape as
/// `crate::uploads::FinalizeOutcome`, without the session concept — the
/// caller already has the whole file in a temp location.
pub struct DirectPutOutcome {
    pub asset_id: AssetId,
    /// Final name on disk — may differ from the requested one if a
    /// collision was resolved with a suffix.
    pub filename: String,
    pub collision: CollisionOutcome,
}

/// Full EXIF of an asset ("SHOT" section of the info panel) — unlike
/// [`AssetRepo::camera_models_among`], which only exposes `camera_model`
/// for the summary dimension, this is the full detail for a single asset
/// opened in the lightbox.
#[derive(Debug, Clone, PartialEq)]
pub struct AssetExifDetail {
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub lens: Option<String>,
    pub iso: Option<i32>,
    pub f_number: Option<f32>,
    pub exposure: Option<String>,
    pub focal_length: Option<f32>,
}

#[derive(sqlx::FromRow)]
struct AssetExifDetailRow {
    camera_make: Option<String>,
    camera_model: Option<String>,
    lens: Option<String>,
    iso: Option<i32>,
    f_number: Option<f32>,
    exposure: Option<String>,
    focal_length: Option<f32>,
}

impl AssetExifDetailRow {
    fn into_domain(self) -> AssetExifDetail {
        AssetExifDetail {
            camera_make: self.camera_make,
            camera_model: self.camera_model,
            lens: self.lens,
            iso: self.iso,
            f_number: self.f_number,
            exposure: self.exposure,
            focal_length: self.focal_length,
        }
    }
}

impl<'a> AssetRepo<'a> {
    #[must_use]
    pub const fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// Creates the asset by moving `temp_path` into the final folder,
    /// resolving any name collision exactly like
    /// `UploadSessionRepo::finalize`: same name and same hash is a
    /// duplicate, skipped; same name and a different hash gets a suffix —
    /// **never** a silent overwrite.
    ///
    /// The caller must already have checked editor permission on the
    /// folder (`FolderRepo::assert_editor`): this method does not repeat
    /// it, so as not to pay for the same query twice on the same HTTP path
    /// (`dav::write::put`).
    ///
    /// As in `finalize`, the `rename()` happens before the commit: if the
    /// commit later fails, the file is left in the right place without an
    /// `assets` row, and the next library scan discovers it as a plain
    /// file — never the opposite risk of a row pointing at a nonexistent
    /// file.
    ///
    /// # Errors
    /// Same as `FolderRepo::absolute_path` for folder visibility. `Io` if
    /// the final `rename()` fails — the temp file stays put, no row is
    /// touched.
    #[allow(clippy::too_many_arguments)]
    pub async fn ingest_direct(
        &self,
        ctx: &AuthContext,
        folder_id: FolderId,
        temp_path: &Path,
        desired_name: &str,
        content_hash: [u8; 32],
        size_bytes: i64,
        mtime: DateTime<Utc>,
        kind: AssetKind,
    ) -> Result<DirectPutOutcome, DbError> {
        let folder_dir = FolderRepo::new(self.db)
            .absolute_path(ctx, folder_id)
            .await?;

        let mut tx = self.db.pool().begin().await?;
        let existing: Option<(uuid::Uuid, Option<Vec<u8>>)> = sqlx::query_as(
            "SELECT id, content_hash FROM assets WHERE folder_id = $1 AND filename = $2",
        )
        .bind(folder_id.as_uuid())
        .bind(desired_name)
        .fetch_optional(&mut *tx)
        .await?;

        if let Some((existing_id, existing_hash)) = &existing
            && existing_hash.as_deref() == Some(content_hash.as_slice())
        {
            // Same name, same hash: exact duplicate. The target folder is
            // never touched — only the temp file is removed, before the
            // commit (which has nothing to confirm here, but stays
            // symmetric with `finalize`).
            crate::uploads::remove_file_tolerant(temp_path)?;
            tx.commit().await?;
            return Ok(DirectPutOutcome {
                asset_id: AssetId::from_uuid(*existing_id),
                filename: desired_name.to_owned(),
                collision: CollisionOutcome::SkippedDuplicate {
                    existing_asset_id: AssetId::from_uuid(*existing_id),
                },
            });
        }

        let (final_name, outcome) = if existing.is_some() {
            let taken: Vec<String> =
                sqlx::query_scalar("SELECT filename FROM assets WHERE folder_id = $1")
                    .bind(folder_id.as_uuid())
                    .fetch_all(&mut *tx)
                    .await?;
            let unique = crate::uploads::unique_suffixed_name(desired_name, &taken);
            (unique.clone(), CollisionOutcome::RenamedTo(unique))
        } else {
            (desired_name.to_owned(), CollisionOutcome::Created)
        };

        let target_path = folder_dir.join(&final_name);
        std::fs::rename(temp_path, &target_path).map_err(|e| {
            DbError::Io(format!(
                "moving {} to {}: {e}",
                temp_path.display(),
                target_path.display()
            ))
        })?;

        let asset_id = AssetId::new();
        sqlx::query(
            "INSERT INTO assets (id, folder_id, filename, content_hash, size_bytes, mtime, kind) \
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(asset_id.as_uuid())
        .bind(folder_id.as_uuid())
        .bind(&final_name)
        .bind(content_hash.as_slice())
        .bind(size_bytes)
        .bind(mtime)
        .bind(kind_str(kind))
        .execute(&mut *tx)
        .await
        .map_err(crate::uploads::map_unique_violation)?;

        tx.commit().await?;

        Ok(DirectPutOutcome {
            asset_id,
            filename: final_name,
            collision: outcome,
        })
    }

    /// Inserts the file found by the walker, or updates size/mtime if it
    /// already exists.
    ///
    /// Returns `None` when mtime and size are identical to what is already
    /// known: the caller must not re-queue metadata/hashing. `kind` is
    /// only reset if the file has actually changed — otherwise a rescan
    /// would wipe out `detect_kind`.
    ///
    /// Does not take an `AuthContext` because the scanner calls this.
    ///
    /// # Errors
    /// `Connection` if the insert fails; `Corrupted` if the returned row
    /// fails domain validation.
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

    /// Like [`Self::upsert_discovered`], but for an entire batch in a
    /// single statement: one network round trip instead of one per file.
    /// The `assets_change_log` trigger (`AFTER INSERT OR UPDATE ... FOR
    /// EACH ROW`) still writes one `change_log` row per asset — the
    /// per-entity granularity required for mobile sync is not lost — but
    /// it happens inside this single statement, without one round trip
    /// per row.
    ///
    /// Returns only the assets that actually changed (same `mtime`/
    /// `size_bytes` filter as [`Self::upsert_discovered`]): the caller
    /// must not re-queue metadata/hashing for an unchanged file.
    ///
    /// Does not take an `AuthContext` because the scanner calls this.
    ///
    /// # Errors
    /// `Connection` if the insert fails; `Corrupted` if a returned row
    /// fails domain validation.
    pub async fn batch_upsert_discovered(&self, items: &[NewAsset]) -> Result<Vec<Asset>, DbError> {
        if items.is_empty() {
            return Ok(Vec::new());
        }
        let ids: Vec<uuid::Uuid> = items.iter().map(|_| AssetId::new().as_uuid()).collect();
        let folder_ids: Vec<uuid::Uuid> = items.iter().map(|i| i.folder_id.as_uuid()).collect();
        let filenames: Vec<&str> = items.iter().map(|i| i.filename.as_str()).collect();
        let sizes: Vec<i64> = items.iter().map(|i| i.size_bytes).collect();
        let mtimes: Vec<DateTime<Utc>> = items.iter().map(|i| i.mtime).collect();
        let inodes: Vec<Option<i64>> = items.iter().map(|i| i.inode).collect();
        let kinds: Vec<&str> = items.iter().map(|i| kind_str(i.kind)).collect();

        let rows: Vec<AssetRow> = sqlx::query_as(&format!(
            "INSERT INTO assets (id, folder_id, filename, size_bytes, mtime, inode, kind) \
             SELECT * FROM UNNEST( \
                 $1::uuid[], $2::uuid[], $3::text[], $4::bigint[], $5::timestamptz[], \
                 $6::bigint[], $7::text[]) \
                 AS t(id, folder_id, filename, size_bytes, mtime, inode, kind) \
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
        .bind(ids)
        .bind(folder_ids)
        .bind(filenames)
        .bind(sizes)
        .bind(mtimes)
        .bind(inodes)
        .bind(kinds)
        .fetch_all(self.db.pool())
        .await?;

        rows.into_iter().map(AssetRow::into_domain).collect()
    }

    /// Does not take an `AuthContext`: the metadata pipeline calls this.
    ///
    /// # Errors
    /// `Connection` if the update fails.
    pub async fn set_kind(&self, id: AssetId, kind: AssetKind) -> Result<(), DbError> {
        sqlx::query("UPDATE assets SET kind = $2, updated_at = now() WHERE id = $1")
            .bind(id.as_uuid())
            .bind(kind_str(kind))
            .execute(self.db.pool())
            .await?;
        Ok(())
    }

    /// Does not take an `AuthContext`: the hashing pipeline calls this.
    ///
    /// # Errors
    /// `Connection` if the update fails.
    pub async fn set_hash(&self, id: AssetId, hash: [u8; 32]) -> Result<(), DbError> {
        sqlx::query("UPDATE assets SET content_hash = $2, updated_at = now() WHERE id = $1")
            .bind(id.as_uuid())
            .bind(hash.as_slice())
            .execute(self.db.pool())
            .await?;
        Ok(())
    }

    /// Does not take an `AuthContext`: the indexing pipeline calls this.
    ///
    /// # Errors
    /// `Connection` if the update fails.
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

    /// Does not take an `AuthContext`: the pipeline calls this.
    ///
    /// # Errors
    /// `Connection` if the update fails.
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

    /// Does not take an `AuthContext`: the scanner calls this when the file disappears.
    ///
    /// # Errors
    /// `Connection` if the update fails.
    pub async fn mark_offline(&self, id: AssetId) -> Result<(), DbError> {
        sqlx::query("UPDATE assets SET status = 'offline', updated_at = now() WHERE id = $1")
            .bind(id.as_uuid())
            .execute(self.db.pool())
            .await?;
        Ok(())
    }

    /// # Errors
    /// `Forbidden` if the caller cannot see the asset — including when the
    /// id does not exist. `NotFound` only for an admin requesting a
    /// nonexistent id.
    pub async fn find_by_id(&self, ctx: &AuthContext, id: AssetId) -> Result<Asset, DbError> {
        let scope = VisibilityScope::resolve(self.db, ctx).await?;
        let filter = scope.filter("f.path", "f.library_id", "a.id", 2);
        let row: Option<AssetRow> = sqlx::query_as(&format!(
            "SELECT {A_COLUMNS} FROM assets a \
             JOIN folders f ON f.id = a.folder_id \
             WHERE a.id = $1 AND {}",
            filter.sql()
        ))
        .bind(id.as_uuid())
        .bind(filter.bind())
        .bind(filter.holes())
        .bind(filter.assets())
        .fetch_optional(self.db.pool())
        .await?;

        match row {
            Some(row) => row.into_domain(),
            None if ctx.is_admin() => Err(DbError::NotFound),
            None => Err(DbError::Forbidden),
        }
    }

    /// Clears `uploaded_by_guest` after the owner approves the file.
    ///
    /// # Errors
    /// Same as `find_by_id`. `Connection` if the update fails.
    pub async fn clear_guest_flag(&self, ctx: &AuthContext, id: AssetId) -> Result<(), DbError> {
        self.find_by_id(ctx, id).await?;
        sqlx::query(
            "UPDATE assets SET uploaded_by_guest = false, updated_at = now() WHERE id = $1",
        )
        .bind(id.as_uuid())
        .execute(self.db.pool())
        .await?;
        Ok(())
    }

    /// # Errors
    /// Same as `FolderRepo::find_by_id` on the folder, then its assets.
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
    /// `Connection` if the query fails.
    pub async fn find_by_hash(
        &self,
        ctx: &AuthContext,
        hash: &[u8; 32],
    ) -> Result<Vec<Asset>, DbError> {
        let scope = VisibilityScope::resolve(self.db, ctx).await?;
        let filter = scope.filter("f.path", "f.library_id", "a.id", 2);
        let rows: Vec<AssetRow> = sqlx::query_as(&format!(
            "SELECT {A_COLUMNS} FROM assets a \
             JOIN folders f ON f.id = a.folder_id \
             WHERE a.content_hash = $1 AND {} \
             ORDER BY a.filename",
            filter.sql()
        ))
        .bind(hash.as_slice())
        .bind(filter.bind())
        .bind(filter.holes())
        .bind(filter.assets())
        .fetch_all(self.db.pool())
        .await?;

        rows.into_iter().map(AssetRow::into_domain).collect()
    }

    /// Which of `hashes` already exist as the `content_hash` of an asset
    /// visible to the caller — the upload pre-check: "I already have
    /// these, only upload the rest." Filtered by visibility like
    /// [`Self::find_by_hash`], so it does not become an oracle over hashes
    /// the caller could not otherwise see.
    ///
    /// # Errors
    /// `Connection` if the query fails.
    pub async fn known_hashes(
        &self,
        ctx: &AuthContext,
        hashes: &[[u8; 32]],
    ) -> Result<std::collections::HashSet<[u8; 32]>, DbError> {
        if hashes.is_empty() {
            return Ok(std::collections::HashSet::new());
        }
        let scope = VisibilityScope::resolve(self.db, ctx).await?;
        let filter = scope.filter("f.path", "f.library_id", "a.id", 2);
        let wanted: Vec<Vec<u8>> = hashes.iter().map(|h| h.to_vec()).collect();
        let rows: Vec<(Vec<u8>,)> = sqlx::query_as(&format!(
            "SELECT DISTINCT a.content_hash FROM assets a \
             JOIN folders f ON f.id = a.folder_id \
             WHERE a.content_hash = ANY($1) AND {}",
            filter.sql()
        ))
        .bind(&wanted)
        .bind(filter.bind())
        .bind(filter.holes())
        .bind(filter.assets())
        .fetch_all(self.db.pool())
        .await?;
        Ok(rows
            .into_iter()
            .filter_map(|(h,)| <[u8; 32]>::try_from(h.as_slice()).ok())
            .collect())
    }

    /// # Errors
    /// `Connection` if the query fails.
    pub async fn count_by_status(
        &self,
        ctx: &AuthContext,
        status: AssetStatus,
    ) -> Result<i64, DbError> {
        let scope = VisibilityScope::resolve(self.db, ctx).await?;
        let filter = scope.filter("f.path", "f.library_id", "a.id", 2);
        let n: i64 = sqlx::query_scalar(&format!(
            "SELECT count(*) FROM assets a \
             JOIN folders f ON f.id = a.folder_id \
             WHERE a.status = $1 AND {}",
            filter.sql()
        ))
        .bind(status_str(status))
        .bind(filter.bind())
        .bind(filter.holes())
        .bind(filter.assets())
        .fetch_one(self.db.pool())
        .await?;
        Ok(n)
    }

    /// Does not take an `AuthContext`: the scanner calls this on the claimed job.
    ///
    /// # Errors
    /// `NotFound` if the id does not exist.
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

    /// Count within the library, for mass-disappearance thresholds.
    ///
    /// Does not take an `AuthContext`: the scanner calls this.
    ///
    /// # Errors
    /// `Connection` if the query fails.
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

    /// Original metadata is immutable: a second insert does not overwrite it.
    ///
    /// Does not take an `AuthContext`: the pipeline calls this.
    ///
    /// # Errors
    /// `Connection` if the insert fails.
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

    /// Saves EXIF coordinates without stomping on a manual correction.
    ///
    /// Does not take an `AuthContext`: the metadata pipeline calls this.
    ///
    /// # Errors
    /// `Connection` if the update fails.
    pub async fn set_exif_location(
        &self,
        asset_id: AssetId,
        point: GeoPoint,
    ) -> Result<(), DbError> {
        sqlx::query(
            "UPDATE assets \
             SET location = ST_SetSRID(ST_MakePoint($2, $3), 4326)::geography, \
                 location_source = $4, \
                 updated_at = now() \
             WHERE id = $1 \
               AND (location_source IS NULL OR location_source = $4)",
        )
        .bind(asset_id.as_uuid())
        .bind(point.lon)
        .bind(point.lat)
        .bind(LocationSource::Exif.as_str())
        .execute(self.db.pool())
        .await?;
        Ok(())
    }

    /// Does not take an `AuthContext`: the derivatives pipeline calls this.
    ///
    /// # Errors
    /// `Connection` if the update fails.
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

    /// Copies an already-known thumbhash onto assets with the same
    /// `content_hash` that do not yet have one. Called by `DeriveRaw` on
    /// its idempotent branch: the derived file exists, but a duplicate
    /// hashed after the first derivation would otherwise be left without a
    /// placeholder.
    ///
    /// Does not take an `AuthContext`: this is the derivatives pipeline,
    /// like [`Self::set_thumbhash_for_hash`].
    ///
    /// # Errors
    /// `Connection` if the update fails.
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

    /// Does not take an `AuthContext`: the derivatives pipeline calls this.
    ///
    /// # Errors
    /// `Connection` if the query fails.
    pub async fn ids_with_hash(&self, hash: &[u8; 32]) -> Result<Vec<AssetId>, DbError> {
        let rows: Vec<uuid::Uuid> =
            sqlx::query_scalar("SELECT id FROM assets WHERE content_hash = $1")
                .bind(hash.as_slice())
                .fetch_all(self.db.pool())
                .await?;
        Ok(rows.into_iter().map(AssetId::from_uuid).collect())
    }

    /// Hashes of assets in `error` to retry. Called by the maintenance
    /// job, not a user: no `AuthContext`.
    ///
    /// # Errors
    /// `Connection` if the query fails.
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

    /// Returns the ids among `ids` that the caller can see, in the same
    /// order as the request. Missing ones (nonexistent or out of scope) do
    /// not appear: partial-success operations put them in `failed`.
    ///
    /// # Errors
    /// `Connection` if the query fails.
    pub async fn filter_visible(
        &self,
        ctx: &AuthContext,
        ids: &[AssetId],
    ) -> Result<Vec<AssetId>, DbError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let uuids: Vec<uuid::Uuid> = ids.iter().map(AssetId::as_uuid).collect();
        let scope = VisibilityScope::resolve(self.db, ctx).await?;
        let filter = scope.filter("f.path", "f.library_id", "a.id", 2);
        let visible: Vec<uuid::Uuid> = sqlx::query_scalar(&format!(
            "SELECT a.id FROM assets a \
             JOIN folders f ON f.id = a.folder_id \
             WHERE a.id = ANY($1) AND {}",
            filter.sql()
        ))
        .bind(&uuids)
        .bind(filter.bind())
        .bind(filter.holes())
        .bind(filter.assets())
        .fetch_all(self.db.pool())
        .await?;

        let visible: std::collections::HashSet<uuid::Uuid> = visible.into_iter().collect();
        Ok(ids
            .iter()
            .copied()
            .filter(|id| visible.contains(&id.as_uuid()))
            .collect())
    }

    /// Checks in a single query that the caller can see **all** the given
    /// ids. Used by operations that must stay all-or-nothing. For
    /// partial-success batches, prefer [`Self::filter_visible`].
    ///
    /// # Errors
    /// `Forbidden` if even a single id is not visible — including the case
    /// where it does not exist at all. `NotFound` only for an admin when
    /// an id does not exist at all.
    pub async fn assert_visible(&self, ctx: &AuthContext, ids: &[AssetId]) -> Result<(), DbError> {
        if ids.is_empty() {
            return Ok(());
        }
        let visible = self.filter_visible(ctx, ids).await?;
        let visible: std::collections::HashSet<uuid::Uuid> =
            visible.into_iter().map(|id| id.as_uuid()).collect();
        let distinct: std::collections::HashSet<uuid::Uuid> =
            ids.iter().map(AssetId::as_uuid).collect();

        if visible.len() == distinct.len() {
            Ok(())
        } else if ctx.is_admin() {
            Err(DbError::NotFound)
        } else {
            Err(DbError::Forbidden)
        }
    }

    /// Moves an asset safely: identity is preserved. `asset_flags`/
    /// `asset_overrides`/`asset_tags`/`faces` are foreign keys on
    /// `asset_id`, never on `folder_id`/`filename` — an `UPDATE` on the
    /// **existing** row (same id) keeps them linked without copying
    /// anything, unlike `moves.rs::after_hash` (`crates/keeppix-jobs`)
    /// which creates a new row and loses most of them because it is an
    /// *after-the-fact* recognition, not a direct move.
    ///
    /// **Deliberate ordering: the physical file moves first, the row
    /// after — the opposite of the convention already used in
    /// `TrashRepo::choose` (`crates/keeppix-db/src/trash.rs`,
    /// row-then-`rename()`).** This is not an inconsistency between the
    /// two functions: trash moves to a secondary location the user rarely
    /// visits, so an orphaned file there is a recoverable nuisance fixed
    /// by a retry; an asset moved by this function instead stays visible
    /// everywhere in the app (timeline, search, albums) until it is seen
    /// moving — a row pointing at a nonexistent path would be silent and
    /// invisible there. A physical file left "extra" without a matching
    /// row, on the other hand, gets picked up by the next scan
    /// (re-indexed as a new asset — it loses `asset_flags`/
    /// `asset_overrides` only in **this** half-failure scenario, never on
    /// the normal path). The worst case is recoverable, which is the
    /// deliberate tradeoff behind this ordering.
    ///
    /// # Errors
    /// `Forbidden` if the caller is not editor on the source folder or the
    /// destination folder (`FolderRepo::assert_editor`, called twice —
    /// not `PermissionRepo::assert_can_edit_assets`, which only resolves
    /// the *current* folder of an existing asset via a join and has no
    /// way to check an arbitrary destination, possibly just created and
    /// still empty). `NotFound`/`Forbidden` like `find_by_id` if the asset
    /// does not exist or is not visible. `Collision` if `(new_folder_id,
    /// new_filename)` is already taken by another asset — a
    /// **best-effort** check against what Keeppix knows, not an atomic
    /// filesystem guarantee: an untracked file at the same path can still
    /// be overwritten by `rename()` between the check and the move. `Io`
    /// if the physical `rename()` fails.
    pub async fn move_asset(
        &self,
        ctx: &AuthContext,
        asset_id: AssetId,
        new_folder_id: FolderId,
        new_filename: AssetName,
    ) -> Result<Asset, DbError> {
        let folders = FolderRepo::new(self.db);
        let asset = self.find_by_id(ctx, asset_id).await?;
        folders.assert_editor(ctx, asset.folder_id).await?;
        folders.assert_editor(ctx, new_folder_id).await?;

        let no_op = asset.folder_id == new_folder_id && asset.filename == new_filename;
        if no_op {
            return Ok(asset);
        }

        let collision: Option<uuid::Uuid> = sqlx::query_scalar(
            "SELECT id FROM assets WHERE folder_id = $1 AND filename = $2 AND id <> $3",
        )
        .bind(new_folder_id.as_uuid())
        .bind(new_filename.as_str())
        .bind(asset_id.as_uuid())
        .fetch_optional(self.db.pool())
        .await?;
        if collision.is_some() {
            return Err(DbError::Collision(format!(
                "{} already exists in the destination folder",
                new_filename.as_str()
            )));
        }

        let old_path = folders
            .absolute_path(ctx, asset.folder_id)
            .await?
            .join(asset.filename.as_str());
        let new_path = folders
            .absolute_path(ctx, new_folder_id)
            .await?
            .join(new_filename.as_str());

        if tokio::fs::symlink_metadata(&new_path).await.is_ok() {
            return Err(DbError::Collision(format!(
                "{} already exists on disk",
                new_path.display()
            )));
        }
        tokio::fs::rename(&old_path, &new_path).await.map_err(|e| {
            DbError::Io(format!(
                "moving {} to {}: {e}",
                old_path.display(),
                new_path.display()
            ))
        })?;
        move_sidecar_best_effort(&old_path, &new_path).await;

        let mut tx = self.db.pool().begin().await?;
        sqlx::query(
            "UPDATE assets SET folder_id = $1, filename = $2, updated_at = now() WHERE id = $3",
        )
        .bind(new_folder_id.as_uuid())
        .bind(new_filename.as_str())
        .bind(asset_id.as_uuid())
        .execute(&mut *tx)
        .await
        .map_err(|e| map_move_collision(e, new_filename.as_str()))?;
        tx.commit().await?;

        self.find_by_id(ctx, asset_id).await
    }

    /// Copies the original EXIF onto a new asset. `ON CONFLICT DO NOTHING`.
    ///
    /// Does not take an `AuthContext`: the move detector calls this.
    ///
    /// # Errors
    /// `Connection` if the query fails.
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

    /// Camera models for a set of assets ("Camera" summary dimension). An
    /// asset without a readable `asset_exif` (or without `camera_model` in
    /// the exif) simply does not appear in the map — no explicit `None` to
    /// propagate. Same idiom as [`crate::FlagRepo::favorites_among`]: a
    /// single query for the whole page, not one per asset — this does not
    /// take an `AuthContext` for the same reason as that method: the
    /// caller has already filtered `asset_ids` down to what is visible.
    ///
    /// # Errors
    /// `Connection` if the query fails.
    pub async fn camera_models_among(
        &self,
        asset_ids: &[AssetId],
    ) -> Result<HashMap<AssetId, String>, DbError> {
        if asset_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let ids: Vec<uuid::Uuid> = asset_ids.iter().map(AssetId::as_uuid).collect();
        let rows: Vec<(uuid::Uuid, String)> = sqlx::query_as(
            "SELECT asset_id, camera_model FROM asset_exif \
              WHERE asset_id = ANY($1) AND camera_model IS NOT NULL",
        )
        .bind(&ids)
        .fetch_all(self.db.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|(id, model)| (AssetId::from_uuid(id), model))
            .collect())
    }

    /// The full `asset_exif` row for **one** asset ("SHOT" section of the
    /// info panel): unlike [`Self::camera_models_among`] (bulk, a single
    /// field), here the lightbox opens one photo at a time and needs
    /// everything — lens, exposure, ISO, focal length — columns already
    /// written by [`Self::insert_exif`] but never read in full until now.
    /// `None` if the asset has no `asset_exif` row (never analyzed, or no
    /// readable EXIF) — not an error.
    ///
    /// # Errors
    /// `Connection` if the query fails.
    pub async fn exif_for(&self, asset_id: AssetId) -> Result<Option<AssetExifDetail>, DbError> {
        let row: Option<AssetExifDetailRow> = sqlx::query_as(
            "SELECT camera_make, camera_model, lens, iso, f_number, exposure, focal_length \
               FROM asset_exif WHERE asset_id = $1",
        )
        .bind(asset_id.as_uuid())
        .fetch_optional(self.db.pool())
        .await?;
        Ok(row.map(AssetExifDetailRow::into_domain))
    }

    /// Moves the asset to a different folder **without renaming it** — the
    /// "Move to folder" field of bulk edit, unlike [`Self::move_asset`]
    /// which also takes a new name for the "rename with move" case. A
    /// thin wrapper: it reads the current name and passes it through
    /// unchanged — [`Self::move_asset`] still does a second `find_by_id`
    /// internally (already the case before this method existed, not a
    /// regression introduced here) for its own no-op check.
    ///
    /// # Errors
    /// Same as [`Self::move_asset`].
    pub async fn move_to_folder(
        &self,
        ctx: &AuthContext,
        asset_id: AssetId,
        new_folder_id: FolderId,
    ) -> Result<Asset, DbError> {
        let current = self.find_by_id(ctx, asset_id).await?;
        self.move_asset(ctx, asset_id, new_folder_id, current.filename)
            .await
    }
}

/// `23505` on `assets_folder_filename_key` during `UPDATE` (rare: the
/// `SELECT` in `move_asset` already checks the same collision beforehand,
/// but a concurrent write between the two can still hit the constraint —
/// this is the gate that actually catches it) -> `Collision`, not the
/// generic `Conflict` from `crate::uploads::map_unique_violation` (same
/// column, the counterpart for `ingest_direct`): the bulk-operation
/// failure taxonomy (`FailureReason`, `crates/keeppix-api/src/bulk.rs`)
/// needs to distinguish "the name collides" from "something else is
/// wrong" without parsing the message text.
fn map_move_collision(err: sqlx::Error, new_filename: &str) -> DbError {
    if let sqlx::Error::Database(ref db_err) = err
        && db_err.code().as_deref() == Some("23505")
    {
        return DbError::Collision(format!(
            "{new_filename} already exists in the destination folder"
        ));
    }
    DbError::Connection(err)
}

/// `IMG_1234.ARW` -> `IMG_1234.ARW.xmp`, the same convention as
/// `keeppix_jobs::xmp::sidecar_path_for` — duplicated here, not imported:
/// `keeppix-db` sits below `keeppix-jobs` in the dependency graph
/// (`keeppix-domain -> keeppix-db -> keeppix-media -> keeppix-jobs`), so it
/// cannot depend on it. The sidecar is an **export** derived from
/// `asset_overrides`/`asset_flags` (see `OverridesRepo::pending_sidecars`,
/// `mark_sidecar_written`), not the source of truth: if this move fails,
/// it does not block `move_asset` (the main file has already moved
/// successfully by this point) — an orphaned `.xmp` is left at the old
/// path, which the next sidecar sweep rewrites from scratch at the
/// correct location the next time `asset_overrides` changes. Cost if the
/// file never changes again after this move: the sidecar stays orphaned
/// until someone cleans it up by hand — acceptable because the real data
/// (the columns) never moved, only the export.
async fn move_sidecar_best_effort(old_asset_path: &Path, new_asset_path: &Path) {
    let old_sidecar = sidecar_path_for(old_asset_path);
    if tokio::fs::symlink_metadata(&old_sidecar).await.is_err() {
        return;
    }
    let new_sidecar = sidecar_path_for(new_asset_path);
    if let Err(e) = tokio::fs::rename(&old_sidecar, &new_sidecar).await {
        tracing::warn!(
            error = %e,
            old = %old_sidecar.display(),
            new = %new_sidecar.display(),
            "move_asset: the asset moved but its .xmp sidecar did not; \
             the next sidecar sweep will rewrite it at the new path"
        );
    }
}

fn sidecar_path_for(asset_path: &Path) -> PathBuf {
    let mut name = asset_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_owned();
    name.push_str(".xmp");
    asset_path.with_file_name(name)
}
