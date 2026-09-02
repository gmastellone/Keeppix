use std::time::Duration;

use chrono::{DateTime, Utc};
use keeppix_domain::{AuthContext, Job, JobKind, JobPriority, JobStatus};
use uuid::Uuid;

use crate::visibility::VisibilityScope;
use crate::{Db, DbError};

/// Ingestion job queue. `claim` / `enqueue` / `reap` are called by the
/// worker: no `AuthContext`. `promote` is called by the user from the
/// viewport, so it filters by visibility.
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

    /// Enqueues a job. With a `dedup_key`, a second call while the first
    /// is still `pending` or `running` returns the existing row.
    ///
    /// # Errors
    /// `DbError::Connection` if the query fails; `DbError::Corrupted` if
    /// an existing row cannot be mapped.
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

    /// Like [`Self::enqueue`] with a `dedup_key`, but for an entire batch
    /// in a single statement: the existing method does one network round
    /// trip per file (`INSERT` plus the possible dedup re-read `SELECT`);
    /// this one does a single round trip per batch. The caller does not
    /// get the jobs back: the scanner does not need them, it finds them
    /// already queued with the same `dedup_key` on the next read.
    ///
    /// The payload travels as `text[]` (JSON serialized in Rust, then
    /// `::jsonb` in SQL) instead of `jsonb[]`: `serde_json::Value` has no
    /// direct `PgHasArrayType` in sqlx, while `text[]` is already a proven
    /// pattern elsewhere in this crate (`geo.rs`, `overrides.rs`).
    ///
    /// # Errors
    /// `Connection` if the insert fails.
    pub async fn enqueue_many(
        &self,
        kind: JobKind,
        items: &[(serde_json::Value, String)],
        priority: JobPriority,
    ) -> Result<(), DbError> {
        if items.is_empty() {
            return Ok(());
        }
        let kinds: Vec<&str> = items.iter().map(|_| kind.as_str()).collect();
        let payloads: Result<Vec<String>, serde_json::Error> = items
            .iter()
            .map(|(payload, _)| serde_json::to_string(payload))
            .collect();
        let payloads = payloads.map_err(|e| DbError::Corrupted(format!("payload: {e}")))?;
        let priorities: Vec<i16> = items.iter().map(|_| priority.as_i16()).collect();
        let dedup_keys: Vec<&str> = items.iter().map(|(_, key)| key.as_str()).collect();

        sqlx::query(
            "INSERT INTO jobs (kind, payload, priority, dedup_key) \
             SELECT kind, payload_text::jsonb, priority, dedup_key \
               FROM UNNEST($1::text[], $2::text[], $3::smallint[], $4::text[]) \
                 AS t(kind, payload_text, priority, dedup_key) \
             ON CONFLICT (dedup_key) \
             WHERE dedup_key IS NOT NULL AND status IN ('pending', 'running') \
             DO NOTHING",
        )
        .bind(kinds)
        .bind(payloads)
        .bind(priorities)
        .bind(dedup_keys)
        .execute(self.db.pool())
        .await?;
        Ok(())
    }

    /// Like [`Self::enqueue`], but with an explicit `run_after` (rechecking
    /// files still in flight without sleeping in the worker).
    ///
    /// # Errors
    /// Same as [`Self::enqueue`].
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

    /// Takes the next runnable job, or `None` if the queue is empty for
    /// this priority ceiling.
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
    /// `DbError::NotFound` if the id is not `running`.
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

    /// Retries with backoff, or marks `failed` if attempts are exhausted.
    ///
    /// # Errors
    /// `DbError::NotFound` if the id is not `running`.
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

    /// Permanently retires active jobs with a dedup key.
    ///
    /// Used by explicit cancellations: leaving a job `running` would let
    /// a later enqueue reuse the old writer.
    ///
    /// # Errors
    /// `DbError::Connection` if the query fails.
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

    /// Renews the lease of a job still owned by the same worker.
    ///
    /// # Errors
    /// `DbError::Connection` if the query fails.
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

    /// Re-queues `running` jobs whose lock is older than `older_than`.
    ///
    /// # Errors
    /// `DbError::Connection` if the query fails.
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

    /// Immediately re-queues all `running` jobs of one kind.
    ///
    /// Must only be used at startup, before creating the workers: any
    /// lock still present necessarily belongs to the previous process.
    ///
    /// # Errors
    /// `DbError::Connection` if the query fails.
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

    /// Recreates missing jobs for regions left `downloading` after a crash.
    ///
    /// Branches on `map_regions.acquisition`: a row created by the manual
    /// URL flow (`RegionRepo::begin_download`) resumes as
    /// `download_map_region`, one created by the catalog/`pmtiles extract`
    /// flow (`RegionRepo::begin_extraction`) resumes as
    /// `extract_map_region` — re-queuing an extraction row as a raw HTTP
    /// download would fetch its placeholder `source_url` byte-for-byte and
    /// always fail, since that URL was never a downloadable file.
    ///
    /// Does not take an `AuthContext`: this is an internal pipeline repair.
    ///
    /// # Errors
    /// `DbError::Connection` if the query fails.
    pub async fn enqueue_missing_region_downloads(&self) -> Result<u64, DbError> {
        let result = sqlx::query(
            "INSERT INTO jobs (kind, payload, priority, dedup_key) \
             SELECT CASE r.acquisition \
                        WHEN 'extract' THEN 'extract_map_region' \
                        ELSE 'download_map_region' \
                    END, \
                    jsonb_build_object( \
                        'region_id', r.id, \
                        'download_generation', r.download_generation::text, \
                        'file_path', r.file_path \
                    ), 1, \
                    CASE r.acquisition \
                        WHEN 'extract' THEN 'map-region-extract:' \
                        ELSE 'map-region:' \
                    END || r.id || ':' || r.download_generation::text \
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

    /// Raises the priority (lower number) of the listed `pending` jobs.
    /// Never lowers a job that is already more urgent — `LEAST`, not an
    /// overwrite. A non-admin can only promote `derive:{hash}` jobs for
    /// assets they can see.
    ///
    /// # Errors
    /// `DbError::Connection` if the query fails.
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

    /// Status of the active `discover_library` job (or the last failed
    /// one) for a library. Called by the status route after the
    /// visibility check.
    ///
    /// Does not take an `AuthContext`: the caller has already authorized.
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

    /// Jobs of `kind` completed successfully after `since_id`, in id
    /// order. Pipeline/notification path (`asset.derivative.ready` over
    /// the WebSocket) — no `AuthContext`, like
    /// [`Self::discover_status_for_library`]: visibility on the asset
    /// involved must be applied by the caller (`AssetRepo::filter_visible`),
    /// exactly as the Problems page already does with
    /// [`Self::discover_status_for_library`].
    ///
    /// # Errors
    /// `Connection` / `Corrupted`.
    pub async fn list_recently_done(
        &self,
        kind: JobKind,
        since_id: i64,
        limit: i64,
    ) -> Result<Vec<Job>, DbError> {
        let rows: Vec<JobRow> = sqlx::query_as(&format!(
            "SELECT {COLUMNS} FROM jobs \
              WHERE kind = $1 AND status = 'done' AND id > $2 \
              ORDER BY id \
              LIMIT $3"
        ))
        .bind(kind.as_str())
        .bind(since_id)
        .bind(limit.clamp(1, 500))
        .fetch_all(self.db.pool())
        .await?;
        rows.into_iter().map(JobRow::into_domain).collect()
    }

    /// Highest id among `kind` jobs already `done` — initializes the
    /// cursor for a client connecting to the WebSocket right now, so it
    /// does not see transcodes that finished before it connected. No
    /// `AuthContext`: same reason as [`Self::list_recently_done`].
    ///
    /// # Errors
    /// `Connection` if the query fails.
    pub async fn max_done_id(&self, kind: JobKind) -> Result<i64, DbError> {
        let id: Option<i64> =
            sqlx::query_scalar("SELECT max(id) FROM jobs WHERE kind = $1 AND status = 'done'")
                .bind(kind.as_str())
                .fetch_one(self.db.pool())
                .await?;
        Ok(id.unwrap_or(0))
    }

    /// How many rows (any status) share this `dedup_key`. Used by the
    /// derive retry logic: the unique constraint only applies to
    /// pending/running, so `done` rows accumulate and are the ceiling.
    ///
    /// Does not take an `AuthContext`: this is the pipeline.
    ///
    /// # Errors
    /// `Connection` if the query fails.
    pub async fn count_for_dedup_key(&self, dedup_key: &str) -> Result<i64, DbError> {
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM jobs WHERE dedup_key = $1")
            .bind(dedup_key)
            .fetch_one(self.db.pool())
            .await?;
        Ok(n)
    }

    /// Deletes `done` jobs older than `before`. Pipeline maintenance —
    /// no `AuthContext` (same class as `SessionRepo::purge_expired`).
    /// Uses `created_at` because the jobs table has no `completed_at`.
    ///
    /// # Errors
    /// `Connection` if the delete fails.
    pub async fn delete_done_older_than(&self, before: DateTime<Utc>) -> Result<u64, DbError> {
        let result = sqlx::query("DELETE FROM jobs WHERE status = 'done' AND created_at < $1")
            .bind(before)
            .execute(self.db.pool())
            .await?;
        Ok(result.rows_affected())
    }
}
