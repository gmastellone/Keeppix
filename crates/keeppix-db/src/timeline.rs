//! Monthly buckets and keyset pages for the timeline. No `OFFSET`.

use chrono::{DateTime, Months, NaiveDate, Utc};
use keeppix_domain::{AssetId, AuthContext, LibraryId};

use crate::assets::A_COLUMNS;
use crate::libraries::LibraryRepo;
use crate::stacks::{
    AssetStackRow, AssetWithStack, STACK_BADGE_COLUMNS_SQL, STACK_BADGE_JOIN_SQL,
    STACK_PRIMARY_JOIN_SQL, STACK_PRIMARY_ONLY_SQL,
};
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

/// A geometry row: known dimensions (or `None` if the asset has not been
/// sized yet) and the shot's timestamp, in the same order as the timeline
/// (`taken_at_utc DESC, id DESC`). No identifier: geometry describes
/// heights, it does not identify assets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GeometryRecord {
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub taken_at_utc: DateTime<Utc>,
}

/// Geometry of an entire timeline view, plus the minimal information
/// needed to build an `ETag`: the max `updated_at` among the view's
/// assets. `records.len()` is the full response's count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Geometry {
    pub records: Vec<GeometryRecord>,
    pub last_modified: Option<DateTime<Utc>>,
    /// `Some` only if this was a paginated request ([`GeometryPage`]) and
    /// there might be more after it — the HTTP caller exposes it as an
    /// opaque cursor for the next page, never in the binary body:
    /// geometry carries no identifiers by construction.
    pub next_cursor: Option<(DateTime<Utc>, AssetId)>,
}

/// Either the first "whole view" page (without `page`) or a keyset
/// continuation after a cursor — never `OFFSET` (see the note at the top
/// of this file): the cost of skipping N rows grows with N, keyset stays
/// O(log n) on the existing index regardless of where you are in the view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GeometryPage {
    pub limit: i64,
    pub after: Option<(DateTime<Utc>, AssetId)>,
}

/// Cap on `GeometryPage::limit` — a client asking for more is silently
/// clamped, not rejected: the request stays valid, it just gets less per
/// round trip. 20,000 records at 6 bytes each is ~117 KB, already well
/// beyond what a first paint over a slow network needs.
const GEOMETRY_PAGE_LIMIT_MAX: i64 = 20_000;

/// Raw row shared by `geometry`/`geometry_in_bounds`: id (for the next
/// page's cursor, never in the binary payload), dimensions, shot timestamp.
type GeometryRow = (uuid::Uuid, Option<i32>, Option<i32>, DateTime<Utc>);

/// Lightweight stamp of the geometry view (`count` + `max(updated_at)`),
/// used to validate `If-None-Match` **before** downloading all the records.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GeometryStamp {
    pub count: u64,
    pub last_modified: Option<DateTime<Utc>>,
}

