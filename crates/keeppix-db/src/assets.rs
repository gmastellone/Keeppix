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

/// Esito di un inserimento diretto (`WebDAV PUT`, Task 7 Fase 5): stessa
/// forma di `crate::uploads::FinalizeOutcome`, senza il concetto di
/// sessione — il chiamante ha già l'intero file in un temporaneo.
pub struct DirectPutOutcome {
    pub asset_id: AssetId,
    /// Nome finale su disco — può differire dal richiesto per una
    /// collisione risolta con un suffisso.
    pub filename: String,
    pub collision: CollisionOutcome,
}

/// EXIF completo di un asset (Fase 11 Task 8, §19.2 "SCATTO") — a
/// differenza di [`AssetRepo::camera_models_among`], che espone solo
/// `camera_model` per la dimensione SP-3, questo è il dettaglio pieno per
/// un singolo asset aperto nel lightbox.
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

    /// Crea l'asset spostando `temp_path` nella cartella finale, risolvendo
    /// un'eventuale collisione di nome esattamente come
    /// `UploadSessionRepo::finalize` (Task 1): stesso nome e stesso hash è
    /// un duplicato, saltato; stesso nome e hash diverso prende un
    /// suffisso — **mai** una sovrascrittura silenziosa.
    ///
    /// Il chiamante deve aver già verificato il permesso di editor sulla
    /// cartella (`FolderRepo::assert_editor`): questo metodo non lo
    /// ripete, per non pagare due volte la stessa query sullo stesso
    /// percorso HTTP (`dav::write::put`).
    ///
    /// Come in `finalize`, il `rename()` avviene prima del commit: se il
    /// commit fallisse dopo, il file resta al posto giusto senza una riga
    /// `assets`, e la prossima scansione della libreria lo scopre come un
    /// file qualunque — mai il rischio opposto di una riga che punta a un
    /// file inesistente.
    ///
    /// # Errors
    /// Come `FolderRepo::absolute_path` per la visibilità della cartella.
    /// `Io` se il `rename()` finale fallisce — il temporaneo resta al suo
    /// posto, nessuna riga viene toccata.
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
            // Stesso nome, stesso hash: duplicato esatto. Non si tocca mai
            // la cartella target — si rimuove solo il temporaneo, prima del
            // commit (che qui non ha nulla da confermare, ma resta simmetrico
            // a `finalize`).
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

    /// Come [`Self::upsert_discovered`], ma per un intero lotto in una sola
    /// istruzione (Fase 10 Task 21): un solo giro di rete invece di uno per
    /// file. Il trigger `assets_change_log` (`AFTER INSERT OR UPDATE ...
    /// FOR EACH ROW`) scrive comunque una riga di `change_log` per asset —
    /// non si perde granularità entità-per-entità richiesta dal sync
    /// mobile (spec §2.6) — ma lo fa dentro questa singola istruzione,
    /// senza un giro di rete per riga.
    ///
    /// Restituisce solo gli asset davvero cambiati (stesso filtro
    /// `mtime`/`size_bytes` di [`Self::upsert_discovered`]): il chiamante
    /// non deve riaccodare metadata/hash per un file fermo.
    ///
    /// Non prende un `AuthContext` perché la chiama lo scanner.
    ///
    /// # Errors
    /// `Connection` se l'inserimento fallisce; `Corrupted` se una riga
    /// restituita non passa la validazione di dominio.
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
    /// Come `find_by_id`. `Connection` se l'aggiornamento fallisce.
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

    /// Quali di `hashes` esistono già come `content_hash` di un asset
    /// visibile al chiamante — il pre-check dell'upload (spec §1.2): «questi
    /// li ho già, caricami solo gli altri». Filtrato per visibilità come
    /// [`Self::find_by_hash`], così non diventa un oracolo su hash che il
    /// chiamante non potrebbe vedere altrimenti.
    ///
    /// # Errors
    /// `Connection` se la query fallisce.
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
    /// `Connection` se la query fallisce.
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

    /// Salva le coordinate EXIF senza calpestare una correzione manuale.
    ///
    /// Non prende un `AuthContext`: la chiama la pipeline di metadati.
    ///
    /// # Errors
    /// `Connection` se l'aggiornamento fallisce.
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

    /// Restituisce gli id fra `ids` che il chiamante può vedere, nello stesso
    /// ordine della richiesta. Gli assenti (inesistenti o fuori scope) non
    /// compaiono: le operazioni a riuscita parziale li mettono in `failed`.
    ///
    /// # Errors
    /// `Connection` se la query fallisce.
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

    /// Verifica in una sola query che il chiamante veda **tutti** gli id
    /// dati. La usano le operazioni che restano all-or-nothing. Per i lotti
    /// a riuscita parziale preferire [`Self::filter_visible`].
    ///
    /// # Errors
    /// `Forbidden` se anche un solo id non è visibile — compreso il caso in
    /// cui non esiste affatto. `NotFound` solo a un admin quando un id non
    /// esiste per niente.
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

    /// Sposta un asset in modo sicuro (Fase 9 Task 1): identità preservata.
    /// `asset_flags`/`asset_overrides`/`asset_tags`/`faces` sono chiavi
    /// esterne su `asset_id`, mai su `folder_id`/`filename` — un `UPDATE`
    /// sulla riga **esistente** (stesso id) le mantiene collegate senza
    /// copiare nulla, a differenza di `moves.rs::after_hash`
    /// (`crates/keeppix-jobs`) che crea una riga nuova e ne perde la
    /// maggior parte perché è un riconoscimento *a posteriori*, non uno
    /// spostamento diretto.
    ///
    /// **Ordine deliberato: il file fisico si sposta prima, la riga dopo —
    /// l'opposto della convenzione già in uso in `TrashRepo::choose`
    /// (`crates/keeppix-db/src/trash.rs`, riga-poi-`rename()`).** Non è
    /// un'incoerenza fra le due funzioni: il cestino sposta verso un posto
    /// secondario che l'utente visita di rado, quindi un file orfano lì è
    /// un fastidio recuperabile con un retry; un asset spostato da questa
    /// funzione resta invece visibile ovunque nell'app (timeline, ricerca,
    /// album) finché non lo si vede spostare — una riga che punta a un
    /// percorso inesistente sarebbe silenziosa e invisibile lì. Un file
    /// fisico "in più" senza riga corrispondente, al contrario, lo ritrova
    /// la prossima scansione (reindicizzato come asset nuovo — perde
    /// `asset_flags`/`asset_overrides` solo in **questo** scenario di
    /// fallimento a metà, mai nel percorso normale). Il caso peggiore è
    /// recuperabile: è il ruling esplicito del piano di fase
    /// (`docs/superpowers/plans/2026-08-20-keeppix-fase-9.md`, Task 1).
    ///
    /// # Errors
    /// `Forbidden` se il chiamante non è editor sulla cartella di partenza o
    /// su quella di destinazione (`FolderRepo::assert_editor`, chiamata due
    /// volte — non `PermissionRepo::assert_can_edit_assets`, che risolve
    /// solo la cartella *corrente* di un asset esistente via join e non ha
    /// modo di verificare una destinazione arbitraria, magari appena creata
    /// e ancora vuota). `NotFound`/`Forbidden` come `find_by_id` se l'asset
    /// non esiste o non è visibile. `Collision` se `(new_folder_id,
    /// new_filename)` è già occupato da un altro asset — verifica
    /// **best-effort** contro ciò che Keeppix conosce (spec §1.2), non una
    /// garanzia atomica di filesystem: un file non ancora tracciato allo
    /// stesso percorso può comunque essere sovrascritto da `rename()` fra
    /// il controllo e lo spostamento. `Io` se il `rename()` fisico fallisce.
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

    /// Modelli di fotocamera per un insieme di asset (Fase 11 Task 7, SP-3
    /// §11: dimensione "Fotocamera"). Un asset senza `asset_exif` leggibile
    /// (o senza `camera_model` nell'exif) semplicemente non compare nella
    /// mappa — nessun `None` esplicito da propagare. Stesso idioma di
    /// [`crate::FlagRepo::favorites_among`]: una query sola per l'intera
    /// pagina, non una per asset — non prende `AuthContext` per lo stesso
    /// motivo di quel metodo, il chiamante ha già filtrato `asset_ids` sul
    /// visibile.
    ///
    /// # Errors
    /// `Connection` se la query fallisce.
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

    /// L'intera riga `asset_exif` di **un** asset (Fase 11 Task 8, §19.2
    /// campi 6-9, sezione "SCATTO" del pannello informazioni): a differenza
    /// di [`Self::camera_models_among`] (bulk, un solo campo, per SP-3),
    /// qui il lightbox apre una foto alla volta e ha bisogno di tutto —
    /// obiettivo, esposizione, ISO, focale — colonne già scritte da
    /// [`Self::insert_exif`] fin dalla Fase 5, mai lette per intero finora.
    /// `None` se l'asset non ha una riga `asset_exif` (mai analizzato, o
    /// senza EXIF leggibile) — non un errore.
    ///
    /// # Errors
    /// `Connection` se la query fallisce.
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

    /// Sposta l'asset in una cartella diversa **senza rinominarlo** — il
    /// campo "Sposta in cartella" di Modifica in blocco (Fase 11 Task 7,
    /// §13.3, campo 8), a differenza di [`Self::move_asset`] che prende
    /// anche un nuovo nome per il caso "Rinomina cartella…"/spostamento con
    /// rinomina contestuale. Wrapper sottile: legge il nome corrente e lo
    /// passa invariato — [`Self::move_asset`] fa comunque un secondo
    /// `find_by_id` al suo interno (già così prima di questo metodo, non
    /// una regressione introdotta qui) per il proprio controllo di no-op.
    ///
    /// # Errors
    /// Come [`Self::move_asset`].
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

