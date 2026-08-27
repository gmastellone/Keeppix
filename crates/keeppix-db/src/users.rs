use keeppix_domain::{AuthContext, NewUser, PasswordHash, SystemRole, User, UserId, Username};

use crate::{Db, DbError};

pub struct UserRepo<'a> {
    db: &'a Db,
}

#[derive(sqlx::FromRow)]
struct UserRow {
    id: uuid::Uuid,
    username: String,
    email: Option<String>,
    display_name: String,
    role: String,
    locale: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
    disabled_at: Option<chrono::DateTime<chrono::Utc>>,
    password_changed_at: chrono::DateTime<chrono::Utc>,
}

impl UserRow {
    fn into_domain(self) -> Result<User, DbError> {
        let username =
            Username::parse(&self.username).map_err(|e| crate::row::corrupted("username", e))?;
        let role = match self.role.as_str() {
            "admin" => SystemRole::Admin,
            "user" => SystemRole::User,
            other => return Err(crate::row::corrupted("role", other)),
        };
        Ok(User {
            id: UserId::from_uuid(self.id),
            username,
            email: self.email,
            display_name: self.display_name,
            role,
            locale: self.locale,
            created_at: self.created_at,
            disabled_at: self.disabled_at,
            password_changed_at: self.password_changed_at,
        })
    }
}

#[derive(sqlx::FromRow)]
struct UserWithHashRow {
    #[sqlx(flatten)]
    user: UserRow,
    password_hash: String,
}

impl UserWithHashRow {
    fn into_domain(self) -> Result<(User, PasswordHash), DbError> {
        Ok((
            self.user.into_domain()?,
            PasswordHash::from_stored(self.password_hash),
        ))
    }
}

const fn role_str(role: SystemRole) -> &'static str {
    match role {
        SystemRole::Admin => "admin",
        SystemRole::User => "user",
    }
}

/// Traduce la violazione dell'indice unico in un conflitto leggibile,
/// distinguendo username ed email così il client sa quale campo cambiare.
fn map_unique_violation(err: sqlx::Error) -> DbError {
    if let sqlx::Error::Database(ref db_err) = err
        && db_err.code().as_deref() == Some("23505")
    {
        let constraint = db_err.constraint().unwrap_or("");
        let detail = if constraint.contains("username") {
            "username is already in use"
        } else if constraint.contains("email") {
            "email is already in use"
        } else {
            "username or email already in use"
        };
        return DbError::Conflict(detail.to_owned());
    }
    DbError::Connection(err)
}

impl<'a> UserRepo<'a> {
    #[must_use]
    pub const fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// Numero totale di utenti. Non richiede `AuthContext` perché serve a
    /// stabilire se l'istanza è ancora vergine, cioè prima che un contesto
    /// possa esistere.
    ///
    /// # Errors
    /// `DbError::Connection` se la query fallisce.
    pub async fn count(&self) -> Result<i64, DbError> {
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM users")
            .fetch_one(self.db.pool())
            .await?;
        Ok(n)
    }

    /// Primo admin attivo per `created_at`. Usato dai job di background che
    /// devono creare un'`Operation` senza richiesta HTTP (Fase 7
    /// `AiAnalysis`): l'avanzamento WS è visibile a quell'owner.
    ///
    /// # Errors
    /// `DbError::Connection` se la query fallisce.
    pub async fn first_admin_id(&self) -> Result<Option<UserId>, DbError> {
        let id: Option<uuid::Uuid> = sqlx::query_scalar(
            "SELECT id FROM users \
             WHERE role = 'admin' AND disabled_at IS NULL \
             ORDER BY created_at ASC \
             LIMIT 1",
        )
        .fetch_optional(self.db.pool())
        .await?;
        Ok(id.map(UserId::from_uuid))
    }

    /// Crea il primo amministratore. Unica scrittura priva di `AuthContext`,
    /// permessa solo finché la tabella è vuota.
    ///
    /// # Errors
    /// `DbError::Conflict` se esistono già utenti.
    pub async fn create_bootstrap_admin(&self, new: NewUser) -> Result<User, DbError> {
        let mut tx = self.db.pool().begin().await?;

        // Blocca la tabella per la durata della transazione: due richieste di
        // setup concorrenti non possono creare due amministratori.
        sqlx::query("LOCK TABLE users IN EXCLUSIVE MODE")
            .execute(&mut *tx)
            .await?;

        let existing: i64 = sqlx::query_scalar("SELECT count(*) FROM users")
            .fetch_one(&mut *tx)
            .await?;
        if existing > 0 {
            return Err(DbError::Conflict(
                "instance is already initialised".to_owned(),
            ));
        }

        let row = insert_user(&mut tx, &new).await?;
        tx.commit().await?;
        row.into_domain()
    }

