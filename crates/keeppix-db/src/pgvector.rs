//! Probe for the presence of pgvector (the `vector` extension) on the
//! connected Postgres.
//!
//! If it is missing, Keeppix **does not** refuse to start — AI features
//! stay disabled and the persisted status explains why, with the command
//! to run. The `0043_ai_embeddings_tags` migration enables `vector` and
//! creates the schema only when the package is installed; otherwise it is
//! a no-op.

use serde::{Deserialize, Serialize};

use crate::{Db, DbError, SettingsRepo};

/// SQL command to show the operator when pgvector is missing or not yet
/// created. The bundled image's migration runs it automatically; here it
/// only serves as a readable instruction for external Postgres instances.
pub const ENABLE_VECTOR_SQL: &str = "CREATE EXTENSION IF NOT EXISTS vector;";

/// Outcome of the pgvector probe at startup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PgVectorStatus {
    /// The package/extension is installed on the server (`pg_available_extensions`).
    pub available: bool,
    /// `CREATE EXTENSION vector` has already been run on this database.
    pub enabled: bool,
    /// English message for logs and the panel when AI features stay
    /// disabled. `None` if pgvector is usable (or will be after the
    /// migration runs).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// Command to run after installing the package on the server.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enable_command: Option<String>,
}

impl PgVectorStatus {
    pub const ENABLE_SQL: &'static str = ENABLE_VECTOR_SQL;

    /// Status when the extension is not installed on the Postgres server.
    #[must_use]
    pub fn missing() -> Self {
        Self {
            available: false,
            enabled: false,
            message: Some(format!(
                "AI features (semantic search and automatic tags) are disabled \
                 because the PostgreSQL `vector` extension (pgvector) is not \
                 installed on this database. Install pgvector for your Postgres \
                 major version, then run: {ENABLE_VECTOR_SQL}"
            )),
            enable_command: Some(ENABLE_VECTOR_SQL.to_owned()),
        }
    }

    /// Status when the extension is installed on the server.
    ///
    /// `enabled` distinguishes "package present" from "already `CREATE
    /// EXTENSION`". After the migration runs on the bundled image,
    /// `enabled` is `true`. On a Postgres where the package exists but
    /// the migration has not run yet (or was skipped), `enabled == false`
    /// remains legitimate.
    #[must_use]
    pub fn present(enabled: bool) -> Self {
        Self {
            available: true,
            enabled,
            message: None,
            enable_command: None,
        }
    }

    /// `true` when the absence of pgvector disables AI features.
    #[must_use]
    pub const fn ai_disabled(&self) -> bool {
        !self.available
    }
}

#[derive(sqlx::FromRow)]
struct ProbeRow {
    available: bool,
    enabled: bool,
}

/// Checks whether `vector` (pgvector) is installable or already active.
///
/// No `AuthContext`: this is an instance capability probe, run at startup
/// before any user session exists (same reason as `count` /
/// `create_bootstrap_admin` — it does not read a user's data).
///
/// # Errors
/// `DbError::Connection` if the query fails.
pub async fn probe_pgvector(db: &Db) -> Result<PgVectorStatus, DbError> {
    let row: ProbeRow = sqlx::query_as(
        "SELECT \
            EXISTS (SELECT 1 FROM pg_available_extensions WHERE name = 'vector') AS available, \
            EXISTS (SELECT 1 FROM pg_extension WHERE extname = 'vector') AS enabled",
    )
    .fetch_one(db.pool())
    .await?;

    Ok(if row.available {
        PgVectorStatus::present(row.enabled)
    } else {
        PgVectorStatus::missing()
    })
}

/// Writes the status to `system_settings` under the `pgvector` key, so the
/// panel can explain why the AI section is disabled without re-running
/// the probe on every request.
///
/// # Errors
/// Database.
pub async fn persist_pgvector_status(db: &Db) -> Result<PgVectorStatus, DbError> {
    let status = probe_pgvector(db).await?;
    let value = serde_json::to_value(&status)
        .map_err(|e| DbError::Corrupted(format!("pgvector status serialize: {e}")))?;
    SettingsRepo::new(db).put_json("pgvector", &value).await?;
    Ok(status)
}

/// Whether the AI schema (`faces`/`persons`/`tags`) can be assumed to
/// exist — for a hot per-request path (every plain-text search, every
/// album rule evaluation, every map cluster query) that only needs a
/// yes/no answer, not the full status object with its message/command.
///
/// Reads the value [`persist_pgvector_status`] already wrote at startup,
/// through [`SettingsRepo::get_json`]'s in-memory cache (`Db::
/// settings_cache`) — no query in the common case, unlike calling
/// [`probe_pgvector`] directly, which was the actual bug here: a live
/// `pg_available_extensions`/`pg_extension` catalog probe on every single
/// search request, multiplied by however many searches a page fires at
/// once (`PeopleView`'s one-search-per-card cover lookup, tens to
/// hundreds on a real library) — enough concurrent extra round trips
/// against a 10-connection pool to make the whole app feel slow, not just
/// search. Falls back to a live probe only if the cache is genuinely
/// empty (a fresh install racing its own startup write, or a manually
/// cleared `system_settings` row) — correct either way, just not the fast
/// path.
///
/// # Errors
/// Database, only on the (rare) fallback.
pub async fn ai_schema_available(db: &Db) -> Result<bool, DbError> {
    if let Some(value) = SettingsRepo::new(db).get_json("pgvector").await? {
        if let Ok(status) = serde_json::from_value::<PgVectorStatus>(value) {
            return Ok(status.available);
        }
    }
    Ok(probe_pgvector(db).await?.available)
}
