use keeppix_domain::UserId;
use serde::{Deserialize, Serialize};

use crate::{Db, DbError};

pub struct IdempotencyRepo<'a> {
    db: &'a Db,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestFingerprint {
    pub method: String,
    pub path: String,
    pub body_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedResponse {
    pub body: Option<serde_json::Value>,
    pub set_cookie: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredEnvelope {
    request: RequestFingerprint,
    response: CachedResponse,
}

#[derive(Debug, Clone)]
pub enum LookupResult {
    Missing,
    Replay {
        status: u16,
        response: CachedResponse,
    },
    Conflict,
}

#[derive(sqlx::FromRow)]
struct StoredRow {
    user_id: uuid::Uuid,
    response_status: i16,
    response_body: Option<serde_json::Value>,
}

impl<'a> IdempotencyRepo<'a> {
    #[must_use]
    pub const fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// # Errors
    /// `DbError::Connection` se la lettura fallisce; `DbError::Corrupted` se
    /// la riga salvata non contiene più un envelope JSON interpretabile.
    pub async fn lookup(
        &self,
        user_id: UserId,
        key: &str,
        fingerprint: &RequestFingerprint,
    ) -> Result<LookupResult, DbError> {
        let row: Option<StoredRow> = sqlx::query_as(
            "SELECT user_id, response_status, response_body \
               FROM idempotency_keys \
              WHERE key = $1",
        )
        .bind(key)
        .fetch_optional(self.db.pool())
        .await?;

        let Some(row) = row else {
            return Ok(LookupResult::Missing);
        };

        if row.user_id != user_id.as_uuid() {
            return Ok(LookupResult::Conflict);
        }

        let envelope: StoredEnvelope =
            serde_json::from_value(row.response_body.unwrap_or_default())
                .map_err(|e| DbError::Corrupted(format!("invalid idempotency payload: {e}")))?;

        if envelope.request != *fingerprint {
            return Ok(LookupResult::Conflict);
        }

        Ok(LookupResult::Replay {
            status: u16::try_from(row.response_status)
                .map_err(|e| DbError::Corrupted(format!("invalid idempotency status: {e}")))?,
            response: envelope.response,
        })
    }

    /// # Errors
    /// `DbError::Connection` se l'upsert fallisce; `DbError::Corrupted` se lo
    /// status HTTP non entra in `smallint` o l'envelope non è serializzabile.
    pub async fn store(
        &self,
        user_id: UserId,
        key: &str,
        fingerprint: RequestFingerprint,
        status: u16,
        response: CachedResponse,
    ) -> Result<(), DbError> {
        let envelope = serde_json::to_value(StoredEnvelope {
            request: fingerprint,
            response,
        })
        .map_err(|e| DbError::Corrupted(format!("cannot serialize idempotency payload: {e}")))?;

        sqlx::query(
            "INSERT INTO idempotency_keys (key, user_id, response_status, response_body) \
             VALUES ($1, $2, $3, $4) \
             ON CONFLICT (key) DO UPDATE \
                 SET user_id = EXCLUDED.user_id, \
                     response_status = EXCLUDED.response_status, \
                     response_body = EXCLUDED.response_body",
        )
        .bind(key)
        .bind(user_id.as_uuid())
        .bind(i16::try_from(status).map_err(|e| {
            DbError::Corrupted(format!("status {status} does not fit smallint: {e}"))
        })?)
        .bind(envelope)
        .execute(self.db.pool())
        .await?;

        Ok(())
    }

    /// Deletes keys older than `before`. Pipeline maintenance — no
    /// `AuthContext` (Task 8).
    ///
    /// # Errors
    /// `DbError::Connection` if the delete fails.
    pub async fn purge_older_than(
        &self,
        before: chrono::DateTime<chrono::Utc>,
    ) -> Result<u64, DbError> {
        let result = sqlx::query("DELETE FROM idempotency_keys WHERE created_at < $1")
            .bind(before)
            .execute(self.db.pool())
            .await?;
        Ok(result.rows_affected())
    }
}