    /// # Errors
    /// `DbError::Forbidden` se il chiamante non è admin; `DbError::Conflict`
    /// se username o email sono già in uso.
    pub async fn create(&self, ctx: &AuthContext, new: NewUser) -> Result<User, DbError> {
        if !ctx.is_admin() {
            return Err(DbError::Forbidden);
        }
        let mut tx = self.db.pool().begin().await?;
        let row = insert_user(&mut tx, &new).await?;
        tx.commit().await?;
        row.into_domain()
    }

    /// Ricerca per il login: restituisce anche l'hash della password.
    /// Non richiede `AuthContext` perché è il passo che lo produce.
    ///
    /// # Errors
    /// `DbError::Connection` se la query fallisce.
    pub async fn find_by_username(
        &self,
        username: &Username,
    ) -> Result<Option<(User, PasswordHash)>, DbError> {
        let row: Option<UserWithHashRow> = sqlx::query_as(
            "SELECT id, username, email, display_name, password_hash, role, locale, \
                    created_at, disabled_at, password_changed_at \
               FROM users WHERE lower(username) = lower($1)",
        )
        .bind(username.as_str())
        .fetch_optional(self.db.pool())
        .await?;

        row.map(UserWithHashRow::into_domain).transpose()
    }

    /// # Errors
    /// `DbError::Forbidden` se un utente non-admin chiede un id diverso dal
    /// proprio; `DbError::NotFound` se l'utente non esiste.
    pub async fn find_by_id(&self, ctx: &AuthContext, id: UserId) -> Result<User, DbError> {
        if !ctx.is_admin() && ctx.user_id() != Some(id) {
            return Err(DbError::Forbidden);
        }

        let row: Option<UserRow> = sqlx::query_as(
            "SELECT id, username, email, display_name, role, locale, created_at, disabled_at, \
                    password_changed_at \
               FROM users WHERE id = $1",
        )
        .bind(id.as_uuid())
        .fetch_optional(self.db.pool())
        .await?;

        row.ok_or(DbError::NotFound)?.into_domain()
    }

    /// Ruolo attuale di un utente, senza `AuthContext` — stesso motivo di
    /// [`Self::find_by_username`]: usata per **ricostruire** l'`AuthContext`
    /// di un job in background che continua un'azione già autorizzata al
    /// momento della richiesta HTTP (es. `BulkRename`, Task 10), non per
    /// un'ispezione arbitraria. Rilegge il ruolo **corrente**, non quello
    /// catturato quando il job è stato accodato: se nel frattempo un admin
    /// viene retrocesso, il job lo scopre qui, non prima.
    ///
    /// # Errors
    /// `DbError::NotFound` se l'utente non esiste (più cancellato del
    /// consueto in questo contesto: l'account che aveva accodato il job).
    pub async fn role_for(&self, id: UserId) -> Result<SystemRole, DbError> {
        let role: Option<String> = sqlx::query_scalar("SELECT role FROM users WHERE id = $1")
            .bind(id.as_uuid())
            .fetch_optional(self.db.pool())
            .await?;
        match role.as_deref() {
            Some("admin") => Ok(SystemRole::Admin),
            Some("user") => Ok(SystemRole::User),
            Some(other) => Err(crate::row::corrupted("role", other)),
            None => Err(DbError::NotFound),
        }
    }

    /// Elenco di tutti gli utenti. Solo admin.
    ///
    /// # Errors
    /// `Forbidden` se non admin.
    pub async fn list(&self, ctx: &AuthContext) -> Result<Vec<User>, DbError> {
        if !ctx.is_admin() {
            return Err(DbError::Forbidden);
        }
        let rows: Vec<UserRow> = sqlx::query_as(
            "SELECT id, username, email, display_name, role, locale, created_at, disabled_at, \
                    password_changed_at \
               FROM users ORDER BY username",
        )
        .fetch_all(self.db.pool())
        .await?;
        rows.into_iter().map(UserRow::into_domain).collect()
    }

