use chrono::{DateTime, Utc};
use keeppix_domain::AuthContext;
use uuid::Uuid;

use crate::{Db, DbError};

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct GuestUploadRow {
    pub id: Uuid,
    pub asset_id: Uuid,
    pub share_link_id: Uuid,
    pub filename: String,
    pub size_bytes: i64,
    pub uploaded_at: DateTime<Utc>,
    pub approved_at: Option<DateTime<Utc>>,
    pub rejected_at: Option<DateTime<Utc>>,
    pub reviewed_by: Option<Uuid>,
}

pub struct GuestUploadRepo<'a> {
    db: &'a Db,
}

impl<'a> GuestUploadRepo<'a> {
    #[must_use]
    pub const fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// Queues a guest upload for review.
    ///
    /// # Errors
    /// `DbError::Connection` on query failure.
    pub async fn queue(
        &self,
        asset_id: Uuid,
        share_link_id: Uuid,
        filename: &str,
        size_bytes: i64,
    ) -> Result<Uuid, DbError> {
        let id = Uuid::now_v7();
        sqlx::query(
            "INSERT INTO guest_upload_queue (id, asset_id, share_link_id, filename, size_bytes) \
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(id)
        .bind(asset_id)
        .bind(share_link_id)
        .bind(filename)
        .bind(size_bytes)
        .execute(self.db.pool())
        .await?;
        Ok(id)
    }

    /// Lists pending uploads for a given link (owner review screen).
    ///
    /// # Errors
    /// `DbError::Forbidden` if caller is not a user.
    /// `DbError::Connection` on query failure.
    pub async fn list_pending(
        &self,
        ctx: &AuthContext,
        share_link_id: Uuid,
    ) -> Result<Vec<GuestUploadRow>, DbError> {
        if ctx.user_id().is_none() {
            return Err(DbError::Forbidden);
        }
        let rows: Vec<GuestUploadRow> = sqlx::query_as(
            "SELECT id, asset_id, share_link_id, filename, size_bytes, uploaded_at, \
             approved_at, rejected_at, reviewed_by \
             FROM guest_upload_queue \
             WHERE share_link_id = $1 AND approved_at IS NULL AND rejected_at IS NULL \
             ORDER BY uploaded_at ASC",
        )
        .bind(share_link_id)
        .fetch_all(self.db.pool())
        .await?;
        Ok(rows)
    }

    /// Approves an upload. The asset stays; the flag is cleared by the caller.
    ///
    /// # Errors
    /// `DbError::Connection` on query failure.
    pub async fn approve(&self, ctx: &AuthContext, upload_id: Uuid) -> Result<(), DbError> {
        let reviewer = ctx.user_id().ok_or(DbError::Forbidden)?;
        sqlx::query(
            "UPDATE guest_upload_queue SET approved_at = now(), reviewed_by = $1 \
             WHERE id = $2 AND approved_at IS NULL AND rejected_at IS NULL",
        )
        .bind(reviewer.as_uuid())
        .bind(upload_id)
        .execute(self.db.pool())
        .await?;
        Ok(())
    }

    /// Rejects an upload. The caller is responsible for removing the file from
    /// disk after this returns.
    ///
    /// # Errors
    /// `DbError::Connection` on query failure.
    pub async fn reject(
        &self,
        ctx: &AuthContext,
        upload_id: Uuid,
    ) -> Result<Option<Uuid>, DbError> {
        let reviewer = ctx.user_id().ok_or(DbError::Forbidden)?;
        let row: Option<(Uuid,)> = sqlx::query_as(
            "UPDATE guest_upload_queue SET rejected_at = now(), reviewed_by = $1 \
             WHERE id = $2 AND approved_at IS NULL AND rejected_at IS NULL \
             RETURNING asset_id",
        )
        .bind(reviewer.as_uuid())
        .bind(upload_id)
        .fetch_optional(self.db.pool())
        .await?;
        Ok(row.map(|r| r.0))
    }
}
