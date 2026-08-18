use chrono::{DateTime, Utc};
use keeppix_domain::AuthContext;
use uuid::Uuid;

use crate::{Db, DbError};

#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow)]
pub struct AuditEntry {
    pub id: i64,
    pub actor_id: Option<Uuid>,
    pub actor_kind: String,
    pub action: String,
    pub object_type: Option<String>,
    pub object_id: Option<Uuid>,
    pub detail: Option<serde_json::Value>,
    pub ip: Option<String>,
    pub at: DateTime<Utc>,
}

pub struct AuditRepo<'a> {
    db: &'a Db,
}

impl<'a> AuditRepo<'a> {
    #[must_use]
    pub const fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// Appends an entry to the audit log. Append-only: no update/delete.
    ///
    /// # Errors
    /// `DbError::Connection` on query failure.
    #[allow(clippy::too_many_arguments)]
    pub async fn append(
        &self,
        actor_id: Option<Uuid>,
        actor_kind: &str,
        action: &str,
        object_type: Option<&str>,
        object_id: Option<Uuid>,
        detail: Option<&serde_json::Value>,
        ip: Option<&str>,
    ) -> Result<i64, DbError> {
        let row: (i64,) = sqlx::query_as(
            "INSERT INTO audit_log (actor_id, actor_kind, action, object_type, object_id, detail, ip) \
             VALUES ($1, $2, $3, $4, $5, $6, $7::inet) RETURNING id",
        )
        .bind(actor_id)
        .bind(actor_kind)
        .bind(action)
        .bind(object_type)
        .bind(object_id)
        .bind(detail)
        .bind(ip)
        .fetch_one(self.db.pool())
        .await?;
        Ok(row.0)
    }

    /// Lists audit entries (most recent first). Admin-only.
    ///
    /// # Errors
    /// `DbError::Forbidden` if caller is not admin.
    /// `DbError::Connection` on query failure.
    pub async fn list(
        &self,
        ctx: &AuthContext,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<AuditEntry>, DbError> {
        if !ctx.is_admin() {
            return Err(DbError::Forbidden);
        }
        let rows: Vec<AuditEntry> = sqlx::query_as(
            "SELECT id, actor_id, actor_kind, action, object_type, object_id, \
             detail, ip::text as ip, at \
             FROM audit_log ORDER BY at DESC LIMIT $1 OFFSET $2",
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(self.db.pool())
        .await?;
        Ok(rows)
    }
}
