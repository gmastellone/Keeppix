//! Long-running operations with progress and cancellation.
//!
//! The WebSocket channel stays what it already is: a notification
//! channel, not the source of truth. This table *is* the source of truth
//! — the `/ws` route reads it on every poll cycle (as it already does
//! with `change_log`), so a client reconnecting mid-operation sees the
//! current state on the first useful cycle, without needing a replay of
//! missed events.
//!
//! **Ruling: cancelling midway produces a partial success, not a
//! rollback.** `succeeded_asset_ids` accumulates what has already been
//! written to disk; `finish_cancelled` closes the state without clearing
//! it — this is exactly the `BulkOutcome` wrapper, read from here instead
//! of being built from scratch.

use keeppix_domain::{AssetId, AuthContext, OperationId, OperationKind, OperationStatus, UserId};
use uuid::Uuid;

use crate::{Db, DbError};

pub struct OperationsRepo<'a> {
    db: &'a Db,
}

#[derive(Debug, Clone)]
pub struct Operation {
    pub id: OperationId,
    pub kind: OperationKind,
    pub owner_id: UserId,
    pub status: OperationStatus,
    pub done: i64,
    pub total: Option<i64>,
    pub phase: String,
    pub succeeded: Vec<AssetId>,
}

#[derive(sqlx::FromRow)]
struct OperationRow {
    id: Uuid,
    kind: String,
    owner_id: Uuid,
    status: String,
    done: i64,
    total: Option<i64>,
    phase: String,
    succeeded_asset_ids: Vec<Uuid>,
}

impl OperationRow {
    fn into_domain(self) -> Result<Operation, DbError> {
        Ok(Operation {
            id: OperationId::from_uuid(self.id),
            kind: OperationKind::parse(&self.kind)
                .map_err(|e| crate::row::corrupted("operation kind", e))?,
            owner_id: UserId::from_uuid(self.owner_id),
            status: OperationStatus::parse(&self.status)
                .map_err(|e| crate::row::corrupted("operation status", e))?,
            done: self.done,
            total: self.total,
            phase: self.phase,
            succeeded: self
                .succeeded_asset_ids
                .into_iter()
                .map(AssetId::from_uuid)
                .collect(),
        })
    }
}

const COLUMNS: &str = "id, kind, owner_id, status, done, total, phase, succeeded_asset_ids";

impl<'a> OperationsRepo<'a> {
    #[must_use]
    pub const fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// Creates a new operation owned by the caller.
    ///
    /// # Errors
    /// `Forbidden` without an authenticated user; `Connection` on DB error.
    pub async fn create(
        &self,
        ctx: &AuthContext,
        kind: OperationKind,
    ) -> Result<Operation, DbError> {
        let owner = ctx.user_id().ok_or(DbError::Forbidden)?;
        let id = Uuid::now_v7();
        let row: OperationRow = sqlx::query_as(&format!(
            "INSERT INTO operations (id, kind, owner_id) VALUES ($1, $2, $3) \
             RETURNING {COLUMNS}"
        ))
        .bind(id)
        .bind(kind.as_str())
        .bind(owner.as_uuid())
        .fetch_one(self.db.pool())
        .await?;
        row.into_domain()
    }

    /// Creates an operation for a known owner without an `AuthContext`.
    /// Used by the background AI-analysis window (no HTTP user).
    /// Documented as an exception: the worker picks the owner (typically
    /// the first admin) and `list_running` filters by that `owner_id`.
    ///
    /// # Errors
    /// `Connection` on DB error.
    pub async fn create_for_owner(
        &self,
        owner: UserId,
        kind: OperationKind,
    ) -> Result<Operation, DbError> {
        let id = Uuid::now_v7();
        let row: OperationRow = sqlx::query_as(&format!(
            "INSERT INTO operations (id, kind, owner_id) VALUES ($1, $2, $3) \
             RETURNING {COLUMNS}"
        ))
        .bind(id)
        .bind(kind.as_str())
        .bind(owner.as_uuid())
        .fetch_one(self.db.pool())
        .await?;
        row.into_domain()
    }

