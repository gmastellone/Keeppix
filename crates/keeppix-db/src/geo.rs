//! Aggregazione geografica per la mappa. Le coordinate effettive applicano
//! sempre gli override utente prima del valore EXIF dell'asset.

use std::path::Path;

use chrono::{DateTime, Utc};
use keeppix_domain::{AlbumId, AssetId, AuthContext, BatchId, FolderId, GeoPoint, LibraryId};
use sqlx::PgConnection;
use tokio::io::{AsyncBufReadExt as _, BufReader};

use crate::search::{SearchBind, compile_for_sql};
use crate::{AlbumRepo, Db, DbError, FolderRepo, SearchRepo, VisibilityScope};

pub const UNCLUSTERED_ZOOM: u8 = 15;
pub const MAX_UNCLUSTERED_POINTS: usize = 500;
const TIMEZONE_IMPORT_BATCH_SIZE: usize = 100;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimezoneChange {
    pub asset_id: AssetId,
    pub filename: String,
    pub before: DateTime<Utc>,
    pub after: DateTime<Utc>,
    pub timezone: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimezoneChangePreview {
    pub count: usize,
    pub example: Option<TimezoneChange>,
}

#[derive(sqlx::FromRow)]
struct ClusterRow {
    lon: f64,
    lat: f64,
    count: i64,
    cover_asset_id: uuid::Uuid,
}

#[derive(sqlx::FromRow)]
struct TimezoneChangeRow {
    asset_id: uuid::Uuid,
    filename: String,
    before: DateTime<Utc>,
    after: DateTime<Utc>,
    timezone: String,
}

#[derive(sqlx::FromRow)]
struct TimezonePreviewRow {
    asset_id: uuid::Uuid,
    filename: String,
    before: DateTime<Utc>,
    after: DateTime<Utc>,
    timezone: String,
    total_count: i64,
}

pub struct GeoRepo<'a> {
    db: &'a Db,
}

impl<'a> GeoRepo<'a> {
    #[must_use]
    pub const fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// Restituisce il nome IANA del poligono che contiene il punto.
    ///
    /// Non prende un `AuthContext`: i confini dei fusi sono un catalogo
    /// globale incorporato nell'immagine, non dati appartenenti a un utente.
    /// `LIMIT 1` rende tollerante il lookup anche se un dataset corrotto
    /// contenesse poligoni sovrapposti.
    ///
    /// # Errors
    /// `Connection` se la query fallisce.
    pub async fn timezone_for(&self, point: GeoPoint) -> Result<Option<String>, DbError> {
        let match_sql = timezone_match_sql("boundary.boundary", "input.point");
        let sql = format!(
            "WITH input(point) AS ( \
                 VALUES (ST_SetSRID(ST_MakePoint($1, $2), 4326)::geography) \
             ) \
             SELECT boundary.tz_name \
               FROM tz_boundaries boundary \
               CROSS JOIN input \
              WHERE {match_sql} \
              ORDER BY boundary.tz_name \
              LIMIT 1"
        );
        sqlx::query_scalar(&sql)
            .bind(point.lon)
            .bind(point.lat)
            .fetch_optional(self.db.pool())
            .await
            .map_err(DbError::from)
    }

