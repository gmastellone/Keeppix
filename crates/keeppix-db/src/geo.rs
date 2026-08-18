//! Aggregazione geografica per la mappa. Le coordinate effettive applicano
//! sempre gli override utente prima del valore EXIF dell'asset.

use keeppix_domain::{AlbumId, AssetId, AuthContext, FolderId, LibraryId};

use crate::search::{SearchBind, compile_for_sql};
use crate::{AlbumRepo, Db, DbError, FolderRepo, SearchRepo, VisibilityScope};

pub const UNCLUSTERED_ZOOM: u8 = 15;
pub const MAX_UNCLUSTERED_POINTS: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MapBounds {
    pub west: f64,
    pub south: f64,
    pub east: f64,
    pub north: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapScope {
    Library(LibraryId),
    Album(AlbumId),
    Folder(FolderId),
    Search(uuid::Uuid),
}

impl MapScope {
    const fn id(self) -> uuid::Uuid {
        match self {
            Self::Library(id) => id.as_uuid(),
            Self::Album(id) => id.as_uuid(),
            Self::Folder(id) => id.as_uuid(),
            Self::Search(id) => id,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MapCluster {
    pub lat: f64,
    pub lon: f64,
    pub count: i64,
    pub cover_asset_id: AssetId,
    pub clustered: bool,
}

#[derive(sqlx::FromRow)]
struct ClusterRow {
    lon: f64,
    lat: f64,
    count: i64,
    cover_asset_id: uuid::Uuid,
}

pub struct GeoRepo<'a> {
    db: &'a Db,
}

impl<'a> GeoRepo<'a> {
    #[must_use]
    pub const fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// Restituisce celle a griglia fino allo zoom 14. Dallo zoom 15 restituisce
    /// punti singoli se sono al massimo 500, altrimenti torna alla griglia.
    ///
    /// # Errors
    /// `Forbidden` se lo scope è sconosciuto o non visibile; `Connection` se
    /// una query fallisce; `Conflict` se una ricerca salvata è malformata.
    pub async fn clusters(
        &self,
        ctx: &AuthContext,
        bounds: MapBounds,
        zoom: u8,
        map_scope: MapScope,
    ) -> Result<Vec<MapCluster>, DbError> {
        let search = self.validate_scope(ctx, map_scope).await?;
        let visibility = VisibilityScope::resolve(self.db, ctx).await?;
        let filter = visibility.filter("f.path", "f.library_id", "a.id", 8);
        let mut next_param = 11_usize;
        let (search_clause, search_binds) = match search {
            Some(search) => compile_for_sql(
                &search,
                &mut next_param,
                0,
                "COALESCE(o.location, a.location)",
            )?,
            None => ("TRUE".to_owned(), Vec::new()),
        };
        let scope_clause = match map_scope {
            MapScope::Library(_) => "f.library_id = $7",
            MapScope::Album(_) => {
                "EXISTS (SELECT 1 FROM album_assets map_aa \
                         WHERE map_aa.album_id = $7 AND map_aa.asset_id = a.id)"
            }
            MapScope::Folder(_) => {
                "f.library_id = (SELECT library_id FROM folders WHERE id = $7) \
                 AND f.path <@ (SELECT path FROM folders WHERE id = $7)"
            }
            MapScope::Search(_) => "$7::uuid IS NOT NULL",
        };
        let query = ClusterQuery {
            bounds,
            cell_degrees: cell_degrees(zoom),
            scope_id: map_scope.id(),
            scope_clause,
            search_clause: &search_clause,
            search_binds: &search_binds,
            filter: &filter,
            viewer_id: ctx.user_id().ok_or(DbError::Forbidden)?.as_uuid(),
        };

        if zoom >= UNCLUSTERED_ZOOM {
            let rows = self.fetch_individual(&query).await?;
            if rows.len() <= MAX_UNCLUSTERED_POINTS {
                return Ok(into_clusters(rows, false));
            }
        }

        self.fetch_grid(&query)
            .await
            .map(|rows| into_clusters(rows, true))
    }

    async fn validate_scope(
        &self,
        ctx: &AuthContext,
        map_scope: MapScope,
    ) -> Result<Option<crate::SearchNode>, DbError> {
        match map_scope {
            MapScope::Library(id) => {
                self.assert_library_visible(ctx, id).await?;
                Ok(None)
            }
            MapScope::Album(id) => {
                AlbumRepo::new(self.db).get(ctx, id).await?;
                Ok(None)
            }
            MapScope::Folder(id) => {
                FolderRepo::new(self.db)
                    .find_by_id(ctx, id)
                    .await
                    .map_err(forbidden_if_missing)?;
                Ok(None)
            }
            MapScope::Search(id) => SearchRepo::new(self.db)
                .saved_query(ctx, id)
                .await
                .map(Some),
        }
    }

    async fn assert_library_visible(
        &self,
        ctx: &AuthContext,
        library_id: LibraryId,
    ) -> Result<(), DbError> {
        let visibility = VisibilityScope::resolve(self.db, ctx).await?;
        let filter = visibility.filter("f.path", "f.library_id", "a.id", 2);
        let sql = format!(
            "SELECT EXISTS( \
               SELECT 1 FROM folders f \
               LEFT JOIN assets a ON a.folder_id = f.id \
                WHERE f.library_id = $1 AND {} \
             )",
            filter.sql()
        );
        let visible: bool = sqlx::query_scalar(&sql)
            .bind(library_id.as_uuid())
            .bind(filter.bind())
            .bind(filter.holes())
            .bind(filter.assets())
            .fetch_one(self.db.pool())
            .await?;
        if visible {
            Ok(())
        } else {
            Err(DbError::Forbidden)
        }
    }

    async fn fetch_individual(&self, query: &ClusterQuery<'_>) -> Result<Vec<ClusterRow>, DbError> {
        let sql = format!(
            "{} \
             SELECT ST_X(location) AS lon, ST_Y(location) AS lat, \
                    1::bigint AS count, id AS cover_asset_id \
               FROM candidates \
              ORDER BY taken_at_utc DESC NULLS LAST, id DESC \
              LIMIT 501",
            query.candidates_sql()
        );
        query
            .bind(sqlx::query_as(&sql))
            .fetch_all(self.db.pool())
            .await
            .map_err(DbError::from)
    }

    async fn fetch_grid(&self, query: &ClusterQuery<'_>) -> Result<Vec<ClusterRow>, DbError> {
        let sql = format!(
            "{} \
             SELECT ST_X(cell) AS lon, ST_Y(cell) AS lat, count(*)::bigint AS count, \
                    (array_agg(id ORDER BY COALESCE(rating, 0) DESC, \
                                           taken_at_utc DESC NULLS LAST, id DESC))[1] \
                        AS cover_asset_id \
               FROM ( \
                    SELECT ST_SnapToGrid(location, $6) AS cell, id, rating, taken_at_utc \
                      FROM candidates \
                    ) snapped \
              GROUP BY cell \
              ORDER BY cell",
            query.candidates_sql()
        );
        query
            .bind(sqlx::query_as(&sql))
            .fetch_all(self.db.pool())
            .await
            .map_err(DbError::from)
    }
}

struct ClusterQuery<'a> {
    bounds: MapBounds,
    cell_degrees: f64,
    scope_id: uuid::Uuid,
    scope_clause: &'a str,
    search_clause: &'a str,
    search_binds: &'a [SearchBind],
    filter: &'a crate::visibility::VisibilityFilter,
    viewer_id: uuid::Uuid,
}

impl ClusterQuery<'_> {
    fn candidates_sql(&self) -> String {
        format!(
            "WITH candidates AS ( \
               SELECT a.id, a.taken_at_utc, fl.rating, \
                      COALESCE(o.location, a.location)::geometry AS location \
                 FROM assets a \
                 JOIN folders f ON f.id = a.folder_id \
                 LEFT JOIN asset_overrides o ON o.asset_id = a.id \
                 LEFT JOIN asset_flags fl ON fl.asset_id = a.id AND fl.user_id = $1 \
                 LEFT JOIN asset_exif e ON e.asset_id = a.id \
                WHERE a.status = 'indexed' \
                  AND ({}) \
                  AND $6::double precision > 0 \
                  AND ({}) \
                  AND {} \
                  AND ({}) \
             )",
            bbox_filter_sql(),
            self.scope_clause,
            self.filter.sql(),
            self.search_clause
        )
    }

    fn bind<'q>(
        &'q self,
        mut query: sqlx::query::QueryAs<
            'q,
            sqlx::Postgres,
            ClusterRow,
            sqlx::postgres::PgArguments,
        >,
    ) -> sqlx::query::QueryAs<'q, sqlx::Postgres, ClusterRow, sqlx::postgres::PgArguments> {
        query = query
            .bind(self.viewer_id)
            .bind(self.bounds.west)
            .bind(self.bounds.south)
            .bind(self.bounds.east)
            .bind(self.bounds.north)
            .bind(self.cell_degrees)
            .bind(self.scope_id)
            .bind(self.filter.bind())
            .bind(self.filter.holes())
            .bind(self.filter.assets());
        for bind in self.search_binds {
            query = match bind {
                SearchBind::Text(value) => query.bind(value),
                SearchBind::I32(value) => query.bind(value),
                SearchBind::Uuid(value) => query.bind(value),
                SearchBind::Ts(value) => query.bind(value),
            };
        }
        query
    }
}

