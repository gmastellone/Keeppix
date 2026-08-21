//! Assegnazioni `asset_tags` proposte dall'IA (Fase 7 Task 8).
//!
//! Nessun `AuthContext`: è la pipeline di analisi di sistema, come
//! [`crate::EmbeddingRepo`]. L'abbinamento non è un'azione utente — scatta
//! dopo create/patch di un tag con embedding, o dopo un lotto di embed foto.
//! Le decisioni umane (`confirmed` / `rejected`) non vengono mai sovrascritte.

use keeppix_domain::{AssetId, TagId};

use crate::{Db, DbError};

/// Banda sotto la soglia del tag: score ≥ `threshold − BAND` produce ancora
/// una proposta (score più basso → fondo coda). Costante di sistema, non
/// esposta in API — un punto percentuale (0.01).
pub const TAG_MATCH_BAND: f32 = 0.01;

pub struct AssetTagRepo<'a> {
    db: &'a Db,
}

impl<'a> AssetTagRepo<'a> {
    #[must_use]
    pub const fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// Abbina tutte le foto con embedding allo stesso `model_version` del tag.
    /// Inserisce/aggiorna solo righe `state='proposed'`, `source='ai'`.
    ///
    /// # Errors
    /// `Connection` se la query fallisce (o se lo schema AI non esiste).
    pub async fn propose_for_tag(&self, tag_id: TagId) -> Result<u64, DbError> {
        let result = sqlx::query(
            "INSERT INTO asset_tags (asset_id, tag_id, state, source, score) \
             SELECT ae.asset_id, t.id, 'proposed', 'ai', \
                    (1.0 - (ae.embedding <=> t.embedding))::real \
             FROM tags t \
             JOIN asset_embeddings ae \
               ON ae.model_version = t.model_version \
             WHERE t.id = $1 \
               AND t.kind = 'tag' \
               AND t.embedding IS NOT NULL \
               AND t.model_version IS NOT NULL \
               AND (1.0 - (ae.embedding <=> t.embedding)) \
                   >= (t.threshold - $2::real) \
             ON CONFLICT (asset_id, tag_id) DO UPDATE \
               SET score = EXCLUDED.score \
               WHERE asset_tags.state = 'proposed'",
        )
        .bind(tag_id.as_uuid())
        .bind(TAG_MATCH_BAND)
        .execute(self.db.pool())
        .await?;
        Ok(result.rows_affected())
    }

    /// Abbina gli asset dati a tutti i tag con embedding (stesso
    /// `model_version`). Stesse regole di [`Self::propose_for_tag`].
    ///
    /// # Errors
    /// `Connection` se la query fallisce (o se lo schema AI non esiste).
    pub async fn propose_for_assets(&self, asset_ids: &[AssetId]) -> Result<u64, DbError> {
        if asset_ids.is_empty() {
            return Ok(0);
        }
        let uuids: Vec<uuid::Uuid> = asset_ids.iter().map(AssetId::as_uuid).collect();
        let result = sqlx::query(
            "INSERT INTO asset_tags (asset_id, tag_id, state, source, score) \
             SELECT ae.asset_id, t.id, 'proposed', 'ai', \
                    (1.0 - (ae.embedding <=> t.embedding))::real \
             FROM tags t \
             JOIN asset_embeddings ae \
               ON ae.model_version = t.model_version \
             WHERE ae.asset_id = ANY($1) \
               AND t.kind = 'tag' \
               AND t.embedding IS NOT NULL \
               AND t.model_version IS NOT NULL \
               AND (1.0 - (ae.embedding <=> t.embedding)) \
                   >= (t.threshold - $2::real) \
             ON CONFLICT (asset_id, tag_id) DO UPDATE \
               SET score = EXCLUDED.score \
               WHERE asset_tags.state = 'proposed'",
        )
        .bind(&uuids)
        .bind(TAG_MATCH_BAND)
        .execute(self.db.pool())
        .await?;
        Ok(result.rows_affected())
    }
}
