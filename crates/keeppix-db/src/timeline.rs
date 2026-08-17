//! Bucket mensili e pagine keyset della timeline. Nessun `OFFSET`.

use chrono::{DateTime, Months, NaiveDate, Utc};
use keeppix_domain::{Asset, AssetId, AuthContext, LibraryId};

use crate::assets::{A_COLUMNS, AssetRow};
use crate::libraries::LibraryRepo;
use crate::visibility::VisibilityScope;
use crate::{Db, DbError};

pub struct TimelineRepo<'a> {
    db: &'a Db,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonthBucket {
    pub month: NaiveDate,
    pub count: i64,
}

impl<'a> TimelineRepo<'a> {
    #[must_use]
    pub const fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// # Errors
    /// `Forbidden` se `library_id` non è del chiamante (anche inesistente).
    /// `Connection` se la query fallisce.
    pub async fn buckets(
        &self,
        ctx: &AuthContext,
        library_id: Option<LibraryId>,
    ) -> Result<Vec<MonthBucket>, DbError> {
        if let Some(id) = library_id {
            LibraryRepo::new(self.db).find_by_id(ctx, id).await?;
        }
        let scope = VisibilityScope::resolve(self.db, ctx).await?;
        let filter = scope.filter("f.path", "f.library_id", 1);
        let sql = format!(
            "SELECT fmc.month, sum(fmc.asset_count)::bigint AS count \
               FROM folder_month_counts fmc \
               JOIN folders f ON f.id = fmc.folder_id \
              WHERE {} AND ($3::uuid IS NULL OR f.library_id = $3) \
              GROUP BY fmc.month \
              ORDER BY fmc.month DESC",
            filter.sql()
        );
        let rows: Vec<(NaiveDate, i64)> = sqlx::query_as(&sql)
            .bind(filter.bind())
            .bind(filter.holes())
            .bind(library_id.map(|id| id.as_uuid()))
            .fetch_all(self.db.pool())
            .await?;
        Ok(rows
            .into_iter()
            .map(|(month, count)| MonthBucket { month, count })
            .collect())
    }

    /// Pagina keyset dentro un mese. `limit` è clampato a 1..=200.
    ///
    /// # Errors
    /// `Forbidden` / `Connection` come `buckets`.
    pub async fn page(
        &self,
        ctx: &AuthContext,
        bucket: NaiveDate,
        cursor: Option<(DateTime<Utc>, AssetId)>,
        limit: i64,
    ) -> Result<Vec<Asset>, DbError> {
        let limit = limit.clamp(1, 200);
        let scope = VisibilityScope::resolve(self.db, ctx).await?;
        let filter = scope.filter("f.path", "f.library_id", 6);
        let start = month_start(bucket);
        let end = bucket
            .checked_add_months(Months::new(1))
            .map_or(start, month_start);
        let (cursor_time, cursor_id) = match cursor {
            Some((t, id)) => (Some(t), Some(id.as_uuid())),
            None => (None, None),
        };
        let sql = format!(
            "SELECT {A_COLUMNS} FROM assets a \
             JOIN folders f ON f.id = a.folder_id \
             WHERE {} \
               AND a.status = 'indexed' \
               AND a.kind <> 'unknown' \
               AND a.taken_at_utc >= $1 AND a.taken_at_utc < $2 \
               AND ($3::timestamptz IS NULL \
                    OR a.taken_at_utc < $3 \
                    OR (a.taken_at_utc = $3 AND a.id < $4)) \
             ORDER BY a.taken_at_utc DESC NULLS LAST, a.id DESC \
             LIMIT $5",
            filter.sql()
        );
        let rows: Vec<AssetRow> = sqlx::query_as(&sql)
            .bind(start)
            .bind(end)
            .bind(cursor_time)
            .bind(cursor_id)
            .bind(limit)
            .bind(filter.bind())
            .bind(filter.holes())
            .fetch_all(self.db.pool())
            .await?;
        rows.into_iter().map(AssetRow::into_domain).collect()
    }
}

fn month_start(d: NaiveDate) -> DateTime<Utc> {
    d.and_hms_opt(0, 0, 0)
        .map_or(DateTime::<Utc>::UNIX_EPOCH, |ndt| ndt.and_utc())
}
