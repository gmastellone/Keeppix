use std::time::Duration;

use chrono::{DateTime, Utc};
use keeppix_domain::{Job, JobKind, JobPriority, JobStatus};
use uuid::Uuid;

use crate::{Db, DbError};

/// Coda dei job di ingestione. La chiama il worker, non un utente: non c'è
/// `AuthContext`. I permessi si applicano quando il job tocca gli asset.
pub struct JobRepo<'a> {
    db: &'a Db,
}

#[derive(sqlx::FromRow)]
struct JobRow {
    id: i64,
    kind: String,
    payload: serde_json::Value,
    priority: i16,
    status: String,
    attempts: i32,
    max_attempts: i32,
    last_error: Option<String>,
    run_after: DateTime<Utc>,
    locked_by: Option<Uuid>,
    dedup_key: Option<String>,
}

impl JobRow {
    fn into_domain(self) -> Result<Job, DbError> {
        Ok(Job {
            id: self.id,
            kind: JobKind::parse(&self.kind).map_err(|e| crate::row::corrupted("job kind", e))?,
            payload: self.payload,
            priority: JobPriority::from_i16(self.priority)
                .map_err(|e| crate::row::corrupted("job priority", e))?,
            status: JobStatus::parse(&self.status)
                .map_err(|e| crate::row::corrupted("job status", e))?,
            attempts: self.attempts,
            max_attempts: self.max_attempts,
            last_error: self.last_error,
            run_after: self.run_after,
            locked_by: self.locked_by,
            dedup_key: self.dedup_key,
        })
    }
}

const COLUMNS: &str = "id, kind, payload, priority, status, attempts, max_attempts, \
                       last_error, run_after, locked_by, dedup_key";

impl<'a> JobRepo<'a> {
    #[must_use]
    pub const fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// Accoda un job. Con `dedup_key`, una seconda chiamata mentre il primo
    /// è ancora `pending` o `running` restituisce la riga esistente.
    ///
    /// # Errors
    /// `DbError::Connection` se la query fallisce; `DbError::Corrupted` se
    /// una riga esistente non è mappabile.
    pub async fn enqueue(
        &self,
        kind: JobKind,
        payload: serde_json::Value,
        priority: JobPriority,
        dedup_key: Option<&str>,
    ) -> Result<Job, DbError> {
        if let Some(key) = dedup_key {
            sqlx::query(
                "INSERT INTO jobs (kind, payload, priority, dedup_key) \
                 VALUES ($1, $2, $3, $4) \
                 ON CONFLICT (dedup_key) \
                 WHERE dedup_key IS NOT NULL AND status IN ('pending', 'running') \
                 DO NOTHING",
            )
            .bind(kind.as_str())
            .bind(&payload)
            .bind(priority.as_i16())
            .bind(key)
            .execute(self.db.pool())
            .await?;

            let row: JobRow = sqlx::query_as(&format!(
                "SELECT {COLUMNS} FROM jobs \
                  WHERE dedup_key = $1 AND status IN ('pending', 'running') \
                  ORDER BY id LIMIT 1"
            ))
            .bind(key)
            .fetch_one(self.db.pool())
            .await?;
            return row.into_domain();
        }

        let row: JobRow = sqlx::query_as(&format!(
            "INSERT INTO jobs (kind, payload, priority) \
             VALUES ($1, $2, $3) \
             RETURNING {COLUMNS}"
        ))
        .bind(kind.as_str())
        .bind(&payload)
        .bind(priority.as_i16())
        .fetch_one(self.db.pool())
        .await?;
        row.into_domain()
    }

    /// Prende il prossimo job eseguibile, o `None` se la coda è vuota per
    /// questo tetto di priorità.
    ///
    /// # Errors
    /// `DbError::Connection` / `Corrupted`.
    pub async fn claim(
        &self,
        worker_id: Uuid,
        max_priority: JobPriority,
    ) -> Result<Option<Job>, DbError> {
        let row: Option<JobRow> = sqlx::query_as(&format!(
            "UPDATE jobs SET \
                 status = 'running', \
                 locked_by = $1, \
                 locked_at = now(), \
                 attempts = attempts + 1 \
             WHERE id = ( \
                 SELECT id FROM jobs \
                  WHERE status = 'pending' \
                    AND run_after <= now() \
                    AND priority <= $2 \
                  ORDER BY priority, run_after, id \
                  FOR UPDATE SKIP LOCKED \
                  LIMIT 1 \
             ) \
             RETURNING {COLUMNS}"
        ))
        .bind(worker_id)
        .bind(max_priority.as_i16())
        .fetch_optional(self.db.pool())
        .await?;

        row.map(JobRow::into_domain).transpose()
    }

    /// # Errors
    /// `DbError::NotFound` se l'id non è `running`.
    pub async fn complete(&self, id: i64) -> Result<(), DbError> {
        let result = sqlx::query(
            "UPDATE jobs SET status = 'done', locked_by = NULL, locked_at = NULL \
              WHERE id = $1 AND status = 'running'",
        )
        .bind(id)
        .execute(self.db.pool())
        .await?;
        if result.rows_affected() == 0 {
            return Err(DbError::NotFound);
        }
        Ok(())
    }

    /// Ritenta con backoff, o marca `failed` se i tentativi sono esauriti.
    ///
    /// # Errors
    /// `DbError::NotFound` se l'id non è `running`.
    pub async fn fail(&self, id: i64, error: &str) -> Result<(), DbError> {
        let result = sqlx::query(
            "UPDATE jobs SET \
                 status = CASE WHEN attempts >= max_attempts THEN 'failed' ELSE 'pending' END, \
                 last_error = $2, \
                 locked_by = NULL, \
                 locked_at = NULL, \
                 run_after = CASE \
                     WHEN attempts >= max_attempts THEN run_after \
                     ELSE now() \
                          + (LEAST(POWER(2, attempts), 300.0) * INTERVAL '1 second') \
                          + (random() * INTERVAL '1 second') \
                 END \
             WHERE id = $1 AND status = 'running'",
        )
        .bind(id)
        .bind(error)
        .execute(self.db.pool())
        .await?;
        if result.rows_affected() == 0 {
            return Err(DbError::NotFound);
        }
        Ok(())
    }

    /// Rimette in coda i job `running` il cui lock è più vecchio di `older_than`.
    ///
    /// # Errors
    /// `DbError::Connection` se la query fallisce.
    pub async fn reap_stale(&self, older_than: Duration) -> Result<u64, DbError> {
        let secs = i32::try_from(older_than.as_secs()).unwrap_or(i32::MAX);
        let result = sqlx::query(
            "UPDATE jobs SET status = 'pending', locked_by = NULL, locked_at = NULL \
              WHERE status = 'running' AND locked_at < now() - make_interval(secs => $1)",
        )
        .bind(secs)
        .execute(self.db.pool())
        .await?;
        Ok(result.rows_affected())
    }

    /// Alza la priorità (numero più basso) dei job `pending` elencati.
    /// Non abbassa mai un job già più urgente — `LEAST`, non un overwrite.
    ///
    /// # Errors
    /// `DbError::Connection` se la query fallisce.
    pub async fn promote(
        &self,
        dedup_keys: &[String],
        priority: JobPriority,
    ) -> Result<u64, DbError> {
        let result = sqlx::query(
            "UPDATE jobs SET priority = LEAST(priority, $1) \
              WHERE dedup_key = ANY($2) AND status = 'pending'",
        )
        .bind(priority.as_i16())
        .bind(dedup_keys)
        .execute(self.db.pool())
        .await?;
        Ok(result.rows_affected())
    }
}
