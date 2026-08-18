use chrono::{DateTime, Utc};
use keeppix_domain::{Actor, AssetKind, AuthContext, NewAsset};
use uuid::Uuid;

use crate::folders::FolderRepo;
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

    /// Bytes already accepted on this link (pending or approved). Rejected
    /// rows no longer count: the caller removes those files from disk.
    ///
    /// # Errors
    /// `Forbidden` if the caller is not the share link that owns the quota.
    /// `Connection` on query failure.
    pub async fn bytes_used(&self, ctx: &AuthContext) -> Result<i64, DbError> {
        let Actor::ShareLink { link_id, .. } = ctx.actor else {
            return Err(DbError::Forbidden);
        };
        let used: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(size_bytes), 0)::bigint FROM guest_upload_queue \
             WHERE share_link_id = $1 AND rejected_at IS NULL",
        )
        .bind(link_id)
        .fetch_one(self.db.pool())
        .await?;
        Ok(used)
    }

    /// Inserts the asset as `discovered` + `uploaded_by_guest` and enqueues it
    /// for review in one transaction, so a crash cannot leave one without the
    /// other.
    ///
    /// # Errors
    /// `Forbidden` if the share cannot write this folder. `Conflict` if
    /// `(folder_id, filename)` already exists. `Connection` on query failure.
    pub async fn receive(&self, ctx: &AuthContext, new: NewAsset) -> Result<(Uuid, Uuid), DbError> {
        let Actor::ShareLink {
            link_id,
            object_type,
            object_id,
            allow_upload,
            ..
        } = &ctx.actor
        else {
            return Err(DbError::Forbidden);
        };
        if !allow_upload || object_type != "folder" || *object_id != new.folder_id.as_uuid() {
            return Err(DbError::Forbidden);
        }
        FolderRepo::new(self.db)
            .find_by_id(ctx, new.folder_id)
            .await?;

        let asset_id = Uuid::now_v7();
        let upload_id = Uuid::now_v7();
        let mut tx = self.db.pool().begin().await?;
        sqlx::query(
            "INSERT INTO assets (id, folder_id, filename, size_bytes, mtime, inode, kind, \
             status, uploaded_by_guest) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, 'discovered', true)",
        )
        .bind(asset_id)
        .bind(new.folder_id.as_uuid())
        .bind(new.filename.as_str())
        .bind(new.size_bytes)
        .bind(new.mtime)
        .bind(new.inode)
        .bind(kind_str(new.kind))
        .execute(&mut *tx)
        .await
        .map_err(map_unique_violation)?;
        sqlx::query(
            "INSERT INTO guest_upload_queue (id, asset_id, share_link_id, filename, size_bytes) \
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(upload_id)
        .bind(asset_id)
        .bind(link_id)
        .bind(new.filename.as_str())
        .bind(new.size_bytes)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok((upload_id, asset_id))
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

    /// Approves an upload. Returns the asset id so the caller can clear the
    /// guest flag and enqueue ingest. Only the link creator (or an admin) may
    /// approve — otherwise the id is an existence oracle.
    ///
    /// # Errors
    /// `Forbidden` if the caller does not own the link or the row is gone.
    /// `Connection` on query failure.
    pub async fn approve(&self, ctx: &AuthContext, upload_id: Uuid) -> Result<Uuid, DbError> {
        let reviewer = ctx.user_id().ok_or(DbError::Forbidden)?;
        let row: Option<(Uuid,)> = if ctx.is_admin() {
            sqlx::query_as(
                "UPDATE guest_upload_queue SET approved_at = now(), reviewed_by = $1 \
                 WHERE id = $2 AND approved_at IS NULL AND rejected_at IS NULL \
                 RETURNING asset_id",
            )
            .bind(reviewer.as_uuid())
            .bind(upload_id)
            .fetch_optional(self.db.pool())
            .await?
        } else {
            sqlx::query_as(
                "UPDATE guest_upload_queue q SET approved_at = now(), reviewed_by = $1 \
                 FROM share_links s \
                 WHERE q.id = $2 AND q.share_link_id = s.id AND s.created_by = $1 \
                   AND q.approved_at IS NULL AND q.rejected_at IS NULL \
                 RETURNING q.asset_id",
            )
            .bind(reviewer.as_uuid())
            .bind(upload_id)
            .fetch_optional(self.db.pool())
            .await?
        };
        row.map(|r| r.0).ok_or(DbError::Forbidden)
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

const fn kind_str(kind: AssetKind) -> &'static str {
    match kind {
        AssetKind::Image => "image",
        AssetKind::RawImage => "raw_image",
        AssetKind::Video => "video",
        AssetKind::Unknown => "unknown",
    }
}

fn map_unique_violation(err: sqlx::Error) -> DbError {
    if let sqlx::Error::Database(ref db_err) = err
        && db_err.code().as_deref() == Some("23505")
    {
        return DbError::Conflict("asset already exists in this folder".into());
    }
    DbError::Connection(err)
}
