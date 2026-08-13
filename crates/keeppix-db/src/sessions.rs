use std::time::Duration;

use keeppix_domain::{AuthContext, SessionToken, SystemRole, UserId};
use sqlx::Row;
use uuid::Uuid;

use crate::{Db, DbError};

pub struct SessionRepo<'a> {
    db: &'a Db,
}

impl<'a> SessionRepo<'a> {
    #[must_use]
    pub const fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// Apre una nuova famiglia di sessione. Il token in chiaro è restituito
    /// una sola volta: nel database resta solo il digest.
    ///
    /// # Errors
    /// `DbError::Connection` se l'inserimento fallisce.
    pub async fn create(
        &self,
        user_id: UserId,
        ttl: Duration,
        user_agent: Option<&str>,
    ) -> Result<SessionToken, DbError> {
        let token = SessionToken::generate();
        let id = Uuid::now_v7();

        sqlx::query(
            "INSERT INTO sessions \
                 (id, family_id, user_id, refresh_token_hash, user_agent, expires_at) \
             VALUES ($1, $1, $2, $3, $4, now() + $5::interval)",
        )
        .bind(id)
        .bind(user_id.as_uuid())
        .bind(token.digest().as_slice())
        .bind(user_agent)
        .bind(interval(ttl))
        .execute(self.db.pool())
        .await?;

        Ok(token)
    }

    /// # Errors
    /// `DbError::NotFound` se il token è sconosciuto, scaduto, consumato,
    /// revocato, oppure se l'utente è disabilitato.
    pub async fn authenticate(&self, token: &SessionToken) -> Result<AuthContext, DbError> {
        let row = sqlx::query(
            "SELECT u.id AS user_id, u.role \
               FROM sessions s JOIN users u ON u.id = s.user_id \
              WHERE s.refresh_token_hash = $1 \
                AND s.consumed_at IS NULL \
                AND s.revoked_at IS NULL \
                AND s.expires_at > now() \
                AND u.disabled_at IS NULL",
        )
        .bind(token.digest().as_slice())
        .fetch_optional(self.db.pool())
        .await?
        .ok_or(DbError::NotFound)?;

        let user_id: Uuid = row.try_get("user_id")?;
        let role: String = row.try_get("role")?;
        let role = match role.as_str() {
            "admin" => SystemRole::Admin,
            _ => SystemRole::User,
        };

        Ok(AuthContext::user(UserId::from_uuid(user_id), role))
    }

    /// Ruota il token. Se quello presentato risulta **già consumato**, l'unica
    /// spiegazione è che una copia sia in mano a qualcun altro: si revoca
    /// l'intera famiglia e si costringe a un nuovo login.
    ///
    /// # Errors
    /// `DbError::Forbidden` in caso di riuso rilevato; `DbError::NotFound` se
    /// il token non esiste o è scaduto.
    pub async fn rotate(
        &self,
        token: &SessionToken,
        ttl: Duration,
    ) -> Result<SessionToken, DbError> {
        let mut tx = self.db.pool().begin().await?;

        let row = sqlx::query(
            "SELECT id, family_id, user_id, consumed_at, revoked_at, expires_at \
               FROM sessions WHERE refresh_token_hash = $1 FOR UPDATE",
        )
        .bind(token.digest().as_slice())
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(DbError::NotFound)?;

        let family_id: Uuid = row.try_get("family_id")?;
        let consumed: Option<chrono::DateTime<chrono::Utc>> = row.try_get("consumed_at")?;
        let revoked: Option<chrono::DateTime<chrono::Utc>> = row.try_get("revoked_at")?;

        if consumed.is_some() {
            sqlx::query(
                "UPDATE sessions SET revoked_at = now() \
                  WHERE family_id = $1 AND revoked_at IS NULL",
            )
            .bind(family_id)
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            return Err(DbError::Forbidden);
        }

        if revoked.is_some() {
            return Err(DbError::NotFound);
        }

        let expires_at: chrono::DateTime<chrono::Utc> = row.try_get("expires_at")?;
        if expires_at <= chrono::Utc::now() {
            return Err(DbError::NotFound);
        }

        let parent_id: Uuid = row.try_get("id")?;
        let user_id: Uuid = row.try_get("user_id")?;

        sqlx::query("UPDATE sessions SET consumed_at = now() WHERE id = $1")
            .bind(parent_id)
            .execute(&mut *tx)
            .await?;

        let next = SessionToken::generate();
        sqlx::query(
            "INSERT INTO sessions \
                 (id, family_id, user_id, refresh_token_hash, parent_id, expires_at) \
             VALUES ($1, $2, $3, $4, $5, now() + $6::interval)",
        )
        .bind(Uuid::now_v7())
        .bind(family_id)
        .bind(user_id)
        .bind(next.digest().as_slice())
        .bind(parent_id)
        .bind(interval(ttl))
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(next)
    }

    /// # Errors
    /// `DbError::Connection` se l'aggiornamento fallisce.
    pub async fn revoke(&self, token: &SessionToken) -> Result<(), DbError> {
        sqlx::query(
            "UPDATE sessions SET revoked_at = now() \
              WHERE refresh_token_hash = $1 AND revoked_at IS NULL",
        )
        .bind(token.digest().as_slice())
        .execute(self.db.pool())
        .await?;
        Ok(())
    }

    /// # Errors
    /// `DbError::Connection` se la cancellazione fallisce.
    pub async fn purge_expired(&self) -> Result<u64, DbError> {
        let result = sqlx::query("DELETE FROM sessions WHERE expires_at <= now()")
            .execute(self.db.pool())
            .await?;
        Ok(result.rows_affected())
    }
}

/// Postgres non accetta un `Duration` di Rust: si passa un intervallo in secondi.
fn interval(ttl: Duration) -> String {
    format!("{} seconds", ttl.as_secs())
}
