use std::path::PathBuf;

use keeppix_domain::{
    AuthContext, FolderId, Library, LibraryId, LibraryStatus, NewLibrary, UserId,
};

use crate::uploads;
use crate::{Db, DbError};

/// Spazio libero e totale sul volume di una libreria.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LibraryStorage {
    pub free_bytes: u64,
    pub total_bytes: u64,
}

pub struct LibraryRepo<'a> {
    db: &'a Db,
}

#[derive(sqlx::FromRow)]
struct LibraryRow {
    id: uuid::Uuid,
    name: String,
    owner_id: uuid::Uuid,
    root_path: String,
    scan_enabled: bool,
    faces_enabled: bool,
    exclude_patterns: Vec<String>,
    culling_root_folder_id: Option<uuid::Uuid>,
    status: String,
    last_scan_at: Option<chrono::DateTime<chrono::Utc>>,
    created_at: chrono::DateTime<chrono::Utc>,
}

impl LibraryRow {
    fn into_domain(self) -> Result<Library, DbError> {
        let status = match self.status.as_str() {
            "active" => LibraryStatus::Active,
            "offline" => LibraryStatus::Offline,
            other => return Err(crate::row::corrupted("library status", other)),
        };
        Ok(Library {
            id: LibraryId::from_uuid(self.id),
            name: self.name,
            owner_id: UserId::from_uuid(self.owner_id),
            root_path: PathBuf::from(self.root_path),
            scan_enabled: self.scan_enabled,
            faces_enabled: self.faces_enabled,
            exclude_patterns: self.exclude_patterns,
            culling_root_folder_id: self.culling_root_folder_id.map(FolderId::from_uuid),
            status,
            last_scan_at: self.last_scan_at,
            created_at: self.created_at,
        })
    }
}

const fn status_str(status: LibraryStatus) -> &'static str {
    match status {
        LibraryStatus::Active => "active",
        LibraryStatus::Offline => "offline",
    }
}

const COLUMNS: &str = "id, name, owner_id, root_path, scan_enabled, faces_enabled, \
                       exclude_patterns, culling_root_folder_id, status, last_scan_at, \
                       created_at";

impl<'a> LibraryRepo<'a> {
    #[must_use]
    pub const fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// # Errors
    /// `Forbidden` se il chiamante non è admin; `Conflict` se il percorso è
    /// già indicizzato da un'altra libreria.
    pub async fn create(&self, ctx: &AuthContext, new: NewLibrary) -> Result<Library, DbError> {
        if !ctx.is_admin() {
            return Err(DbError::Forbidden);
        }

        let row: LibraryRow = sqlx::query_as(&format!(
            "INSERT INTO libraries (id, name, owner_id, root_path, exclude_patterns) \
             VALUES ($1, $2, $3, $4, $5) RETURNING {COLUMNS}"
        ))
        .bind(LibraryId::new().as_uuid())
        .bind(&new.name)
        .bind(new.owner_id.as_uuid())
        .bind(new.root_path.to_string_lossy().as_ref())
        .bind(&new.exclude_patterns)
        .fetch_one(self.db.pool())
        .await
        .map_err(map_root_path_conflict)?;

        self.db
            .invalidate_permission_cache_for_user(new.owner_id)
            .await;
        row.into_domain()
    }

    /// Un amministratore vede tutte le librerie, chiunque altro solo le
    /// proprie.
    ///
    /// # Errors
    /// `Connection` se la query fallisce.
    pub async fn list(&self, ctx: &AuthContext) -> Result<Vec<Library>, DbError> {
        let owner_filter = if ctx.is_admin() { None } else { ctx.user_id() };

        let rows: Vec<LibraryRow> = sqlx::query_as(&format!(
            "SELECT {COLUMNS} FROM libraries \
              WHERE $1::uuid IS NULL OR owner_id = $1 \
              ORDER BY name"
        ))
        .bind(owner_filter.map(|id| id.as_uuid()))
        .fetch_all(self.db.pool())
        .await?;

        rows.into_iter().map(LibraryRow::into_domain).collect()
    }

    /// # Errors
    /// `Forbidden` se la libreria non è del chiamante e non è admin — anche
    /// quando l'id non esiste, per non offrire un oracolo di esistenza.
    /// `NotFound` solo a un admin che chiede un id inesistente.
    pub async fn find_by_id(&self, ctx: &AuthContext, id: LibraryId) -> Result<Library, DbError> {
        let row: Option<LibraryRow> =
            sqlx::query_as(&format!("SELECT {COLUMNS} FROM libraries WHERE id = $1"))
                .bind(id.as_uuid())
                .fetch_optional(self.db.pool())
                .await?;

        match row {
            Some(row)
                if ctx.is_admin() || Some(UserId::from_uuid(row.owner_id)) == ctx.user_id() =>
            {
                row.into_domain()
            }
            None if ctx.is_admin() => Err(DbError::NotFound),
            None | Some(_) => Err(DbError::Forbidden),
        }
    }