impl<'a> TimelineRepo<'a> {
    #[must_use]
    pub const fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// Counts stacks (a stacked group counts as 1, not however many files
    /// make it up), no longer reading rows from `folder_month_counts`:
    /// the trigger that feeds that table does not look at `stack_id` —
    /// doing so would mean teaching it to recompute a stack's count every
    /// time the primary changes (`StackRepo::set_primary`) or a member is
    /// added/removed, far more complexity in the trigger for a single
    /// endpoint. Instead this counts directly from `assets` with the same
    /// primary filter as `page`, so the number of months and the number
    /// of tiles per month never diverge. The `folder_month_counts` table
    /// stays intact for its other uses (folder counters, trash).
    ///
    /// # Errors
    /// `Forbidden` if `library_id` does not belong to the caller (even if
    /// nonexistent). `Connection` if the query fails.
    pub async fn buckets(
        &self,
        ctx: &AuthContext,
        library_id: Option<LibraryId>,
    ) -> Result<Vec<MonthBucket>, DbError> {
        if let Some(id) = library_id {
            LibraryRepo::new(self.db).find_by_id(ctx, id).await?;
        }
        let scope = VisibilityScope::resolve(self.db, ctx).await?;
        let filter = scope.filter("f.path", "f.library_id", "a.id", 1);
        let sql = format!(
            "SELECT date_trunc('month', a.taken_at_utc)::date AS month, \
                    count(*)::bigint AS count \
               FROM assets a \
               JOIN folders f ON f.id = a.folder_id \
               {STACK_PRIMARY_JOIN_SQL} \
              WHERE {} \
                AND ($4::uuid IS NULL OR f.library_id = $4) \
                AND a.status = 'indexed' \
                AND a.kind <> 'unknown' \
                AND a.taken_at_utc IS NOT NULL \
                AND {STACK_PRIMARY_ONLY_SQL} \
              GROUP BY month \
              ORDER BY month DESC",
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

    /// Monthly counts recomputed over the assets actually inside `bounds`.
    ///
    /// # Errors
    /// `Forbidden` if `library_id` does not belong to the caller;
    /// `Connection` if the query fails.
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
               {STACK_PRIMARY_JOIN_SQL} \
              WHERE {} \
                AND ($4::uuid IS NULL OR f.library_id = $4) \
                AND a.status = 'indexed' \
                AND a.kind <> 'unknown' \
                AND a.taken_at_utc IS NOT NULL \
                AND ({bbox}) \
                AND {STACK_PRIMARY_ONLY_SQL} \
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

    /// Keyset page within a month. `limit` is clamped to 1..=200. Returns
    /// only the primary of each stack, with the stack badge: a stacked
    /// RAW+JPEG asset is one tile, not two.
    ///
    /// # Errors
    /// `Forbidden` / `Connection` same as `buckets`.
    pub async fn page(
        &self,
        ctx: &AuthContext,
        bucket: NaiveDate,
        cursor: Option<(DateTime<Utc>, AssetId)>,
        limit: i64,
    ) -> Result<Vec<AssetWithStack>, DbError> {
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
            "SELECT {A_COLUMNS}, {STACK_BADGE_COLUMNS_SQL} FROM assets a \
             JOIN folders f ON f.id = a.folder_id \
             {STACK_BADGE_JOIN_SQL} \
             WHERE {} \
               AND a.status = 'indexed' \
               AND a.kind <> 'unknown' \
               AND a.taken_at_utc >= $1 AND a.taken_at_utc < $2 \
               AND ($3::timestamptz IS NULL \
                    OR a.taken_at_utc < $3 \
                    OR (a.taken_at_utc = $3 AND a.id < $4)) \
               AND {STACK_PRIMARY_ONLY_SQL} \
             ORDER BY a.taken_at_utc DESC NULLS LAST, a.id DESC \
             LIMIT $5",
            filter.sql()
        );
        let rows: Vec<AssetStackRow> = sqlx::query_as(&sql)
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
        rows.into_iter().map(AssetStackRow::into_domain).collect()
    }

    /// Keyset page within a month and a geographic bounding box.
    ///
    /// # Errors
    /// `Forbidden` / `Connection` same as `page`.
    pub async fn page_in_bounds(
        &self,
        ctx: &AuthContext,
        bucket: NaiveDate,
        cursor: Option<(DateTime<Utc>, AssetId)>,
        limit: i64,
        bounds: MapBounds,
    ) -> Result<Vec<AssetWithStack>, DbError> {
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
            "SELECT {A_COLUMNS}, {STACK_BADGE_COLUMNS_SQL} FROM assets a \
             JOIN folders f ON f.id = a.folder_id \
             LEFT JOIN asset_overrides o ON o.asset_id = a.id \
             {STACK_BADGE_JOIN_SQL} \
             WHERE {} \
               AND a.status = 'indexed' \
               AND a.kind <> 'unknown' \
               AND a.taken_at_utc >= $1 AND a.taken_at_utc < $2 \
               AND ($3::timestamptz IS NULL \
                    OR a.taken_at_utc < $3 \
                    OR (a.taken_at_utc = $3 AND a.id < $4)) \
               AND ({bbox}) \
               AND {STACK_PRIMARY_ONLY_SQL} \
             ORDER BY a.taken_at_utc DESC NULLS LAST, a.id DESC \
             LIMIT $5",
            filter.sql()
        );
        let rows: Vec<AssetStackRow> = sqlx::query_as(&sql)
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
        rows.into_iter().map(AssetStackRow::into_domain).collect()
    }

