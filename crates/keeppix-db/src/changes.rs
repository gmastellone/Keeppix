use keeppix_domain::{AssetId, AuthContext};

use crate::visibility::VisibilityScope;
use crate::{Db, DbError};

const PAGE: usize = 1000;

pub struct ChangeLogRepo<'a> {
    db: &'a Db,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangePage {
    pub cursor: i64,
    pub upserted: Vec<AssetId>,
    pub deleted: Vec<AssetId>,
    pub has_more: bool,
}

#[derive(sqlx::FromRow)]
struct ChangeRow {
    seq: i64,
    entity_id: uuid::Uuid,
    op: String,
}

impl<'a> ChangeLogRepo<'a> {
    #[must_use]
    pub const fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// Modifiche con `seq > cursor`, visibili al chiamante.
    ///
    /// Il cursore restituito è arretrato al massimo `seq` le cui transazioni
    /// sono certamente concluse (`xmin < pg_snapshot_xmin`). Senza, una
    /// transazione con seq più basso che committà dopo una con seq più alto
    /// sparirebbe dal delta.
    ///
    /// # Errors
    /// `Connection` se la query fallisce.
    pub async fn since(&self, ctx: &AuthContext, cursor: i64) -> Result<ChangePage, DbError> {
        let scope = VisibilityScope::resolve(self.db, ctx).await?;
        let filter = scope.filter("c.library_id", 2);

        let rows: Vec<ChangeRow> = sqlx::query_as(&format!(
            "SELECT c.seq, c.entity_id, c.op FROM change_log c \
              WHERE c.seq > $1 AND c.entity = 'asset' AND {} \
              ORDER BY c.seq \
              LIMIT {}",
            filter.sql(),
            PAGE + 1
        ))
        .bind(cursor)
        .bind(filter.bind())
        .fetch_all(self.db.pool())
        .await?;

        let has_more = rows.len() > PAGE;
        let page: Vec<ChangeRow> = rows.into_iter().take(PAGE).collect();

        let mut upserted = Vec::new();
        let mut deleted = Vec::new();
        for row in &page {
            let id = AssetId::from_uuid(row.entity_id);
            match row.op.as_str() {
                "delete" => deleted.push(id),
                _ => upserted.push(id),
            }
        }

        let last_delivered = page.last().map_or(cursor, |row| row.seq);
        let safe = self.safe_cursor(cursor).await?;
        let new_cursor = last_delivered.min(safe);

        Ok(ChangePage {
            cursor: new_cursor,
            upserted,
            deleted,
            has_more,
        })
    }

    async fn safe_cursor(&self, cursor: i64) -> Result<i64, DbError> {
        // Spec §2.6: arretrare al limite delle transazioni certamente
        // concluse. Lo sketch usa `xmin >= snapshot_xmin` e `min(seq)-1`;
        // quello avanzerebbe il cursore *oltre* il seq ancora in-flight
        // (non visibile). Il massimo seq con `xmin < snapshot_xmin` è il
        // high-water mark che non perde righe: i client rivedono eventuali
        // duplicati, che per un delta `upsert` sono idempotenti.
        let frozen: Option<i64> = sqlx::query_scalar(
            "SELECT MAX(seq) FROM change_log \
              WHERE xmin::text::bigint \
                    < pg_snapshot_xmin(pg_current_snapshot())::text::bigint",
        )
        .fetch_one(self.db.pool())
        .await?;

        Ok(frozen.unwrap_or(cursor).max(cursor))
    }

    /// High-water mark for a live subscriber that already loaded via REST.
    /// `seq` is global; visibility is applied on each [`Self::since`] poll.
    ///
    /// # Errors
    /// `Connection` se la query fallisce.
    pub async fn head_seq(&self, _ctx: &AuthContext) -> Result<i64, DbError> {
        self.safe_cursor(0).await
    }
}