    /// Aggiorna `display_name`, `locale` e/o `role`. Admin può chiunque;
    /// altrimenti solo sé, e non il ruolo.
    ///
    /// # Errors
    /// `Forbidden` / `NotFound` come `find_by_id`. `Forbidden` se un non-admin
    /// prova a cambiare il ruolo.
    pub async fn update_profile(
        &self,
        ctx: &AuthContext,
        id: UserId,
        display_name: Option<&str>,
        locale: Option<&str>,
        role: Option<SystemRole>,
    ) -> Result<User, DbError> {
        self.find_by_id(ctx, id).await?;
        if role.is_some() && !ctx.is_admin() {
            return Err(DbError::Forbidden);
        }
        let row: UserRow = sqlx::query_as(
            "UPDATE users SET \
                display_name = COALESCE($2, display_name), \
                locale = COALESCE($3, locale), \
                role = COALESCE($4, role), \
                updated_at = now() \
              WHERE id = $1 \
              RETURNING id, username, email, display_name, role, locale, created_at, disabled_at, \
                        password_changed_at",
        )
        .bind(id.as_uuid())
        .bind(display_name)
        .bind(locale)
        .bind(role.map(role_str))
        .fetch_one(self.db.pool())
        .await?;
        self.db.invalidate_permission_cache_for_user(id).await;
        row.into_domain()
    }

    /// Imposta `disabled_at = now()`. Solo admin. Non revoca le sessioni:
    /// lo fa il chiamante HTTP con `SessionRepo`.
    ///
    /// # Errors
    /// `Forbidden` se non admin; `NotFound` se l'id non esiste.
    pub async fn disable(&self, ctx: &AuthContext, id: UserId) -> Result<(), DbError> {
        if !ctx.is_admin() {
            return Err(DbError::Forbidden);
        }
        let result = sqlx::query(
            "UPDATE users SET disabled_at = now(), updated_at = now() \
              WHERE id = $1 AND disabled_at IS NULL",
        )
        .bind(id.as_uuid())
        .execute(self.db.pool())
        .await?;
        if result.rows_affected() == 0 {
            // Già disabilitato o inesistente: distingue.
            let exists: bool =
                sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE id = $1)")
                    .bind(id.as_uuid())
                    .fetch_one(self.db.pool())
                    .await?;
            if exists {
                return Ok(());
            }
            return Err(DbError::NotFound);
        }
        self.db.invalidate_permission_cache_for_user(id).await;
        Ok(())
    }

    /// Azzera `disabled_at`. Solo admin.
    ///
    /// # Errors
    /// `Forbidden` se non admin; `NotFound` se l'id non esiste.
    pub async fn enable(&self, ctx: &AuthContext, id: UserId) -> Result<(), DbError> {
        if !ctx.is_admin() {
            return Err(DbError::Forbidden);
        }
        let result = sqlx::query(
            "UPDATE users SET disabled_at = NULL, updated_at = now() \
              WHERE id = $1 AND disabled_at IS NOT NULL",
        )
        .bind(id.as_uuid())
        .execute(self.db.pool())
        .await?;
        if result.rows_affected() == 0 {
            let exists: bool =
                sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE id = $1)")
                    .bind(id.as_uuid())
                    .fetch_one(self.db.pool())
                    .await?;
            if exists {
                return Ok(());
            }
            return Err(DbError::NotFound);
        }
        self.db.invalidate_permission_cache_for_user(id).await;
        Ok(())
    }

    /// Sostituisce l'hash della password. Il chiamante ha già verificato
    /// la password attuale (o è un reset admin futuro).
    ///
    /// # Errors
    /// `Forbidden` / `NotFound` come `find_by_id`.
    pub async fn set_password_hash(
        &self,
        ctx: &AuthContext,
        id: UserId,
        password_hash: &str,
    ) -> Result<(), DbError> {
        self.find_by_id(ctx, id).await?;
        sqlx::query(
            "UPDATE users SET password_hash = $2, updated_at = now(), \
             password_changed_at = now() WHERE id = $1",
        )
        .bind(id.as_uuid())
        .bind(password_hash)
        .execute(self.db.pool())
        .await?;
        Ok(())
    }
}

async fn insert_user(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    new: &NewUser,
) -> Result<UserRow, DbError> {
    sqlx::query_as(
        "INSERT INTO users (id, username, email, display_name, password_hash, role) \
         VALUES ($1, $2, $3, $4, $5, $6) \
         RETURNING id, username, email, display_name, role, locale, created_at, disabled_at, \
                   password_changed_at",
    )
    .bind(UserId::new().as_uuid())
    .bind(new.username.as_str())
    .bind(new.email.as_deref())
    .bind(&new.display_name)
    .bind(&new.password_hash)
    .bind(role_str(new.role))
    .fetch_one(&mut **tx)
    .await
    .map_err(map_unique_violation)
}