    /// Spazio libero e totale sul volume della libreria. Il risultato è in
    /// cache per 60 secondi: la sidebar lo chiede a ogni caricamento e
    /// `statvfs` su un volume di rete non è gratis.
    ///
    /// # Errors
    /// `Forbidden` se la libreria non è visibile (anche se l'id non esiste).
    /// `NotFound` solo a un admin su id assente. `Io` se `statvfs` fallisce.
    pub async fn storage(
        &self,
        ctx: &AuthContext,
        id: LibraryId,
    ) -> Result<LibraryStorage, DbError> {
        let library = self.find_by_id(ctx, id).await?;
        let cache = self.db.library_storage_cache();
        if let Some(cached) = cache.get(&id.as_uuid()).await {
            return Ok(cached);
        }
        let (free_bytes, total_bytes) = uploads::disk_usage(&library.root_path)?;
        let usage = LibraryStorage {
            free_bytes,
            total_bytes,
        };
        cache.insert(id.as_uuid(), usage).await;
        Ok(usage)
    }

    /// Verifica di raggiungibilità su richiesta (§47 «Riprova connessione»):
    /// un semplice `is_dir` sul `root_path`, che porta lo stesso costo dello
    /// `stat` già fatto da `discover::run` ad ogni scansione — nessuna nuova
    /// primitiva di I/O. Aggiorna lo stato solo se cambia, così una libreria
    /// già `active` sondata di nuovo non genera un `UPDATE` a vuoto.
    ///
    /// # Errors
    /// `Forbidden` se la libreria non è visibile (anche se l'id non esiste).
    /// `NotFound` solo a un admin su id assente.
    pub async fn probe(&self, ctx: &AuthContext, id: LibraryId) -> Result<Library, DbError> {
        let library = self.find_by_id(ctx, id).await?;
        let reachable = library.root_path.is_dir();
        let status = if reachable {
            LibraryStatus::Active
        } else {
            LibraryStatus::Offline
        };
        if status == library.status {
            return Ok(library);
        }
        self.set_status(ctx, id, status).await?;
        self.find_by_id(ctx, id).await
    }

    /// # Errors
    /// `Forbidden` se il chiamante non può vedere la libreria.
    pub async fn set_status(
        &self,
        ctx: &AuthContext,
        id: LibraryId,
        status: LibraryStatus,
    ) -> Result<(), DbError> {
        // Riusa il controllo di find_by_id invece di riscriverlo.
        self.find_by_id(ctx, id).await?;

        sqlx::query("UPDATE libraries SET status = $2, updated_at = now() WHERE id = $1")
            .bind(id.as_uuid())
            .bind(status_str(status))
            .execute(self.db.pool())
            .await?;
        Ok(())
    }

    /// Non prende un `AuthContext`: la chiama lo scanner, che non agisce
    /// per conto di un utente.
    ///
    /// # Errors
    /// `NotFound` se l'id non esiste.
    pub async fn load_for_scan(&self, id: LibraryId) -> Result<Library, DbError> {
        let row: Option<LibraryRow> =
            sqlx::query_as(&format!("SELECT {COLUMNS} FROM libraries WHERE id = $1"))
                .bind(id.as_uuid())
                .fetch_optional(self.db.pool())
                .await?;
        row.map(LibraryRow::into_domain)
            .transpose()?
            .ok_or(DbError::NotFound)
    }

    /// Non prende un `AuthContext`: disco assente o svuotato durante la scansione.
    ///
    /// # Errors
    /// `Connection` se l'aggiornamento fallisce.
    pub async fn set_status_for_scan(
        &self,
        id: LibraryId,
        status: LibraryStatus,
    ) -> Result<(), DbError> {
        sqlx::query("UPDATE libraries SET status = $2, updated_at = now() WHERE id = $1")
            .bind(id.as_uuid())
            .bind(status_str(status))
            .execute(self.db.pool())
            .await?;
        Ok(())
    }

    /// Elenco per il watcher all'avvio del processo. Non prende un
    /// `AuthContext`: non agisce per conto di un utente.
    ///
    /// # Errors
    /// `Connection` se la query fallisce.
    pub async fn list_for_scan(&self) -> Result<Vec<Library>, DbError> {
        let rows: Vec<LibraryRow> =
            sqlx::query_as(&format!("SELECT {COLUMNS} FROM libraries ORDER BY name"))
                .fetch_all(self.db.pool())
                .await?;
        rows.into_iter().map(LibraryRow::into_domain).collect()
    }

    /// Registra l'istante dell'ultima scansione completata.
    ///
    /// Non prende un `AuthContext` perché la chiama lo scanner, che non
    /// agisce per conto di un utente. Stessa giustificazione delle altre
    /// eccezioni `*_for_scan`.
    ///
    /// # Errors
    /// `Connection` se l'aggiornamento fallisce.
    pub async fn mark_scanned(&self, id: LibraryId) -> Result<(), DbError> {
        sqlx::query("UPDATE libraries SET last_scan_at = now(), updated_at = now() WHERE id = $1")
            .bind(id.as_uuid())
            .execute(self.db.pool())
            .await?;
        Ok(())
    }