/// `23505` su `assets_folder_filename_key` durante `UPDATE` (raro: la
/// `SELECT` di `move_asset` già controlla la stessa collisione prima, ma una
/// scrittura concorrente fra le due può ancora colpire il vincolo — questo
/// è il gate che la intercetta davvero) → `Collision`, non il generico
/// `Conflict` di `crate::uploads::map_unique_violation` (stessa colonna,
/// controparte per `ingest_direct`): la tassonomia delle operazioni di
/// massa (`FailureReason`, `crates/keeppix-api/src/bulk.rs`) deve poter
/// distinguere "il nome collide" da "qualcos'altro non torna" senza
/// analizzare il testo del messaggio.
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

/// `IMG_1234.ARW` → `IMG_1234.ARW.xmp`, stessa convenzione di
/// `keeppix_jobs::xmp::sidecar_path_for` — duplicata qui, non importata:
/// `keeppix-db` sta sotto `keeppix-jobs` nel grafo delle dipendenze
/// (`keeppix-domain → keeppix-db → keeppix-media → keeppix-jobs`), quindi
/// non può dipenderne. Il sidecar è un **export** derivato da
/// `asset_overrides`/`asset_flags` (vedi `OverridesRepo::pending_sidecars`,
/// `mark_sidecar_written`), non la fonte di verità: se questo spostamento
/// fallisce, non si blocca `move_asset` (il file principale è già spostato
/// con successo a questo punto) — resta un `.xmp` orfano al vecchio
/// percorso, che il prossimo giro dello sweep dei sidecar riscrive da zero
/// alla posizione corretta la prossima volta che `asset_overrides` cambia.
/// Costo se il file non cambia mai più dopo questo spostamento: il sidecar
/// resta orfano finché qualcuno non lo pulisce a mano — accettabile perché
/// il dato vero (le colonne) non si è mosso, solo l'export.
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