fn forbidden_if_missing(error: DbError) -> DbError {
    if matches!(error, DbError::NotFound) {
        DbError::Forbidden
    } else {
        error
    }
}

fn cell_degrees(zoom: u8) -> f64 {
    90.0 / 2_f64.powi(i32::from(zoom.min(30)))
}

const fn bbox_filter_sql() -> &'static str {
    "($2 <= $4 AND (\
         (o.location IS NOT NULL AND o.location \
          && ST_Segmentize(ST_MakeEnvelope($2, $3, $4, $5, 4326), 90.0)::geography) \
         OR (o.location IS NULL AND a.location \
             && ST_Segmentize(ST_MakeEnvelope($2, $3, $4, $5, 4326), 90.0)::geography)\
     )) OR ($2 > $4 AND (\
         (o.location IS NOT NULL AND (\
             o.location && ST_Segmentize(\
                 ST_MakeEnvelope($2, $3, 180.0, $5, 4326), 90.0\
             )::geography \
             OR o.location && ST_Segmentize(\
                 ST_MakeEnvelope(-180.0, $3, $4, $5, 4326), 90.0\
             )::geography\
         )) OR (o.location IS NULL AND (\
             a.location && ST_Segmentize(\
                 ST_MakeEnvelope($2, $3, 180.0, $5, 4326), 90.0\
             )::geography \
             OR a.location && ST_Segmentize(\
                 ST_MakeEnvelope(-180.0, $3, $4, $5, 4326), 90.0\
             )::geography\
         ))\
     ))"
}

