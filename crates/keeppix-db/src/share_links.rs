use chrono::{DateTime, Utc};
use keeppix_domain::{AuthContext, ShareToken};
use uuid::Uuid;

use crate::{Db, DbError};

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ShareLinkRow {
    pub id: Uuid,
    pub object_type: String,
    pub object_id: Uuid,
    pub created_by: Uuid,
    pub password_hash: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub max_views: Option<i32>,
    pub view_count: i32,
    pub allow_download: bool,
    pub allow_original: bool,
    pub allow_upload: bool,
    pub allow_cdn_cache: bool,
    pub hide_metadata: bool,
    pub upload_quota_bytes: Option<i64>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub last_accessed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[allow(clippy::struct_excessive_bools)]
pub struct NewShareLink {
    pub object_type: String,
    pub object_id: Uuid,
    pub password_hash: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub max_views: Option<i32>,
    pub allow_download: bool,
    pub allow_original: bool,
    pub allow_upload: bool,
    pub allow_cdn_cache: bool,
    pub hide_metadata: bool,
    pub upload_quota_bytes: Option<i64>,
}

pub struct ShareLinkRepo<'a> {
    db: &'a Db,
}

impl<'a> ShareLinkRepo<'a> {
    #[must_use]
    pub const fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// Creates a new share link and returns the plaintext token (returned
    /// exactly once — not stored). The `AuthContext` identifies the creator.
    ///
    /// # Errors
    /// `DbError::Connection` on query failure.
    pub async fn create(
        &self,
        ctx: &AuthContext,
        link: NewShareLink,
    ) -> Result<(Uuid, ShareToken), DbError> {
        let creator = ctx.user_id().ok_or(DbError::Forbidden)?;
        let id = Uuid::now_v7();
        let token = ShareToken::generate();
        let hash = token.digest();

        sqlx::query(
            "INSERT INTO share_links \
             (id, token_hash, object_type, object_id, created_by, password_hash, \
              expires_at, max_views, allow_download, allow_original, allow_upload, \
              allow_cdn_cache, hide_metadata, upload_quota_bytes) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)",
        )
        .bind(id)
        .bind(hash.as_slice())
        .bind(&link.object_type)
        .bind(link.object_id)
        .bind(creator.as_uuid())
        .bind(&link.password_hash)
        .bind(link.expires_at)
        .bind(link.max_views)
        .bind(link.allow_download)
        .bind(link.allow_original)
        .bind(link.allow_upload)
        .bind(link.allow_cdn_cache)
        .bind(link.hide_metadata)
        .bind(link.upload_quota_bytes)
        .execute(self.db.pool())
        .await?;

        Ok((id, token))
    }

    /// Revokes a link. Only the creator (or admin) may revoke.
    ///
    /// # Errors
    /// `DbError::Forbidden` if the caller doesn't own the link.
    /// `DbError::Connection` on query failure.
    pub async fn revoke(&self, ctx: &AuthContext, link_id: Uuid) -> Result<(), DbError> {
        let affected = if ctx.is_admin() {
            sqlx::query(
                "UPDATE share_links SET revoked_at = now() \
                 WHERE id = $1 AND revoked_at IS NULL",
            )
            .bind(link_id)
            .execute(self.db.pool())
            .await?
            .rows_affected()
        } else {
            let creator = ctx.user_id().ok_or(DbError::Forbidden)?;
            sqlx::query(
                "UPDATE share_links SET revoked_at = now() \
                 WHERE id = $1 AND created_by = $2 AND revoked_at IS NULL",
            )
            .bind(link_id)
            .bind(creator.as_uuid())
            .execute(self.db.pool())
            .await?
            .rows_affected()
        };

        if affected == 0 {
            return Err(DbError::Forbidden);
        }
        Ok(())
    }

    /// Lists links created by the authenticated user (most recent first).
    ///
    /// # Errors
    /// `DbError::Connection` on query failure.
    pub async fn list_by_creator(
        &self,
        ctx: &AuthContext,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ShareLinkRow>, DbError> {
        let creator = ctx.user_id().ok_or(DbError::Forbidden)?;
        let rows: Vec<ShareLinkRow> = sqlx::query_as(
            "SELECT id, object_type, object_id, created_by, password_hash, \
             expires_at, max_views, view_count, allow_download, allow_original, \
             allow_upload, allow_cdn_cache, hide_metadata, upload_quota_bytes, \
             revoked_at, last_accessed_at, created_at \
             FROM share_links WHERE created_by = $1 \
             ORDER BY created_at DESC LIMIT $2 OFFSET $3",
        )
        .bind(creator.as_uuid())
        .bind(limit)
        .bind(offset)
        .fetch_all(self.db.pool())
        .await?;
        Ok(rows)
    }

    /// Looks up a share link by the SHA-256 hash of its token.
    /// Returns `None` if not found, expired, revoked, or views exhausted.
    ///
    /// # Errors
    /// `DbError::Connection` on query failure.
    pub async fn lookup_by_token_hash(
        &self,
        token_hash: &[u8; 32],
    ) -> Result<Option<ShareLinkRow>, DbError> {
        let row: Option<ShareLinkRow> = sqlx::query_as(
            "SELECT id, object_type, object_id, created_by, password_hash, \
             expires_at, max_views, view_count, allow_download, allow_original, \
             allow_upload, allow_cdn_cache, hide_metadata, upload_quota_bytes, \
             revoked_at, last_accessed_at, created_at \
             FROM share_links \
             WHERE token_hash = $1 \
               AND revoked_at IS NULL \
               AND (expires_at IS NULL OR expires_at > now()) \
               AND (max_views IS NULL OR view_count < max_views)",
        )
        .bind(token_hash.as_slice())
        .fetch_optional(self.db.pool())
        .await?;
        Ok(row)
    }

    /// Increments view count and updates `last_accessed_at`.
    ///
    /// # Errors
    /// `DbError::Connection` on query failure.
    pub async fn record_view(&self, link_id: Uuid) -> Result<(), DbError> {
        sqlx::query(
            "UPDATE share_links SET view_count = view_count + 1, \
             last_accessed_at = now() WHERE id = $1",
        )
        .bind(link_id)
        .execute(self.db.pool())
        .await?;
        Ok(())
    }
}