    /// Geometry of the whole view (no pagination): width, height, and
    /// shot timestamp for every visible asset, in the same order as the
    /// timeline. Assets with no known `width`/`height` stay in the
    /// result with `None`: excluding them would make the layout "jump"
    /// when sizing arrives.
    ///
    /// Filters `kind <> 'unknown'` like `page` (this used not to, to stay
    /// consistent with `folder_month_counts`, which does not look at
    /// `kind` — but now that `buckets` no longer reads from there and
    /// filters `kind` directly, that is the consistency to maintain).
    /// Returns only the primary of each stack. The query stays
    /// index-only on `assets_geometry_idx` (`folder_id, taken_at_utc
    /// DESC, id DESC INCLUDE (width, height, stack_id, kind) WHERE status
    /// = 'indexed'`): both `stack_id` (for the primary filter) and `kind`
    /// are in the `INCLUDE`; the join to `stacks` for the primary
    /// equality only touches that small table, not `assets`.
    ///
    /// # Errors
    /// `Forbidden` if `library_id` does not belong to the caller;
    /// `Connection` if the query fails.
    pub async fn geometry(
        &self,
        ctx: &AuthContext,
        library_id: Option<LibraryId>,
        page: Option<GeometryPage>,
    ) -> Result<Geometry, DbError> {
        if let Some(id) = library_id {
            LibraryRepo::new(self.db).find_by_id(ctx, id).await?;
        }
        let scope = VisibilityScope::resolve(self.db, ctx).await?;
        let filter = scope.filter("f.path", "f.library_id", "a.id", 1);
        let (cursor_time, cursor_id, limit) = page.map_or((None, None, None), |p| {
            (
                p.after.map(|(t, _)| t),
                p.after.map(|(_, id)| id.as_uuid()),
                Some(p.limit.clamp(1, GEOMETRY_PAGE_LIMIT_MAX)),
            )
        });
        // `LIMIT $7` is always in the query: Postgres treats `LIMIT NULL`
        // as "no limit" (equivalent to omitting it), so no conditional SQL
        // branch is needed — just a binding that is always present, which
        // keeps the placeholder count fixed.
        let sql = format!(
            "SELECT a.id, a.width, a.height, a.taken_at_utc FROM assets a \
             JOIN folders f ON f.id = a.folder_id \
             {STACK_PRIMARY_JOIN_SQL} \
             WHERE {} \
               AND a.status = 'indexed' \
               AND a.kind <> 'unknown' \
               AND a.taken_at_utc IS NOT NULL \
               AND ($4::uuid IS NULL OR f.library_id = $4) \
               AND ($5::timestamptz IS NULL \
                    OR a.taken_at_utc < $5 \
                    OR (a.taken_at_utc = $5 AND a.id < $6)) \
               AND {STACK_PRIMARY_ONLY_SQL} \
             ORDER BY a.taken_at_utc DESC, a.id DESC \
             LIMIT $7",
            filter.sql()
        );
        let rows: Vec<GeometryRow> = sqlx::query_as(&sql)
            .bind(filter.bind())
            .bind(filter.holes())
            .bind(filter.assets())
            .bind(library_id.map(|id| id.as_uuid()))
            .bind(cursor_time)
            .bind(cursor_id)
            .bind(limit)
            .fetch_all(self.db.pool())
            .await?;
        // If `page` is set and the response arrives at exactly the
        // requested (clamped) `limit`, there might be more after it: the
        // HTTP caller signals this with the last row's cursor. If the
        // response is shorter (or `page` is `None`), that was the whole
        // view.
        let next_cursor = limit
            .filter(|&l| usize::try_from(l).is_ok_and(|l| rows.len() == l))
            .and_then(|_| {
                rows.last()
                    .map(|(id, _, _, taken_at_utc)| (*taken_at_utc, AssetId::from_uuid(*id)))
            });
        let records = rows
            .into_iter()
            .map(|(_, width, height, taken_at_utc)| GeometryRecord {
                width,
                height,
                taken_at_utc,
            })
            .collect();

        // The ETag stamp only makes sense on the whole view: a paginated
        // (cold-start) request skips 304 validation and this query, which
        // would otherwise pay for an extra scan on every page without
        // ever using its result.
        let last_modified = if page.is_none() {
            let last_modified_sql = format!(
                "SELECT max(a.updated_at) FROM assets a \
                 JOIN folders f ON f.id = a.folder_id \
                 {STACK_PRIMARY_JOIN_SQL} \
                 WHERE {} \
                   AND a.status = 'indexed' \
                   AND a.kind <> 'unknown' \
                   AND a.taken_at_utc IS NOT NULL \
                   AND ($4::uuid IS NULL OR f.library_id = $4) \
                   AND {STACK_PRIMARY_ONLY_SQL}",
                filter.sql()
            );
            sqlx::query_scalar(&last_modified_sql)
                .bind(filter.bind())
                .bind(filter.holes())
                .bind(filter.assets())
                .bind(library_id.map(|id| id.as_uuid()))
                .fetch_one(self.db.pool())
                .await?
        } else {
            None
        };
        Ok(Geometry {
            records,
            last_modified,
            next_cursor,
        })
    }