fn into_clusters(rows: Vec<ClusterRow>, clustered: bool) -> Vec<MapCluster> {
    rows.into_iter()
        .map(|row| MapCluster {
            lat: row.lat,
            lon: row.lon,
            count: row.count,
            cover_asset_id: AssetId::from_uuid(row.cover_asset_id),
            clustered,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::bbox_filter_sql;

    #[test]
    fn bbox_filter_keeps_geography_columns_bare_for_gist() {
        let sql = bbox_filter_sql();

        assert!(sql.contains(
            "o.location IS NOT NULL AND o.location \
             && ST_Segmentize(ST_MakeEnvelope($2, $3, $4, $5, 4326), 90.0)::geography"
        ));
        assert!(sql.contains(
            "o.location IS NULL AND a.location \
             && ST_Segmentize(ST_MakeEnvelope($2, $3, $4, $5, 4326), 90.0)::geography"
        ));
        assert_eq!(
            sql.matches("o.location && ST_Segmentize").count(),
            3,
            "normal bounds plus both antimeridian envelopes"
        );
        assert_eq!(
            sql.matches("a.location && ST_Segmentize").count(),
            3,
            "normal bounds plus both antimeridian envelopes"
        );
        assert!(!sql.contains("COALESCE"));
        assert!(!sql.contains("::geometry"));
    }
}
