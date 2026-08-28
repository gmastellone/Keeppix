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

/// Translates the unique index violation into a readable conflict,
/// distinguishing username and email so the client knows which field to change.
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

    /// Total number of users. Does not require an `AuthContext` because
    /// it is used to establish whether the instance is still pristine,
    /// i.e. before a context can even exist.
    ///
    /// # Errors
    /// `DbError::Connection` if the query fails.
    pub async fn count(&self) -> Result<i64, DbError> {
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM users")
            .fetch_one(self.db.pool())
            .await?;
        Ok(n)
    }

    /// First active admin by `created_at`. Used by background jobs that
    /// need to create an `Operation` without an HTTP request (e.g.
    /// `AiAnalysis`): the WS progress is visible to that owner.
    ///
    /// # Errors
    /// `DbError::Connection` if the query fails.
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

    /// Creates the first administrator. The only write without an
    /// `AuthContext`, allowed only while the table is empty.
    ///
    /// # Errors
    /// `DbError::Conflict` if users already exist.
    pub async fn create_bootstrap_admin(&self, new: NewUser) -> Result<User, DbError> {
        let mut tx = self.db.pool().begin().await?;

        // Locks the table for the duration of the transaction: two
        // concurrent setup requests cannot create two administrators.
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
    /// `DbError::Forbidden` if the caller is not admin; `DbError::Conflict`
    /// if username or email are already in use.
    pub async fn create(&self, ctx: &AuthContext, new: NewUser) -> Result<User, DbError> {
        if !ctx.is_admin() {
            return Err(DbError::Forbidden);
        }
        let mut tx = self.db.pool().begin().await?;
        let row = insert_user(&mut tx, &new).await?;
        tx.commit().await?;
        row.into_domain()
    }

    /// Lookup for login: also returns the password hash. Does not
    /// require an `AuthContext` because this is the step that produces one.
    ///
    /// # Errors
    /// `DbError::Connection` if the query fails.
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
    /// `DbError::Forbidden` if a non-admin user requests an id other than
    /// their own; `DbError::NotFound` if the user does not exist.
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

    /// A user's current role, without an `AuthContext` — same reason as
    /// [`Self::find_by_username`]: used to **reconstruct** the
    /// `AuthContext` of a background job that continues an action already
    /// authorized at HTTP request time (e.g. `BulkRename`), not for
    /// arbitrary inspection. Rereads the **current** role, not the one
    /// captured when the job was enqueued: if an admin gets demoted in
    /// the meantime, the job discovers it here, not before.
    ///
    /// # Errors
    /// `DbError::NotFound` if the user does not exist (more likely
    /// deleted than usual in this context: the account that had
    /// enqueued the job).
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

    /// List of all users. Admin only.
    ///
    /// # Errors
    /// `Forbidden` if not admin.
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

    /// Updates `display_name`, `locale`, and/or `role`. An admin can
    /// target anyone; otherwise only self, and never the role.
    ///
    /// # Errors
    /// `Forbidden` / `NotFound` same as `find_by_id`. `Forbidden` if a
    /// non-admin tries to change the role.
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

    /// Sets `disabled_at = now()`. Admin only. Does not revoke sessions:
    /// the HTTP caller does that with `SessionRepo`.
    ///
    /// # Errors
    /// `Forbidden` if not admin; `NotFound` if the id does not exist.
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
            // Already disabled or nonexistent: distinguish between the two.
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

    /// Clears `disabled_at`. Admin only.
    ///
    /// # Errors
    /// `Forbidden` if not admin; `NotFound` if the id does not exist.
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

    /// Replaces the password hash. The caller has already verified the
    /// current password (or this is a future admin reset).
    ///
    /// # Errors
    /// `Forbidden` / `NotFound` same as `find_by_id`.
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