    /// Fetches an operation. `Forbidden` — not `NotFound` — if it is not
    /// visible: otherwise the endpoint would become an existence oracle.
    ///
    /// # Errors
    /// `Forbidden` if the caller is neither owner nor admin; `Connection`
    /// on DB error.
    pub async fn find(&self, ctx: &AuthContext, id: OperationId) -> Result<Operation, DbError> {
        let row: Option<OperationRow> =
            sqlx::query_as(&format!("SELECT {COLUMNS} FROM operations WHERE id = $1"))
                .bind(id.as_uuid())
                .fetch_optional(self.db.pool())
                .await?;
        let Some(row) = row else {
            return Err(DbError::Forbidden);
        };
        let operation = row.into_domain()?;
        if ctx.is_admin() || ctx.user_id() == Some(operation.owner_id) {
            Ok(operation)
        } else {
            Err(DbError::Forbidden)
        }
    }

    /// Operations still `running` owned by the caller. Used by the
    /// WebSocket poll to emit `operation.progress`.
    ///
    /// # Errors
    /// `Connection` on DB error.
    pub async fn list_running(&self, ctx: &AuthContext) -> Result<Vec<Operation>, DbError> {
        let Some(user_id) = ctx.user_id() else {
            return Ok(Vec::new());
        };
        let rows: Vec<OperationRow> = sqlx::query_as(&format!(
            "SELECT {COLUMNS} FROM operations \
              WHERE owner_id = $1 AND status = 'running' \
              ORDER BY created_at"
        ))
        .bind(user_id.as_uuid())
        .fetch_all(self.db.pool())
        .await?;
        rows.into_iter().map(OperationRow::into_domain).collect()
    }

    /// Sets the expected total (`None` when it is not known in advance,
    /// like on the first scan of a library with no data yet). Called by
    /// the pipeline, not the user: no `AuthContext`, like `JobRepo::claim`.
    ///
    /// # Errors
    /// `Connection` on DB error.
    pub async fn set_total(&self, id: OperationId, total: Option<i64>) -> Result<(), DbError> {
        sqlx::query(
            "UPDATE operations SET total = $2, updated_at = now() \
              WHERE id = $1 AND status = 'running'",
        )
        .bind(id.as_uuid())
        .bind(total)
        .execute(self.db.pool())
        .await?;
        Ok(())
    }

    /// Updates the text phase shown over the WebSocket (e.g. `"scanning"`).
    /// Internal pipeline: no `AuthContext`.
    ///
    /// # Errors
    /// `Connection` on DB error.
    pub async fn set_phase(&self, id: OperationId, phase: &str) -> Result<(), DbError> {
        sqlx::query(
            "UPDATE operations SET phase = $2, updated_at = now() \
              WHERE id = $1 AND status = 'running'",
        )
        .bind(id.as_uuid())
        .bind(phase)
        .execute(self.db.pool())
        .await?;
        Ok(())
    }

    /// Records a successful element: increments `done` and appends it to
    /// the partial-success wrapper. Silent no-op if the operation is
    /// already concluded — the worker should have already stopped calling
    /// this, but a stray late write must not reopen a terminal state.
    ///
    /// Internal pipeline: no `AuthContext`, like `JobRepo::complete`.
    ///
    /// # Errors
    /// `Connection` on DB error.
    pub async fn record_success(&self, id: OperationId, asset_id: AssetId) -> Result<(), DbError> {
        sqlx::query(
            "UPDATE operations SET \
                 done = done + 1, \
                 succeeded_asset_ids = succeeded_asset_ids || $2::uuid, \
                 updated_at = now() \
              WHERE id = $1 AND status = 'running'",
        )
        .bind(id.as_uuid())
        .bind(asset_id.as_uuid())
        .execute(self.db.pool())
        .await?;
        Ok(())
    }

