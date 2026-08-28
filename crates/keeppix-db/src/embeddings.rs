//! Repository for CLIP embeddings.
//!
//! No `AuthContext`: this is the system's analysis pipeline, like
//! `AssetRepo::get_for_scan` / `LibraryRepo::mark_scanned`. Excluding the
//! Culling subtree uses `libraries.culling_root_folder_id` + `path <@`
//! (inert as long as the root is NULL).

use keeppix_domain::AssetId;
use std::fmt::Write as _;

use crate::{Db, DbError};

/// Kept in sync with `keeppix_media::openclip_xlmr::MODEL_VERSION`.
/// Duplicated here because `keeppix-db` cannot depend on `keeppix-media`
/// (`deny.toml`).
pub const MODEL_VERSION: &str = "openclip-xlmr-it-en";

/// A candidate for inference: has a `content_hash` (so it can have a thumbnail).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingEmbedding {
    pub asset_id: AssetId,
    pub content_hash: [u8; 32],
}

/// A row already materialized in `asset_embeddings`.
#[derive(Debug, Clone, PartialEq)]
pub struct AssetEmbedding {
    pub asset_id: AssetId,
    pub embedding: Vec<f32>,
    pub model_version: String,
}

pub struct EmbeddingRepo<'a> {
    db: &'a Db,
}

impl<'a> EmbeddingRepo<'a> {
    #[must_use]
    pub const fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// Image assets with a hash, without an embedding for `model_version`,
    /// outside the subtree rooted at `libraries.culling_root_folder_id`.
    ///
    /// # Errors
    /// `Connection` if the query fails (or if the AI schema does not exist).
    pub async fn list_pending(
        &self,
        model_version: &str,
        limit: i64,
    ) -> Result<Vec<PendingEmbedding>, DbError> {
        let rows: Vec<(uuid::Uuid, Vec<u8>)> = sqlx::query_as(
            "SELECT a.id, a.content_hash \
             FROM assets a \
             JOIN folders f ON f.id = a.folder_id \
             JOIN libraries l ON l.id = f.library_id \
             LEFT JOIN folders cull ON cull.id = l.culling_root_folder_id \
             WHERE a.content_hash IS NOT NULL \
               AND a.kind = 'image' \
               AND (cull.path IS NULL OR NOT (f.path <@ cull.path)) \
               AND NOT EXISTS ( \
                   SELECT 1 FROM asset_embeddings e \
                   WHERE e.asset_id = a.id AND e.model_version = $1 \
               ) \
             ORDER BY a.id \
             LIMIT $2",
        )
        .bind(model_version)
        .bind(limit)
        .fetch_all(self.db.pool())
        .await?;

        rows.into_iter()
            .map(|(id, hash)| {
                let content_hash: [u8; 32] = hash
                    .as_slice()
                    .try_into()
                    .map_err(|_| DbError::Corrupted(format!("content_hash len {}", hash.len())))?;
                Ok(PendingEmbedding {
                    asset_id: AssetId::from_uuid(id),
                    content_hash,
                })
            })
            .collect()
    }

    /// How many image assets (outside culling) still have no embedding for
    /// `model_version`. Used by `set_total` on the `AiAnalysis` window.
    ///
    /// # Errors
    /// `Connection` / missing AI schema.
    pub async fn count_pending(&self, model_version: &str) -> Result<i64, DbError> {
        let n: i64 = sqlx::query_scalar(
            "SELECT count(*)::bigint \
             FROM assets a \
             JOIN folders f ON f.id = a.folder_id \
             JOIN libraries l ON l.id = f.library_id \
             LEFT JOIN folders cull ON cull.id = l.culling_root_folder_id \
             WHERE a.content_hash IS NOT NULL \
               AND a.kind = 'image' \
               AND (cull.path IS NULL OR NOT (f.path <@ cull.path)) \
               AND NOT EXISTS ( \
                   SELECT 1 FROM asset_embeddings e \
                   WHERE e.asset_id = a.id AND e.model_version = $1 \
               )",
        )
        .bind(model_version)
        .fetch_one(self.db.pool())
        .await?;
        Ok(n)
    }

    /// Inserts or updates the embedding. With the same `model_version` a
    /// second upsert is a no-op on the vector already present (`ON
    /// CONFLICT DO UPDATE` only fires if the model changes — the caller
    /// does not need to re-check `list_pending` for the same version).
    ///
    /// # Errors
    /// Dimension != 512, or a database error / missing AI schema.
    pub async fn upsert(
        &self,
        asset_id: AssetId,
        embedding: &[f32],
        model_version: &str,
    ) -> Result<(), DbError> {
        if embedding.len() != 512 {
            return Err(DbError::Corrupted(format!(
                "embedding must be 512-d, got {}",
                embedding.len()
            )));
        }
        let literal = crate::embeddings::vector_literal(embedding);
        sqlx::query(
            "INSERT INTO asset_embeddings (asset_id, embedding, model_version) \
             VALUES ($1, $2::vector, $3) \
             ON CONFLICT (asset_id) DO UPDATE SET \
               embedding = EXCLUDED.embedding, \
               model_version = EXCLUDED.model_version, \
               computed_at = now() \
             WHERE asset_embeddings.model_version IS DISTINCT FROM EXCLUDED.model_version",
        )
        .bind(asset_id.as_uuid())
        .bind(&literal)
        .bind(model_version)
        .execute(self.db.pool())
        .await?;
        Ok(())
    }

    /// # Errors
    /// `Connection` / missing AI schema.
    pub async fn get(&self, asset_id: AssetId) -> Result<Option<AssetEmbedding>, DbError> {
        let row: Option<(uuid::Uuid, String, String)> = sqlx::query_as(
            "SELECT asset_id, embedding::text, model_version \
             FROM asset_embeddings WHERE asset_id = $1",
        )
        .bind(asset_id.as_uuid())
        .fetch_optional(self.db.pool())
        .await?;

        row.map(|(id, text, model_version)| {
            let embedding = parse_vector_text(&text)?;
            Ok(AssetEmbedding {
                asset_id: AssetId::from_uuid(id),
                embedding,
                model_version,
            })
        })
        .transpose()
    }
}

/// Text form `[f,f,...]` accepted by pgvector (`$n::vector`).
#[must_use]
pub fn vector_literal(embedding: &[f32]) -> String {
    let mut out = String::with_capacity(embedding.len() * 8 + 2);
    out.push('[');
    for (i, v) in embedding.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(out, "{v}");
    }
    out.push(']');
    out
}

/// `pub(crate)`: reused by [`crate::faces`] to read `faces.embedding` with
/// the same text format.
pub(crate) fn parse_vector_text(text: &str) -> Result<Vec<f32>, DbError> {
    let trimmed = text.trim();
    let inner = trimmed
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .ok_or_else(|| DbError::Corrupted(format!("vector text: {text}")))?;
    if inner.is_empty() {
        return Ok(Vec::new());
    }
    inner
        .split(',')
        .map(|p| {
            p.trim()
                .parse::<f32>()
                .map_err(|e| DbError::Corrupted(format!("vector float: {e}")))
        })
        .collect()
}