    /// Importa il TSV normalizzato dei confini solo se il catalogo è vuoto.
    /// Un file assente è normale fuori dall'immagine Docker; un file presente
    /// ma malformato fallisce senza lasciare un import parziale.
    ///
    /// Non prende un `AuthContext`: è bootstrap amministrativo di un catalogo
    /// globale, non accesso a dati utente.
    ///
    /// # Errors
    /// `Io` se il file presente non è leggibile; `Corrupted` se una riga non
    /// contiene `tz_name` e una geometria `GeoJSON` `MultiPolygon`;
    /// `Connection` se l'import database fallisce.
    pub async fn seed_timezones_from_csv_if_empty(&self, path: &Path) -> Result<usize, DbError> {
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM tz_boundaries")
            .fetch_one(self.db.pool())
            .await?;
        if count != 0 {
            return Ok(0);
        }

        let file = match tokio::fs::File::open(path).await {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
            Err(error) => {
                return Err(DbError::Io(format!(
                    "cannot open timezone boundary CSV {}: {error}",
                    path.display()
                )));
            }
        };

        let mut lines = BufReader::new(file).lines();
        let mut transaction = self.db.pool().begin().await?;
        let mut batch = Vec::with_capacity(TIMEZONE_IMPORT_BATCH_SIZE);
        let mut imported = 0_usize;
        let mut line_number = 0_usize;
        while let Some(line) = lines.next_line().await.map_err(|error| {
            DbError::Io(format!(
                "cannot read timezone boundary CSV {}: {error}",
                path.display()
            ))
        })? {
            line_number += 1;
            if line.is_empty() {
                continue;
            }
            batch.push(parse_timezone_boundary(&line, line_number)?);
            if batch.len() == TIMEZONE_IMPORT_BATCH_SIZE {
                upsert_timezone_batch(&mut transaction, &batch).await?;
                imported += batch.len();
                batch.clear();
            }
        }
        if !batch.is_empty() {
            upsert_timezone_batch(&mut transaction, &batch).await?;
            imported += batch.len();
        }
        if imported == 0 {
            return Err(DbError::Corrupted(
                "timezone boundary CSV contains no rows".to_owned(),
            ));
        }
        transaction.commit().await?;
        Ok(imported)
    }

    /// Calcola conteggio e primo esempio senza materializzare tutti i candidati.
    ///
    /// # Errors
    /// `Forbidden` se la libreria non appartiene al chiamante; `Connection` se
    /// la query fallisce; `Corrupted` se `PostgreSQL` restituisce un conteggio
    /// non rappresentabile.
    pub async fn timezone_change_preview(
        &self,
        ctx: &AuthContext,
        library_id: LibraryId,
    ) -> Result<TimezoneChangePreview, DbError> {
        crate::LibraryRepo::new(self.db)
            .find_by_id(ctx, library_id)
            .await?;

        let sql = format!(
            "{} \
             SELECT asset_id, filename, before, after, timezone, \
                    count(*) OVER () AS total_count \
               FROM candidates \
              WHERE after IS DISTINCT FROM before \
              ORDER BY asset_id \
              LIMIT 1",
            timezone_candidates_sql()
        );
        let row: Option<TimezonePreviewRow> = sqlx::query_as(&sql)
            .bind(library_id.as_uuid())
            .fetch_optional(self.db.pool())
            .await?;
        let Some(row) = row else {
            return Ok(TimezoneChangePreview {
                count: 0,
                example: None,
            });
        };
        let count = usize::try_from(row.total_count)
            .map_err(|error| DbError::Corrupted(format!("timezone preview count: {error}")))?;
        Ok(TimezoneChangePreview {
            count,
            example: Some(timezone_change(row)),
        })
    }

    /// Calcola e scrive le correzioni nella stessa transazione annullabile.
    ///
    /// La scrittura ricontrolla inoltre `asset_overrides.taken_at` nel suo
    /// `ON CONFLICT`, quindi un override concorrente vince anche se nasce dopo
    /// la lettura dei candidati.
    ///
    /// # Errors
    /// `Forbidden` se la libreria non appartiene al chiamante; `Connection` se
    /// la transazione fallisce.
    pub async fn apply_timezone_changes(
        &self,
        ctx: &AuthContext,
        library_id: LibraryId,
    ) -> Result<(usize, Option<BatchId>, Vec<AssetId>), DbError> {
        crate::LibraryRepo::new(self.db)
            .find_by_id(ctx, library_id)
            .await?;
        let actor = ctx.user_id().ok_or(DbError::Forbidden)?;
        let mut transaction = self.db.pool().begin().await?;
        let changes = timezone_changes_on(&mut transaction, library_id).await?;
        let succeeded: Vec<AssetId> = changes.iter().map(|change| change.asset_id).collect();
        let assignments: Vec<_> = changes
            .iter()
            .map(|change| (change.asset_id, change.after))
            .collect();
        let (changed_count, batch_id) =
            crate::overrides::apply_taken_at_assignments_in_transaction(
                &mut transaction,
                &assignments,
                actor.as_uuid(),
            )
            .await?;
        transaction.commit().await?;
        if changed_count != 0 {
            crate::overrides::enqueue_sidecar_sweep(self.db).await?;
        }
        Ok((changed_count, batch_id, succeeded))
    }

