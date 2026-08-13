use keeppix_domain::{AuthContext, NewUser, PasswordHash, SystemRole, User, UserId, Username};
use sqlx::Row;

use crate::{Db, DbError};

pub struct UserRepo<'a> {
    db: &'a Db,
}

/// Riga grezza della tabella `users`, convertita in `User` dal dominio.
struct UserRow {
    id: uuid::Uuid,
    username: String,
    email: Option<String>,
    display_name: String,
    role: String,
    locale: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
    disabled_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl UserRow {
    fn into_domain(self) -> Result<User, DbError> {
        let username = Username::parse(&self.username)
            .map_err(|e| DbError::Migration(format!("stored username is invalid: {e}")))?;
        let role = match self.role.as_str() {
            "admin" => SystemRole::Admin,
            "user" => SystemRole::User,
            other => return Err(DbError::Migration(format!("unknown role: {other}"))),
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
        })
    }
}

const fn role_str(role: SystemRole) -> &'static str {
    match role {
        SystemRole::Admin => "admin",
        SystemRole::User => "user",
    }
}

/// Traduce la violazione dell'indice unico in un conflitto leggibile.
fn map_unique_violation(err: sqlx::Error) -> DbError {
    if let sqlx::Error::Database(ref db_err) = err
        && db_err.code().as_deref() == Some("23505")
    {
        return DbError::Conflict("username or email already in use".to_owned());
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
        let row = sqlx::query(
            "SELECT id, username, email, display_name, password_hash, role, locale, \
                    created_at, disabled_at \
               FROM users WHERE lower(username) = lower($1)",
        )
        .bind(username.as_str())
        .fetch_optional(self.db.pool())
        .await?;

        let Some(row) = row else { return Ok(None) };

        let hash = PasswordHash::from_stored(row.try_get("password_hash")?);
        let user = UserRow {
            id: row.try_get("id")?,
            username: row.try_get("username")?,
            email: row.try_get("email")?,
            display_name: row.try_get("display_name")?,
            role: row.try_get("role")?,
            locale: row.try_get("locale")?,
            created_at: row.try_get("created_at")?,
            disabled_at: row.try_get("disabled_at")?,
        }
        .into_domain()?;

        Ok(Some((user, hash)))
    }

    /// # Errors
    /// `DbError::Forbidden` se un utente non-admin chiede un id diverso dal
    /// proprio; `DbError::NotFound` se l'utente non esiste.
    pub async fn find_by_id(&self, ctx: &AuthContext, id: UserId) -> Result<User, DbError> {
        if !ctx.is_admin() && ctx.user_id() != Some(id) {
            return Err(DbError::Forbidden);
        }

        let row = sqlx::query(
            "SELECT id, username, email, display_name, role, locale, created_at, disabled_at \
               FROM users WHERE id = $1",
        )
        .bind(id.as_uuid())
        .fetch_optional(self.db.pool())
        .await?
        .ok_or(DbError::NotFound)?;

        UserRow {
            id: row.try_get("id")?,
            username: row.try_get("username")?,
            email: row.try_get("email")?,
            display_name: row.try_get("display_name")?,
            role: row.try_get("role")?,
            locale: row.try_get("locale")?,
            created_at: row.try_get("created_at")?,
            disabled_at: row.try_get("disabled_at")?,
        }
        .into_domain()
    }
}

async fn insert_user(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    new: &NewUser,
) -> Result<UserRow, DbError> {
    let row = sqlx::query(
        "INSERT INTO users (id, username, email, display_name, password_hash, role) \
         VALUES ($1, $2, $3, $4, $5, $6) \
         RETURNING id, username, email, display_name, role, locale, created_at, disabled_at",
    )
    .bind(UserId::new().as_uuid())
    .bind(new.username.as_str())
    .bind(new.email.as_deref())
    .bind(&new.display_name)
    .bind(&new.password_hash)
    .bind(role_str(new.role))
    .fetch_one(&mut **tx)
    .await
    .map_err(map_unique_violation)?;

    Ok(UserRow {
        id: row.try_get("id")?,
        username: row.try_get("username")?,
        email: row.try_get("email")?,
        display_name: row.try_get("display_name")?,
        role: row.try_get("role")?,
        locale: row.try_get("locale")?,
        created_at: row.try_get("created_at")?,
        disabled_at: row.try_get("disabled_at")?,
    })
}