    /// `count(*)` + `max(updated_at)` under the same filters as
    /// [`Self::geometry`], without reading `width`/`height`. Used to
    /// answer `304` without paying for a full scan of the view.
    ///
    /// # Errors
    /// Same as [`Self::geometry`].
    pub async fn geometry_stamp(
        &self,
        ctx: &AuthContext,
        library_id: Option<LibraryId>,
    ) -> Result<GeometryStamp, DbError> {
        if let Some(id) = library_id {
            LibraryRepo::new(self.db).find_by_id(ctx, id).await?;
        }
        let scope = VisibilityScope::resolve(self.db, ctx).await?;
        let filter = scope.filter("f.path", "f.library_id", "a.id", 1);
        let sql = format!(
            "SELECT count(*)::bigint, max(a.updated_at) FROM assets a \
             JOIN folders f ON f.id = a.folder_id \
             {STACK_PRIMARY_JOIN_SQL} \
             WHERE {} \
               AND a.status = 'indexed' \
               AND a.kind <> 'unknown' \
               AND a.taken_at_utc IS NOT NULL \
               AND ($4::uuid IS NULL OR f.library_id = $4) \
               AND {STACK_PRIMARY_ONLY_SQL}",
            filter.sql()
        );
        let (count, last_modified): (i64, Option<DateTime<Utc>>) = sqlx::query_as(&sql)
            .bind(filter.bind())
            .bind(filter.holes())
            .bind(filter.assets())
            .bind(library_id.map(|id| id.as_uuid()))
            .fetch_one(self.db.pool())
            .await?;
        Ok(GeometryStamp {
            count: u64::try_from(count).unwrap_or(0),
            last_modified,
        })
    }

