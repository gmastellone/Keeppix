use std::path::PathBuf;

use keeppix_domain::{AuthContext, Library, LibraryId, LibraryStatus, NewLibrary, UserId};

use crate::{Db, DbError};

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
    exclude_patterns: Vec<String>,
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
            exclude_patterns: self.exclude_patterns,
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

const COLUMNS: &str = "id, name, owner_id, root_path, scan_enabled, exclude_patterns, \
                       status, last_scan_at, created_at";

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

    /// Registra l'istante dell'ultima scansione completata.
    ///
    /// Non prende un `AuthContext` perché la chiama lo scanner, che non
    /// agisce per conto di un utente. È la quarta e ultima eccezione alla
    /// regola, e non ne vanno aggiunte altre senza la stessa giustificazione.
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
}

fn map_root_path_conflict(err: sqlx::Error) -> DbError {
    if let sqlx::Error::Database(ref db_err) = err
        && db_err.code().as_deref() == Some("23505")
    {
        return DbError::Conflict("root_path is already indexed by another library".to_owned());
    }
    DbError::Connection(err)
}