    /// Aggiorna nome, `scan_enabled`, `faces_enabled` e/o `exclude_patterns`.
    ///
    /// # Errors
    /// `Forbidden` se il chiamante non può vedere la libreria (anche se l'id
    /// non esiste, per i non-admin). `NotFound` solo a un admin su id assente.
    pub async fn update(
        &self,
        ctx: &AuthContext,
        id: LibraryId,
        name: Option<&str>,
        scan_enabled: Option<bool>,
        faces_enabled: Option<bool>,
        exclude_patterns: Option<&[String]>,
    ) -> Result<Library, DbError> {
        self.find_by_id(ctx, id).await?;

        let row: LibraryRow = sqlx::query_as(&format!(
            "UPDATE libraries SET \
                name = COALESCE($2, name), \
                scan_enabled = COALESCE($3, scan_enabled), \
                faces_enabled = COALESCE($4, faces_enabled), \
                exclude_patterns = COALESCE($5, exclude_patterns), \
                updated_at = now() \
              WHERE id = $1 \
              RETURNING {COLUMNS}"
        ))
        .bind(id.as_uuid())
        .bind(name)
        .bind(scan_enabled)
        .bind(faces_enabled)
        .bind(exclude_patterns)
        .fetch_one(self.db.pool())
        .await?;

        row.into_domain()
    }

    /// Designa (o rimuove, con `None`) la radice del culling a cartelle
    /// (Fase 9 Task 2, spec §2.6). A differenza di [`Self::update`] — aperto
    /// a chiunque veda la libreria, come le altre impostazioni — qui il
    /// permesso è **owner o admin esplicito**: la radice decide dove
    /// finiscono fisicamente le foto scelte/scartate e cosa l'analisi IA
    /// esclude (`libraries.culling_root_folder_id`, letta da
    /// `embeddings.rs`/`faces.rs` dalla Fase 7), non una preferenza di
    /// visualizzazione.
    ///
    /// # Errors
    /// `Forbidden` se il chiamante non vede la libreria, o la vede ma non ne
    /// è proprietario/admin. `Conflict` se `folder_id` non appartiene a
    /// questa libreria.
    pub async fn set_culling_root(
        &self,
        ctx: &AuthContext,
        id: LibraryId,
        folder_id: Option<FolderId>,
    ) -> Result<Library, DbError> {
        let library = self.find_by_id(ctx, id).await?;
        if !ctx.is_admin() && ctx.user_id() != Some(library.owner_id) {
            return Err(DbError::Forbidden);
        }
        if let Some(folder_id) = folder_id {
            let belongs: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM folders WHERE id = $1 AND library_id = $2)",
            )
            .bind(folder_id.as_uuid())
            .bind(id.as_uuid())
            .fetch_one(self.db.pool())
            .await?;
            if !belongs {
                return Err(DbError::Conflict(
                    "culling root folder must belong to this library".to_owned(),
                ));
            }
        }

        let row: LibraryRow = sqlx::query_as(&format!(
            "UPDATE libraries SET culling_root_folder_id = $2, updated_at = now() \
              WHERE id = $1 \
              RETURNING {COLUMNS}"
        ))
        .bind(id.as_uuid())
        .bind(folder_id.map(|f| f.as_uuid()))
        .fetch_one(self.db.pool())
        .await?;

        row.into_domain()
    }

    /// Cancella la riga (e in cascata cartelle/asset). **Non tocca i file
    /// sul disco.** Solo admin.
    ///
    /// # Errors
    /// `Forbidden` se il chiamante non è admin; `NotFound` se l'id non esiste.
    pub async fn delete(&self, ctx: &AuthContext, id: LibraryId) -> Result<(), DbError> {
        if !ctx.is_admin() {
            return Err(DbError::Forbidden);
        }

        let owner: Option<uuid::Uuid> =
            sqlx::query_scalar("SELECT owner_id FROM libraries WHERE id = $1")
                .bind(id.as_uuid())
                .fetch_optional(self.db.pool())
                .await?;
        let Some(owner) = owner else {
            return Err(DbError::NotFound);
        };

        let result = sqlx::query("DELETE FROM libraries WHERE id = $1")
            .bind(id.as_uuid())
            .execute(self.db.pool())
            .await?;
        if result.rows_affected() == 0 {
            return Err(DbError::NotFound);
        }
        self.db
            .invalidate_permission_cache_for_user(UserId::from_uuid(owner))
            .await;
        Ok(())
    }
}

fn map_root_path_conflict(err: sqlx::Error) -> DbError {
    if let sqlx::Error::Database(ref db_err) = err
        && db_err.code().as_deref() == Some("23505")
    {
        return DbError::Conflict("root_path is already indexed by another library".to_owned());
    }
    DbError::Connection(err)
}