    /// Like [`Self::geometry`], but restricted to a geographic bounding
    /// box. Filters `kind <> 'unknown'` like
    /// `buckets_in_bounds`/`page_in_bounds`: here the query touches
    /// `asset_overrides` anyway for the effective position, and there is
    /// no covering index to preserve like in the unfiltered case.
    ///
    /// # Errors
    /// `Forbidden` / `Connection` same as [`Self::geometry`].
    pub async fn geometry_in_bounds(
        &self,
        ctx: &AuthContext,
        library_id: Option<LibraryId>,
        bounds: MapBounds,
        page: Option<GeometryPage>,
    ) -> Result<Geometry, DbError> {
        if let Some(id) = library_id {
            LibraryRepo::new(self.db).find_by_id(ctx, id).await?;
        }
        let scope = VisibilityScope::resolve(self.db, ctx).await?;
        let filter = scope.filter("f.path", "f.library_id", "a.id", 1);
        let bbox = effective_bbox_filter_sql(5, 6, 7, 8);
        let (cursor_time, cursor_id, limit) = page.map_or((None, None, None), |p| {
            (
                p.after.map(|(t, _)| t),
                p.after.map(|(_, id)| id.as_uuid()),
                Some(p.limit.clamp(1, GEOMETRY_PAGE_LIMIT_MAX)),
            )
        });
        // Same as in `geometry`: `LIMIT $11` always present, `NULL` = no
        // limit — no conditional SQL branch.
        let sql = format!(
            "SELECT a.id, a.width, a.height, a.taken_at_utc FROM assets a \
             JOIN folders f ON f.id = a.folder_id \
             LEFT JOIN asset_overrides o ON o.asset_id = a.id \
             {STACK_PRIMARY_JOIN_SQL} \
             WHERE {} \
               AND a.status = 'indexed' \
               AND a.kind <> 'unknown' \
               AND a.taken_at_utc IS NOT NULL \
               AND ($4::uuid IS NULL OR f.library_id = $4) \
               AND ({bbox}) \
               AND ($9::timestamptz IS NULL \
                    OR a.taken_at_utc < $9 \
                    OR (a.taken_at_utc = $9 AND a.id < $10)) \
               AND {STACK_PRIMARY_ONLY_SQL} \
             ORDER BY a.taken_at_utc DESC, a.id DESC \
             LIMIT $11",
            filter.sql()
        );
        let rows: Vec<GeometryRow> = sqlx::query_as(&sql)
            .bind(filter.bind())
            .bind(filter.holes())
            .bind(filter.assets())
            .bind(library_id.map(|id| id.as_uuid()))
            .bind(bounds.west)
            .bind(bounds.south)
            .bind(bounds.east)
            .bind(bounds.north)
            .bind(cursor_time)
            .bind(cursor_id)
            .bind(limit)
            .fetch_all(self.db.pool())
            .await?;
        let next_cursor = limit
            .filter(|&l| usize::try_from(l).is_ok_and(|l| rows.len() == l))
            .and_then(|_| {
                rows.last()
                    .map(|(id, _, _, taken_at_utc)| (*taken_at_utc, AssetId::from_uuid(*id)))
            });
        let records = rows
            .into_iter()
            .map(|(_, width, height, taken_at_utc)| GeometryRecord {
                width,
                height,
                taken_at_utc,
            })
            .collect();

        let last_modified = if page.is_none() {
            let last_modified_sql = format!(
                "SELECT max(a.updated_at) FROM assets a \
                 JOIN folders f ON f.id = a.folder_id \
                 LEFT JOIN asset_overrides o ON o.asset_id = a.id \
                 {STACK_PRIMARY_JOIN_SQL} \
                 WHERE {} \
                   AND a.status = 'indexed' \
                   AND a.kind <> 'unknown' \
                   AND a.taken_at_utc IS NOT NULL \
                   AND ($4::uuid IS NULL OR f.library_id = $4) \
                   AND ({bbox}) \
                   AND {STACK_PRIMARY_ONLY_SQL}",
                filter.sql()
            );
            sqlx::query_scalar(&last_modified_sql)
                .bind(filter.bind())
                .bind(filter.holes())
                .bind(filter.assets())
                .bind(library_id.map(|id| id.as_uuid()))
                .bind(bounds.west)
                .bind(bounds.south)
                .bind(bounds.east)
                .bind(bounds.north)
                .fetch_one(self.db.pool())
                .await?
        } else {
            None
        };
        Ok(Geometry {
            records,
            last_modified,
            next_cursor,
        })
    }

    /// Lightweight stamp under the same filters as [`Self::geometry_in_bounds`].
    ///
    /// # Errors
    /// Same as [`Self::geometry_in_bounds`].
    pub async fn geometry_stamp_in_bounds(
        &self,
        ctx: &AuthContext,
        library_id: Option<LibraryId>,
        bounds: MapBounds,
    ) -> Result<GeometryStamp, DbError> {
        if let Some(id) = library_id {
            LibraryRepo::new(self.db).find_by_id(ctx, id).await?;
        }
        let scope = VisibilityScope::resolve(self.db, ctx).await?;
        let filter = scope.filter("f.path", "f.library_id", "a.id", 1);
        let bbox = effective_bbox_filter_sql(5, 6, 7, 8);
        let sql = format!(
            "SELECT count(*)::bigint, max(a.updated_at) FROM assets a \
             JOIN folders f ON f.id = a.folder_id \
             LEFT JOIN asset_overrides o ON o.asset_id = a.id \
             {STACK_PRIMARY_JOIN_SQL} \
             WHERE {} \
               AND a.status = 'indexed' \
               AND a.kind <> 'unknown' \
               AND a.taken_at_utc IS NOT NULL \
               AND ($4::uuid IS NULL OR f.library_id = $4) \
               AND ({bbox}) \
               AND {STACK_PRIMARY_ONLY_SQL}",
            filter.sql()
        );
        let (count, last_modified): (i64, Option<DateTime<Utc>>) = sqlx::query_as(&sql)
            .bind(filter.bind())
            .bind(filter.holes())
            .bind(filter.assets())
            .bind(library_id.map(|id| id.as_uuid()))
            .bind(bounds.west)
            .bind(bounds.south)
            .bind(bounds.east)
            .bind(bounds.north)
            .fetch_one(self.db.pool())
            .await?;
        Ok(GeometryStamp {
            count: u64::try_from(count).unwrap_or(0),
            last_modified,
        })
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