    /// Like [`Self::record_success`], but for an entire batch in a single
    /// statement: one network round trip instead of one per asset. No-op
    /// if the batch is empty or the operation is already concluded — same
    /// behavior as [`Self::record_success`].
    ///
    /// Internal pipeline: no `AuthContext`.
    ///
    /// # Errors
    /// `Connection` on DB error.
    pub async fn record_success_many(
        &self,
        id: OperationId,
        asset_ids: &[AssetId],
    ) -> Result<(), DbError> {
        if asset_ids.is_empty() {
            return Ok(());
        }
        let ids: Vec<Uuid> = asset_ids.iter().map(AssetId::as_uuid).collect();
        sqlx::query(
            "UPDATE operations SET \
                 done = done + array_length($2::uuid[], 1), \
                 succeeded_asset_ids = succeeded_asset_ids || $2::uuid[], \
                 updated_at = now() \
              WHERE id = $1 AND status = 'running'",
        )
        .bind(id.as_uuid())
        .bind(&ids)
        .execute(self.db.pool())
        .await?;
        Ok(())
    }

    /// `true` if the caller has requested cancellation. Queried by the
    /// worker between elements — never by the user directly, so no
    /// `AuthContext`.
    ///
    /// # Errors
    /// `Connection` on DB error.
    pub async fn is_cancel_requested(&self, id: OperationId) -> Result<bool, DbError> {
        let flag: Option<bool> =
            sqlx::query_scalar("SELECT cancel_requested FROM operations WHERE id = $1")
                .bind(id.as_uuid())
                .fetch_optional(self.db.pool())
                .await?;
        Ok(flag.unwrap_or(false))
    }

    /// Requests cancellation. Owner or admin only — otherwise anyone
    /// could interrupt another user's operation.
    ///
    /// # Errors
    /// `Forbidden` if the caller is neither owner nor admin; `Connection`
    /// on DB error.
    pub async fn request_cancel(&self, ctx: &AuthContext, id: OperationId) -> Result<(), DbError> {
        self.find(ctx, id).await?;
        sqlx::query("UPDATE operations SET cancel_requested = true WHERE id = $1")
            .bind(id.as_uuid())
            .execute(self.db.pool())
            .await?;
        Ok(())
    }

    /// Closes the operation as cancelled **without** clearing the partial
    /// result already accumulated. Internal pipeline.
    ///
    /// # Errors
    /// `Connection` on DB error.
    pub async fn finish_cancelled(&self, id: OperationId) -> Result<(), DbError> {
        sqlx::query(
            "UPDATE operations SET status = 'cancelled', updated_at = now() \
              WHERE id = $1 AND status = 'running'",
        )
        .bind(id.as_uuid())
        .execute(self.db.pool())
        .await?;
        Ok(())
    }

    /// Closes the operation as completed successfully. Internal pipeline.
    ///
    /// # Errors
    /// `Connection` on DB error.
    pub async fn finish_done(&self, id: OperationId) -> Result<(), DbError> {
        sqlx::query(
            "UPDATE operations SET status = 'done', updated_at = now() \
              WHERE id = $1 AND status = 'running'",
        )
        .bind(id.as_uuid())
        .execute(self.db.pool())
        .await?;
        Ok(())
    }

    /// Closes the operation as failed, with the partial result already
    /// accumulated (same spirit as [`Self::finish_cancelled`]: a
    /// mid-flight failure does not undo what already succeeded).
    /// `OperationStatus::Failed` existed from early on (`operation.rs`,
    /// already handled by `routes/ws.rs::drain_operations`) but no code
    /// path ever wrote it — a tracked job that failed (a worker error,
    /// not a cancellation) left the operation stuck on `running` forever,
    /// orphaned on the WebSocket. This was discovered while building the
    /// `BulkRename` job; the same gap remains open in
    /// `discover.rs`/`embed.rs`/`detect_faces.rs`
    /// (`LibraryScan`/`AiAnalysis`/`FaceDetection`), not addressed here —
    /// declared debt, not forgotten.
    ///
    /// # Errors
    /// `Connection` on DB error.
    pub async fn finish_failed(&self, id: OperationId) -> Result<(), DbError> {
        sqlx::query(
            "UPDATE operations SET status = 'failed', updated_at = now() \
              WHERE id = $1 AND status = 'running'",
        )
        .bind(id.as_uuid())
        .execute(self.db.pool())
        .await?;
        Ok(())
    }
}
