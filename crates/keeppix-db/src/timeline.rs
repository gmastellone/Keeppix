//! Bucket mensili e pagine keyset della timeline. Nessun `OFFSET`.

use chrono::{DateTime, Months, NaiveDate, Utc};
use keeppix_domain::{Asset, AssetId, AuthContext, LibraryId};

use crate::assets::{A_COLUMNS, AssetRow};
use crate::libraries::LibraryRepo;
use crate::visibility::VisibilityScope;
use crate::{Db, DbError, MapBounds};

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
        let filter = scope.filter_for_folder_aggregate("f.path", "f.library_id", "f.id", 1);
        let sql = format!(
            "SELECT fmc.month, sum(fmc.asset_count)::bigint AS count \
               FROM folder_month_counts fmc \
               JOIN folders f ON f.id = fmc.folder_id \
              WHERE {} AND ($4::uuid IS NULL OR f.library_id = $4) \
              GROUP BY fmc.month \
              ORDER BY fmc.month DESC",
            filter.sql()
        );
        let rows: Vec<(NaiveDate, i64)> = sqlx::query_as(&sql)
            .bind(filter.bind())
            .bind(filter.holes())
            .bind(filter.assets())
            .bind(library_id.map(|id| id.as_uuid()))
            .fetch_all(self.db.pool())
            .await?;
        Ok(rows
            .into_iter()
            .map(|(month, count)| MonthBucket { month, count })
            .collect())
    }

    /// Conteggi mensili ricalcolati sugli asset effettivamente dentro `bounds`.
    ///
    /// # Errors
    /// `Forbidden` se `library_id` non è del chiamante; `Connection` se la
    /// query fallisce.
    pub async fn buckets_in_bounds(
        &self,
        ctx: &AuthContext,
        library_id: Option<LibraryId>,
        bounds: MapBounds,
    ) -> Result<Vec<MonthBucket>, DbError> {
        if let Some(id) = library_id {
            LibraryRepo::new(self.db).find_by_id(ctx, id).await?;
        }
        let scope = VisibilityScope::resolve(self.db, ctx).await?;
        let filter = scope.filter("f.path", "f.library_id", "a.id", 1);
        let bbox = effective_bbox_filter_sql(5, 6, 7, 8);
        let sql = format!(
            "SELECT date_trunc('month', a.taken_at_utc)::date AS month, \
                    count(*)::bigint AS count \
               FROM assets a \
               JOIN folders f ON f.id = a.folder_id \
               LEFT JOIN asset_overrides o ON o.asset_id = a.id \
              WHERE {} \
                AND ($4::uuid IS NULL OR f.library_id = $4) \
                AND a.status = 'indexed' \
                AND a.kind <> 'unknown' \
                AND a.taken_at_utc IS NOT NULL \
                AND ({bbox}) \
              GROUP BY month \
              ORDER BY month DESC",
            filter.sql()
        );
        let rows: Vec<(NaiveDate, i64)> = sqlx::query_as(&sql)
            .bind(filter.bind())
            .bind(filter.holes())
            .bind(filter.assets())
            .bind(library_id.map(|id| id.as_uuid()))
            .bind(bounds.west)
            .bind(bounds.south)
            .bind(bounds.east)
            .bind(bounds.north)
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
        let filter = scope.filter("f.path", "f.library_id", "a.id", 6);
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
            .bind(filter.assets())
            .fetch_all(self.db.pool())
            .await?;
        rows.into_iter().map(AssetRow::into_domain).collect()
    }

    /// Pagina keyset dentro un mese e un riquadro geografico.
    ///
    /// # Errors
    /// `Forbidden` / `Connection` come `page`.
    pub async fn page_in_bounds(
        &self,
        ctx: &AuthContext,
        bucket: NaiveDate,
        cursor: Option<(DateTime<Utc>, AssetId)>,
        limit: i64,
        bounds: MapBounds,
    ) -> Result<Vec<Asset>, DbError> {
        let limit = limit.clamp(1, 200);
        let scope = VisibilityScope::resolve(self.db, ctx).await?;
        let filter = scope.filter("f.path", "f.library_id", "a.id", 6);
        let start = month_start(bucket);
        let end = bucket
            .checked_add_months(Months::new(1))
            .map_or(start, month_start);
        let (cursor_time, cursor_id) = match cursor {
            Some((t, id)) => (Some(t), Some(id.as_uuid())),
            None => (None, None),
        };
        let bbox = effective_bbox_filter_sql(9, 10, 11, 12);
        let sql = format!(
            "SELECT {A_COLUMNS} FROM assets a \
             JOIN folders f ON f.id = a.folder_id \
             LEFT JOIN asset_overrides o ON o.asset_id = a.id \
             WHERE {} \
               AND a.status = 'indexed' \
               AND a.kind <> 'unknown' \
               AND a.taken_at_utc >= $1 AND a.taken_at_utc < $2 \
               AND ($3::timestamptz IS NULL \
                    OR a.taken_at_utc < $3 \
                    OR (a.taken_at_utc = $3 AND a.id < $4)) \
               AND ({bbox}) \
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
            .bind(filter.assets())
            .bind(bounds.west)
            .bind(bounds.south)
            .bind(bounds.east)
            .bind(bounds.north)
            .fetch_all(self.db.pool())
            .await?;
        rows.into_iter().map(AssetRow::into_domain).collect()
    }
}

fn effective_bbox_filter_sql(west: usize, south: usize, east: usize, north: usize) -> String {
    let w = format!("${west}");
    let s = format!("${south}");
    let e = format!("${east}");
    let n = format!("${north}");
    format!(
        "({w} <= {e} AND (\
             (o.location IS NOT NULL AND o.location \
              && ST_Segmentize(ST_MakeEnvelope({w}, {s}, {e}, {n}, 4326), 90.0)::geography) \
             OR (o.location IS NULL AND a.location \
                 && ST_Segmentize(ST_MakeEnvelope({w}, {s}, {e}, {n}, 4326), 90.0)::geography)\
         )) OR ({w} > {e} AND (\
             (o.location IS NOT NULL AND (\
                 o.location && ST_Segmentize(\
                     ST_MakeEnvelope({w}, {s}, 180.0, {n}, 4326), 90.0\
                 )::geography \
                 OR o.location && ST_Segmentize(\
                     ST_MakeEnvelope(-180.0, {s}, {e}, {n}, 4326), 90.0\
                 )::geography\
             )) OR (o.location IS NULL AND (\
                 a.location && ST_Segmentize(\
                     ST_MakeEnvelope({w}, {s}, 180.0, {n}, 4326), 90.0\
                 )::geography \
                 OR a.location && ST_Segmentize(\
                     ST_MakeEnvelope(-180.0, {s}, {e}, {n}, 4326), 90.0\
                 )::geography\
             ))\
         ))"
    )
}

fn month_start(d: NaiveDate) -> DateTime<Utc> {
    d.and_hms_opt(0, 0, 0)
        .map_or(DateTime::<Utc>::UNIX_EPOCH, |ndt| ndt.and_utc())
}
