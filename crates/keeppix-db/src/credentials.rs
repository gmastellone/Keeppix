//! App passwords: dedicated credentials for non-interactive clients
//! (`WebDAV`). See `keeppix_domain::credential` for the domain types.

use chrono::{DateTime, Utc};
use keeppix_domain::{
    AppPasswordId, AppPasswordSecret, AppPasswordSummary, AuthContext, Password, PasswordHash,
    UserId, hash_password, verify_password,
};
use uuid::Uuid;

use crate::{Db, DbError};

#[derive(sqlx::FromRow)]
struct SummaryRow {
    id: Uuid,
    user_id: Uuid,
    label: String,
    created_at: DateTime<Utc>,
    last_used_at: Option<DateTime<Utc>>,
}

impl SummaryRow {
    fn into_domain(self) -> AppPasswordSummary {
        AppPasswordSummary {
            id: AppPasswordId::from_uuid(self.id),
            user_id: UserId::from_uuid(self.user_id),
            label: self.label,
            created_at: self.created_at,
            last_used_at: self.last_used_at,
        }
    }
}

pub struct AppPasswordRepo<'a> {
    db: &'a Db,
}

impl<'a> AppPasswordRepo<'a> {
    #[must_use]
    pub const fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// Creates an app password for the authenticated user. The plaintext
    /// secret is returned only once, together with the summary: from this
    /// point on only the Argon2id hash remains in `secret_hash`, and no
    /// later call can retrieve it again.
    ///
    /// # Errors
    /// `DbError::Forbidden` if the caller is not an authenticated user — a
    /// shared link has no identity to attach the app password to.
    pub async fn create(
        &self,
        ctx: &AuthContext,
        label: String,
    ) -> Result<(AppPasswordSummary, AppPasswordSecret), DbError> {
        let user_id = ctx.user_id().ok_or(DbError::Forbidden)?;
        let secret = AppPasswordSecret::generate();
        let password = Password::parse(secret.expose()).map_err(|e| {
            DbError::Corrupted(format!("generated app-password secret rejected: {e}"))
        })?;
        let hash = hash_password(&password)
            .map_err(|e| DbError::Corrupted(format!("app-password hashing failed: {e}")))?;

        let row: SummaryRow = sqlx::query_as(
            "INSERT INTO app_passwords (id, user_id, label, secret_hash) \
             VALUES ($1, $2, $3, $4) \
             RETURNING id, user_id, label, created_at, last_used_at",
        )
        .bind(AppPasswordId::new().as_uuid())
        .bind(user_id.as_uuid())
        .bind(&label)
        .bind(hash.as_str())
        .fetch_one(self.db.pool())
        .await?;

        Ok((row.into_domain(), secret))
    }

    /// Verifies `username:secret` for pre-session HTTP Basic authentication
    /// (`WebDAV`). **Documented exception** to the invariant "every method
    /// that reads a user's data takes an `AuthContext`": no context exists
    /// yet at this point — exactly like `UserRepo::find_by_username` for
    /// session login. Only non-revoked app passwords enter the candidate
    /// list, so a revocation takes effect immediately without needing to
    /// invalidate any cache — there is none. A success updates
    /// `last_used_at` fire-and-forget, so the request path does not pay
    /// the cost of a write.
    ///
    /// # Errors
    /// `DbError::Connection` if the query fails.
    pub async fn verify(&self, username: &str, secret: &str) -> Result<Option<UserId>, DbError> {
        let Ok(password) = Password::parse(secret) else {
            return Ok(None);
        };

        let rows: Vec<(Uuid, Uuid, String)> = sqlx::query_as(
            "SELECT ap.id, ap.user_id, ap.secret_hash \
               FROM app_passwords ap \
               JOIN users u ON u.id = ap.user_id \
              WHERE lower(u.username) = lower($1) AND ap.revoked_at IS NULL",
        )
        .bind(username)
        .fetch_all(self.db.pool())
        .await?;

        for (id, user_id, secret_hash) in rows {
            let stored = PasswordHash::from_stored(secret_hash);
            if verify_password(&password, &stored) {
                let db = self.db.clone();
                tokio::spawn(async move {
                    let _ =
                        sqlx::query("UPDATE app_passwords SET last_used_at = now() WHERE id = $1")
                            .bind(id)
                            .execute(db.pool())
                            .await;
                });
                return Ok(Some(UserId::from_uuid(user_id)));
            }
        }

        Ok(None)
    }

    /// List of the authenticated user's non-revoked app passwords.
    ///
    /// # Errors
    /// `DbError::Forbidden` if the caller is not an authenticated user.
    pub async fn list(&self, ctx: &AuthContext) -> Result<Vec<AppPasswordSummary>, DbError> {
        let user_id = ctx.user_id().ok_or(DbError::Forbidden)?;
        let rows: Vec<SummaryRow> = sqlx::query_as(
            "SELECT id, user_id, label, created_at, last_used_at \
               FROM app_passwords \
              WHERE user_id = $1 AND revoked_at IS NULL \
              ORDER BY created_at DESC",
        )
        .bind(user_id.as_uuid())
        .fetch_all(self.db.pool())
        .await?;
        Ok(rows.into_iter().map(SummaryRow::into_domain).collect())
    }

    /// Immediate revocation: owner only, or an admin. An id belonging to
    /// another user returns `Forbidden`, never `NotFound` — otherwise the
    /// endpoint becomes an existence oracle (same rule as
    /// `UploadSessionRepo::load_owned`). Idempotent: a second revocation
    /// on the same id does not fail and does not touch `revoked_at` again.
    ///
    /// # Errors
    /// `DbError::Forbidden` if the caller does not own the id, or if the
    /// id does not exist and the caller is not admin. `DbError::NotFound`
    /// only for an admin requesting an id that truly does not exist.
    pub async fn revoke(&self, ctx: &AuthContext, id: AppPasswordId) -> Result<(), DbError> {
        let owner: Option<Uuid> =
            sqlx::query_scalar("SELECT user_id FROM app_passwords WHERE id = $1")
                .bind(id.as_uuid())
                .fetch_optional(self.db.pool())
                .await?;

        let Some(owner) = owner else {
            return Err(if ctx.is_admin() {
                DbError::NotFound
            } else {
                DbError::Forbidden
            });
        };

        if !ctx.is_admin() && ctx.user_id() != Some(UserId::from_uuid(owner)) {
            return Err(DbError::Forbidden);
        }

        sqlx::query(
            "UPDATE app_passwords SET revoked_at = now() WHERE id = $1 AND revoked_at IS NULL",
        )
        .bind(id.as_uuid())
        .execute(self.db.pool())
        .await?;
        Ok(())
    }
}
