use std::time::Duration;

use chrono::{DateTime, Utc};
use keeppix_domain::{AuthContext, Job, JobKind, JobPriority, JobStatus};
use uuid::Uuid;

use crate::visibility::VisibilityScope;
use crate::{Db, DbError};

/// Coda dei job di ingestione. `claim` / `enqueue` / `reap` li chiama il
/// worker: niente `AuthContext`. `promote` lo chiama l'utente dal viewport,
/// quindi filtra con la visibilità.
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

    /// Come [`Self::enqueue`], ma con `run_after` esplicito (ricontrollo
    /// di file ancora in arrivo senza dormire nel worker).
    ///
    /// # Errors
    /// Come [`Self::enqueue`].
    pub async fn enqueue_after(
        &self,
        kind: JobKind,
        payload: serde_json::Value,
        priority: JobPriority,
        dedup_key: Option<&str>,
        run_after: DateTime<Utc>,
    ) -> Result<Job, DbError> {
        if let Some(key) = dedup_key {
            sqlx::query(
                "INSERT INTO jobs (kind, payload, priority, dedup_key, run_after) \
                 VALUES ($1, $2, $3, $4, $5) \
                 ON CONFLICT (dedup_key) \
                 WHERE dedup_key IS NOT NULL AND status IN ('pending', 'running') \
                 DO NOTHING",
            )
            .bind(kind.as_str())
            .bind(&payload)
            .bind(priority.as_i16())
            .bind(key)
            .bind(run_after)
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
            "INSERT INTO jobs (kind, payload, priority, run_after) \
             VALUES ($1, $2, $3, $4) \
             RETURNING {COLUMNS}"
        ))
        .bind(kind.as_str())
        .bind(&payload)
        .bind(priority.as_i16())
        .bind(run_after)
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

    /// Ritira definitivamente i job attivi con una chiave di deduplica.
    ///
    /// Serve alle cancellazioni esplicite: lasciare un job `running`
    /// permetterebbe a un enqueue successivo di riusare il vecchio writer.
    ///
    /// # Errors
    /// `DbError::Connection` se la query fallisce.
    pub async fn retire_active(&self, dedup_key: &str, error: &str) -> Result<u64, DbError> {
        let result = sqlx::query(
            "UPDATE jobs SET \
                 status = 'failed', last_error = $2, \
                 locked_by = NULL, locked_at = NULL \
              WHERE dedup_key = $1 AND status IN ('pending', 'running')",
        )
        .bind(dedup_key)
        .bind(error)
        .execute(self.db.pool())
        .await?;
        Ok(result.rows_affected())
    }

    /// Rinnova il lease di un job ancora posseduto dallo stesso worker.
    ///
    /// # Errors
    /// `DbError::Connection` se la query fallisce.
    pub async fn renew_lock(&self, id: i64, worker_id: Uuid) -> Result<bool, DbError> {
        let result = sqlx::query(
            "UPDATE jobs SET locked_at = now() \
              WHERE id = $1 AND status = 'running' AND locked_by = $2",
        )
        .bind(id)
        .bind(worker_id)
        .execute(self.db.pool())
        .await?;
        Ok(result.rows_affected() == 1)
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

    /// Rimette subito in coda tutti i job `running` di un tipo.
    ///
    /// Va usato solo all'avvio, prima di creare i worker: ogni lock ancora
    /// presente appartiene necessariamente al processo precedente.
    ///
    /// # Errors
    /// `DbError::Connection` se la query fallisce.
    pub async fn reset_running(&self, kind: JobKind) -> Result<u64, DbError> {
        let result = sqlx::query(
            "UPDATE jobs SET status = 'pending', locked_by = NULL, locked_at = NULL \
              WHERE status = 'running' AND kind = $1",
        )
        .bind(kind.as_str())
        .execute(self.db.pool())
        .await?;
        Ok(result.rows_affected())
    }

    /// Ricrea i job mancanti per regioni rimaste `downloading` dopo un crash.
    ///
    /// Non prende `AuthContext`: è una riparazione interna della pipeline.
    ///
    /// # Errors
    /// `DbError::Connection` se la query fallisce.
    pub async fn enqueue_missing_region_downloads(&self) -> Result<u64, DbError> {
        let result = sqlx::query(
            "INSERT INTO jobs (kind, payload, priority, dedup_key) \
             SELECT 'download_map_region', jsonb_build_object('region_id', r.id), 1, \
                    'map-region:' || r.id \
               FROM map_regions r \
              WHERE r.status = 'downloading' AND NOT r.cancel_requested \
             ON CONFLICT (dedup_key) \
             WHERE dedup_key IS NOT NULL AND status IN ('pending', 'running') \
             DO NOTHING",
        )
        .execute(self.db.pool())
        .await?;
        Ok(result.rows_affected())
    }

    /// Alza la priorità (numero più basso) dei job `pending` elencati.
    /// Non abbassa mai un job già più urgente — `LEAST`, non un overwrite.
    /// Un non-admin promuove solo `derive:{hash}` di asset che può vedere.
    ///
    /// # Errors
    /// `DbError::Connection` se la query fallisce.
    pub async fn promote(
        &self,
        ctx: &AuthContext,
        dedup_keys: &[String],
        priority: JobPriority,
    ) -> Result<u64, DbError> {
        let scope = VisibilityScope::resolve(self.db, ctx).await?;
        let filter = scope.filter("f.path", "f.library_id", "a.id", 3);
        let result = sqlx::query(
            "UPDATE jobs j SET priority = LEAST(priority, $1) \
              WHERE j.dedup_key = ANY($2) AND j.status = 'pending' \
                AND ($3::uuid[] IS NULL OR EXISTS ( \
                  SELECT 1 FROM assets a \
                  JOIN folders f ON f.id = a.folder_id \
                  JOIN folders vis_g ON vis_g.id = ANY($3::uuid[]) \
                  WHERE a.content_hash IS NOT NULL \
                    AND j.dedup_key = 'derive:' || encode(a.content_hash, 'hex') \
                    AND f.library_id = vis_g.library_id \
                    AND f.path <@ vis_g.path \
                    AND NOT EXISTS ( \
                      SELECT 1 FROM folders vis_h \
                       WHERE vis_h.id = ANY($4::uuid[]) \
                         AND f.library_id = vis_h.library_id \
                         AND f.path <@ vis_h.path \
                    ) \
                ))",
        )
        .bind(priority.as_i16())
        .bind(dedup_keys)
        .bind(filter.bind())
        .bind(filter.holes())
        .bind(filter.assets())
        .execute(self.db.pool())
        .await?;
        Ok(result.rows_affected())
    }

    /// Stato del job `discover_library` attivo (o l'ultimo fallito) per una
    /// libreria. Lo chiama la rotta di stato dopo il controllo di visibilità.
    ///
    /// Non prende un `AuthContext`: il chiamante ha già autorizzato.
    ///
    /// # Errors
    /// `Connection` / `Corrupted`.
    pub async fn discover_status_for_library(
        &self,
        library_id: keeppix_domain::LibraryId,
    ) -> Result<Option<Job>, DbError> {
        let key = format!("discover:{library_id}");
        let row: Option<JobRow> = sqlx::query_as(&format!(
            "SELECT {COLUMNS} FROM jobs \
              WHERE dedup_key = $1 \
              ORDER BY \
                CASE status \
                  WHEN 'running' THEN 0 \
                  WHEN 'pending' THEN 1 \
                  WHEN 'failed' THEN 2 \
                  ELSE 3 \
                END, \
                id DESC \
              LIMIT 1"
        ))
        .bind(&key)
        .fetch_optional(self.db.pool())
        .await?;
        row.map(JobRow::into_domain).transpose()
    }

    /// Quante righe (qualsiasi stato) condividono questa `dedup_key`.
    /// Lo usa il ritentativo dei derive: il vincolo unique vale solo su
    /// pending/running, quindi i `done` si accumulano e sono il tetto.
    ///
    /// Non prende un `AuthContext`: è la pipeline.
    ///
    /// # Errors
    /// `Connection` se la query fallisce.
    pub async fn count_for_dedup_key(&self, dedup_key: &str) -> Result<i64, DbError> {
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM jobs WHERE dedup_key = $1")
            .bind(dedup_key)
            .fetch_one(self.db.pool())
            .await?;
        Ok(n)
    }
}