    /// Calcola le correzioni applicabili a una libreria senza scrivere nulla.
    ///
    /// Il timestamp originale viene trattato come quadrante ingenuo salvato
    /// in UTC: `14:00Z` presso Tokyo significa «14:00 locale», quindi diventa
    /// `05:00Z`. Gli override di posizione precedono il GPS EXIF; un override
    /// `taken_at` esistente appartiene invece all'utente e viene escluso.
    ///
    /// # Errors
    /// `Forbidden` se la libreria non appartiene al chiamante; `NotFound` solo
    /// per un admin su un id inesistente; `Connection` se la query fallisce.
    pub async fn timezone_changes(
        &self,
        ctx: &AuthContext,
        library_id: LibraryId,
    ) -> Result<Vec<TimezoneChange>, DbError> {
        crate::LibraryRepo::new(self.db)
            .find_by_id(ctx, library_id)
            .await?;

        let rows: Vec<TimezoneChangeRow> = sqlx::query_as(&timezone_changes_sql())
            .bind(library_id.as_uuid())
            .fetch_all(self.db.pool())
            .await?;
        Ok(rows.into_iter().map(timezone_change_from_row).collect())
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

fn timezone_match_sql(boundary: &str, point: &str) -> String {
    format!("{boundary} && {point} AND ST_Covers({boundary}, {point})")
}

fn timezone_candidates_sql() -> String {
    let match_sql = timezone_match_sql("boundary.boundary", "COALESCE(o.location, a.location)");
    format!(
        "WITH candidates AS ( \
             SELECT a.id AS asset_id, a.filename, a.taken_at_utc AS before, \
                    ((a.taken_at_utc AT TIME ZONE 'UTC') \
                        AT TIME ZONE timezone.tz_name) AS after, \
                    timezone.tz_name AS timezone \
               FROM assets a \
               JOIN folders f ON f.id = a.folder_id \
               LEFT JOIN asset_overrides o ON o.asset_id = a.id \
               JOIN LATERAL ( \
                   SELECT boundary.tz_name \
                     FROM tz_boundaries boundary \
                    WHERE {match_sql} \
                    ORDER BY boundary.tz_name \
                    LIMIT 1 \
               ) timezone ON true \
              WHERE f.library_id = $1 \
                AND a.status = 'indexed' \
                AND a.taken_at_utc IS NOT NULL \
                AND o.taken_at IS NULL \
                AND COALESCE(o.location, a.location) IS NOT NULL \
         )"
    )
}

fn timezone_changes_sql() -> String {
    format!(
        "{} \
         SELECT asset_id, filename, before, after, timezone \
           FROM candidates \
          WHERE after IS DISTINCT FROM before \
          ORDER BY asset_id",
        timezone_candidates_sql()
    )
}

async fn timezone_changes_on(
    connection: &mut PgConnection,
    library_id: LibraryId,
) -> Result<Vec<TimezoneChange>, DbError> {
    let rows: Vec<TimezoneChangeRow> = sqlx::query_as(&timezone_changes_sql())
        .bind(library_id.as_uuid())
        .fetch_all(connection)
        .await?;
    Ok(rows.into_iter().map(timezone_change_from_row).collect())
}

fn timezone_change_from_row(row: TimezoneChangeRow) -> TimezoneChange {
    TimezoneChange {
        asset_id: AssetId::from_uuid(row.asset_id),
        filename: row.filename,
        before: row.before,
        after: row.after,
        timezone: row.timezone,
    }
}

fn timezone_change(row: TimezonePreviewRow) -> TimezoneChange {
    TimezoneChange {
        asset_id: AssetId::from_uuid(row.asset_id),
        filename: row.filename,
        before: row.before,
        after: row.after,
        timezone: row.timezone,
    }
}

fn parse_timezone_boundary(line: &str, line_number: usize) -> Result<(String, String), DbError> {
    let Some((tz_name, geojson)) = line.split_once('\t') else {
        return Err(corrupted_timezone_line(
            line_number,
            "expected two tab-separated columns",
        ));
    };
    if tz_name.is_empty() {
        return Err(corrupted_timezone_line(line_number, "empty timezone name"));
    }
    let geometry: serde_json::Value = serde_json::from_str(geojson)
        .map_err(|error| corrupted_timezone_line(line_number, error))?;
    if geometry.get("type").and_then(serde_json::Value::as_str) != Some("MultiPolygon")
        || !geometry
            .get("coordinates")
            .is_some_and(serde_json::Value::is_array)
    {
        return Err(corrupted_timezone_line(
            line_number,
            "geometry must be a GeoJSON MultiPolygon",
        ));
    }
    Ok((tz_name.to_owned(), geojson.to_owned()))
}

fn corrupted_timezone_line(line_number: usize, detail: impl std::fmt::Display) -> DbError {
    DbError::Corrupted(format!(
        "timezone boundary CSV line {line_number}: {detail}"
    ))
}

async fn upsert_timezone_batch(
    connection: &mut PgConnection,
    boundaries: &[(String, String)],
) -> Result<(), DbError> {
    let names: Vec<&str> = boundaries.iter().map(|(name, _)| name.as_str()).collect();
    let unknown_name: Option<String> = sqlx::query_scalar(
        "SELECT input.tz_name \
           FROM unnest($1::text[]) AS input(tz_name) \
          WHERE NOT EXISTS ( \
                    SELECT 1 FROM pg_timezone_names known WHERE known.name = input.tz_name \
                ) \
          ORDER BY input.tz_name \
          LIMIT 1",
    )
    .bind(&names)
    .fetch_optional(&mut *connection)
    .await?;
    if let Some(name) = unknown_name {
        return Err(DbError::Corrupted(format!(
            "timezone boundary CSV contains unknown IANA timezone name {name:?}"
        )));
    }
    let geometries: Vec<&str> = boundaries
        .iter()
        .map(|(_, geometry)| geometry.as_str())
        .collect();
    sqlx::query(
        "INSERT INTO tz_boundaries (tz_name, boundary) \
         SELECT input.tz_name, \
                ST_Multi(ST_CollectionExtract(ST_MakeValid( \
                    ST_SetSRID(ST_GeomFromGeoJSON(input.geojson), 4326) \
                ), 3))::geography \
           FROM unnest($1::text[], $2::text[]) AS input(tz_name, geojson) \
         ON CONFLICT (tz_name) DO UPDATE SET boundary = EXCLUDED.boundary",
    )
    .bind(&names)
    .bind(&geometries)
    .execute(connection)
    .await?;
    Ok(())
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
    use super::{bbox_filter_sql, timezone_match_sql};

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

    #[test]
    fn timezone_match_keeps_the_geography_column_bare_for_gist() {
        let sql = timezone_match_sql("boundary.boundary", "$1");

        assert_eq!(
            sql,
            "boundary.boundary && $1 AND ST_Covers(boundary.boundary, $1)"
        );
        assert!(!sql.contains("::geometry"));
    }
}
