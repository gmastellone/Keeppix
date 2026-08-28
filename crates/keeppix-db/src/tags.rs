//! Shared vocabulary of tags and categories.
//!
//! Any authenticated user can CRUD: tags are not per-user. An unknown id
//! -> `Forbidden` (never `NotFound`), like the rest of the domain. Only
//! one level of nesting: category (`parent_id = NULL`) -> tag.
//!
//! The text embedding lives on `tags.embedding`; creating/modifying a tag
//! does not touch `asset_embeddings`.

use std::str::FromStr;

use chrono::{DateTime, Utc};
use keeppix_domain::{AuthContext, TagId, TagKind, UserId};

use crate::embeddings::vector_literal;
use crate::{Db, DbError};

pub struct TagRepo<'a> {
    db: &'a Db,
}

#[derive(Debug, Clone)]
pub struct NewTag {
    pub name: String,
    pub kind: TagKind,
    pub parent_id: Option<TagId>,
    pub prompt: Option<String>,
    pub color: Option<String>,
    pub threshold: Option<f32>,
    pub embedding: Option<Vec<f32>>,
    pub model_version: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TagPatch {
    pub name: Option<String>,
    #[allow(clippy::option_option)]
    pub parent_id: Option<Option<TagId>>,
    #[allow(clippy::option_option)]
    pub prompt: Option<Option<String>>,
    #[allow(clippy::option_option)]
    pub color: Option<Option<String>>,
    pub threshold: Option<f32>,
    #[allow(clippy::option_option)]
    pub embedding: Option<Option<Vec<f32>>>,
    #[allow(clippy::option_option)]
    pub model_version: Option<Option<String>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TagView {
    pub id: TagId,
    pub name: String,
    pub kind: TagKind,
    pub parent_id: Option<TagId>,
    pub prompt: Option<String>,
    pub color: Option<String>,
    pub threshold: f32,
    pub has_embedding: bool,
    pub model_version: Option<String>,
    pub created_by: UserId,
    pub created_at: DateTime<Utc>,
    pub assignment_count: i64,
}

#[derive(Debug, sqlx::FromRow)]
struct TagRow {
    id: uuid::Uuid,
    name: String,
    kind: String,
    parent_id: Option<uuid::Uuid>,
    prompt: Option<String>,
    color: Option<String>,
    threshold: f32,
    has_embedding: bool,
    model_version: Option<String>,
    created_by: uuid::Uuid,
    created_at: DateTime<Utc>,
    assignment_count: i64,
}

impl TagRow {
    fn into_view(self) -> Result<TagView, DbError> {
        let kind = TagKind::from_str(&self.kind)
            .map_err(|e| DbError::Corrupted(format!("tags.kind: {e}")))?;
        Ok(TagView {
            id: TagId::from_uuid(self.id),
            name: self.name,
            kind,
            parent_id: self.parent_id.map(TagId::from_uuid),
            prompt: self.prompt,
            color: self.color,
            threshold: self.threshold,
            has_embedding: self.has_embedding,
            model_version: self.model_version,
            created_by: UserId::from_uuid(self.created_by),
            created_at: self.created_at,
            assignment_count: self.assignment_count,
        })
    }
}

const TAG_SELECT: &str = "SELECT t.id, t.name, t.kind, t.parent_id, t.prompt, t.color, \
     t.threshold, (t.embedding IS NOT NULL) AS has_embedding, t.model_version, \
     t.created_by, t.created_at, \
     (SELECT count(*)::bigint FROM asset_tags at WHERE at.tag_id = t.id) AS assignment_count \
     FROM tags t";

fn map_unique_conflict(err: sqlx::Error) -> DbError {
    if let sqlx::Error::Database(ref db_err) = err
        && db_err.code().as_deref() == Some("23505")
    {
        return DbError::Conflict("tag name is already in use for this kind".to_owned());
    }
    DbError::Connection(err)
}

impl<'a> TagRepo<'a> {
    #[must_use]
    pub const fn new(db: &'a Db) -> Self {
        Self { db }
    }

    fn require_user(ctx: &AuthContext) -> Result<UserId, DbError> {
        ctx.user_id().ok_or(DbError::Forbidden)
    }

    /// Flat list of the vocabulary (categories and tags). Ordered by kind, name.
    ///
    /// # Errors
    /// `Forbidden` without an authenticated user; `Connection` on DB error / missing AI schema.
    pub async fn list(&self, ctx: &AuthContext) -> Result<Vec<TagView>, DbError> {
        Self::require_user(ctx)?;
        let rows: Vec<TagRow> =
            sqlx::query_as(&format!("{TAG_SELECT} ORDER BY t.kind ASC, t.name ASC"))
                .fetch_all(self.db.pool())
                .await?;
        rows.into_iter().map(TagRow::into_view).collect()
    }

    /// # Errors
    /// `Forbidden` if absent or without a user; never `NotFound`.
    pub async fn get(&self, ctx: &AuthContext, id: TagId) -> Result<TagView, DbError> {
        Self::require_user(ctx)?;
        let row: Option<TagRow> = sqlx::query_as(&format!("{TAG_SELECT} WHERE t.id = $1"))
            .bind(id.as_uuid())
            .fetch_optional(self.db.pool())
            .await?;
        row.ok_or(DbError::Forbidden).and_then(TagRow::into_view)
    }

    /// # Errors
    /// `Forbidden` without a user; `Conflict` on duplicate name or
    /// illegal nesting; `Corrupted` if the embedding is not 512-d.
    pub async fn create(&self, ctx: &AuthContext, new: NewTag) -> Result<TagView, DbError> {
        let created_by = Self::require_user(ctx)?;
        self.validate_parent(new.kind, new.parent_id).await?;

        // Calibrated against OpenCLIP XLM-R IT/EN: real text-image cosine
        // similarity in this embedding space sits at 0.10-0.20 even for
        // correct matches (correct_score min 0.126-0.132, best_wrong max
        // 0.174-0.177 on a real CI benchmark) — 0.75 would NEVER have
        // produced a proposal. 0.20 sits above the observed ceiling of
        // the most confusable false positives, with real margin for
        // typical cases. This starting number comes from a synthetic
        // benchmark of 20 pairs, not a definitive calibration — it stays
        // adjustable per tag (`tags.threshold`), this is only the default.
        let threshold = new.threshold.unwrap_or(0.20);
        let embedding_lit = match new.embedding.as_deref() {
            Some(v) => Some(Self::embedding_literal(v)?),
            None => None,
        };

        let id = TagId::new();
        // INSERT + get kept separate: a CTE `INSERT ... RETURNING` joined
        // to `TAG_SELECT` (which already has a `FROM`) confused
        // sqlx/Postgres and returned RowNotFound even on a successful insert.
        sqlx::query(
            "INSERT INTO tags (id, name, kind, parent_id, prompt, embedding, model_version, \
                              color, threshold, created_by) \
             VALUES ($1, $2, $3, $4, $5, $6::vector, $7, $8, $9, $10)",
        )
        .bind(id.as_uuid())
        .bind(&new.name)
        .bind(new.kind.as_str())
        .bind(new.parent_id.map(|p| p.as_uuid()))
        .bind(&new.prompt)
        .bind(embedding_lit.as_deref())
        .bind(&new.model_version)
        .bind(&new.color)
        .bind(threshold)
        .bind(created_by.as_uuid())
        .execute(self.db.pool())
        .await
        .map_err(map_unique_conflict)?;
        self.get(ctx, id).await
    }

    /// # Errors
    /// `Forbidden` if absent; `Conflict` on name/nesting; `Corrupted` on embedding.
    pub async fn update(
        &self,
        ctx: &AuthContext,
        id: TagId,
        patch: TagPatch,
    ) -> Result<TagView, DbError> {
        Self::require_user(ctx)?;
        // Existence check first: otherwise a patch on a ghost id would
        // become NotFound via zero rows updated — here it is Forbidden.
        let current = self.get(ctx, id).await?;

        let name = patch.name.as_ref().unwrap_or(&current.name);
        let parent_id = match patch.parent_id {
            Some(v) => v,
            None => current.parent_id,
        };
        let prompt = match &patch.prompt {
            Some(v) => v.clone(),
            None => current.prompt.clone(),
        };
        let color = match &patch.color {
            Some(v) => v.clone(),
            None => current.color.clone(),
        };
        let threshold = patch.threshold.unwrap_or(current.threshold);

        self.validate_parent(current.kind, parent_id).await?;

        let (embedding_lit, model_version) = match &patch.embedding {
            Some(Some(v)) => (
                Some(Self::embedding_literal(v)?),
                patch
                    .model_version
                    .clone()
                    .unwrap_or_else(|| current.model_version.clone()),
            ),
            Some(None) => (None, patch.model_version.clone().unwrap_or(None)),
            None => {
                // Do not touch the vector: UPDATE leaves embedding/model_version
                // as they were (a sentinel is passed via CASE below).
                (None, current.model_version.clone())
            }
        };
        let clear_embedding = matches!(patch.embedding, Some(None));
        let set_embedding = matches!(patch.embedding, Some(Some(_)));

        let updated = sqlx::query(
            "UPDATE tags SET \
               name = $2, \
               parent_id = $3, \
               prompt = $4, \
               color = $5, \
               threshold = $6, \
               embedding = CASE \
                 WHEN $7 THEN NULL \
                 WHEN $8 THEN $9::vector \
                 ELSE embedding \
               END, \
               model_version = CASE \
                 WHEN $7 THEN NULL \
                 WHEN $8 THEN $10 \
                 ELSE model_version \
               END \
             WHERE id = $1",
        )
        .bind(id.as_uuid())
        .bind(name)
        .bind(parent_id.map(|p| p.as_uuid()))
        .bind(&prompt)
        .bind(&color)
        .bind(threshold)
        .bind(clear_embedding)
        .bind(set_embedding)
        .bind(embedding_lit.as_deref())
        .bind(model_version.as_deref())
        .execute(self.db.pool())
        .await
        .map_err(map_unique_conflict)?
        .rows_affected();

        if updated == 0 {
            return Err(DbError::Forbidden);
        }
        self.get(ctx, id).await
    }

    /// Deletes the tag; `asset_tags` cascades via FK.
    ///
    /// # Errors
    /// `Forbidden` if absent or without a user.
    pub async fn delete(&self, ctx: &AuthContext, id: TagId) -> Result<(), DbError> {
        Self::require_user(ctx)?;
        let deleted = sqlx::query("DELETE FROM tags WHERE id = $1")
            .bind(id.as_uuid())
            .execute(self.db.pool())
            .await?
            .rows_affected();
        if deleted == 0 {
            return Err(DbError::Forbidden);
        }
        Ok(())
    }

    async fn validate_parent(
        &self,
        kind: TagKind,
        parent_id: Option<TagId>,
    ) -> Result<(), DbError> {
        match (kind, parent_id) {
            (TagKind::Category, Some(_)) => Err(DbError::Conflict(
                "a category cannot have a parent".to_owned(),
            )),
            (TagKind::Category | TagKind::Tag, None) => Ok(()),
            (TagKind::Tag, Some(parent)) => {
                let parent_kind: Option<String> =
                    sqlx::query_scalar("SELECT kind FROM tags WHERE id = $1")
                        .bind(parent.as_uuid())
                        .fetch_optional(self.db.pool())
                        .await?;
                match parent_kind.as_deref() {
                    Some("category") => Ok(()),
                    Some(_) => Err(DbError::Conflict(
                        "a tag's parent must be a category".to_owned(),
                    )),
                    None => Err(DbError::Forbidden),
                }
            }
        }
    }

    fn embedding_literal(embedding: &[f32]) -> Result<String, DbError> {
        if embedding.len() != 512 {
            return Err(DbError::Corrupted(format!(
                "tag embedding must be 512-d, got {}",
                embedding.len()
            )));
        }
        Ok(vector_literal(embedding))
    }
}
