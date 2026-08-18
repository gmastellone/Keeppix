use std::path::Path;

use keeppix_domain::{AuthContext, GeoPoint, Place};
use sqlx::{PgConnection, Row as _};
use tokio::io::{AsyncBufReadExt as _, BufReader};

use crate::{Db, DbError};

pub struct PlaceRepo<'a> {
    db: &'a Db,
}

const COLUMNS: &str = "p.id, p.name, p.ascii_name, p.country_code::text AS country_code, \
                       p.admin1, p.admin2, ST_X(p.location::geometry) AS lon, \
                       ST_Y(p.location::geometry) AS lat, p.population";
const IMPORT_BATCH_SIZE: usize = 1_000;
const REGION_FALLBACK_RADIUS_M: f64 = 200_000.0;
const COUNTRY_FALLBACK_RADIUS_M: f64 = 1_000_000.0;
const USER_HISTORY_BOOST_RADIUS_M: f64 = 250_000.0;

#[derive(sqlx::FromRow)]
struct PlaceRow {
    id: i64,
    name: String,
    ascii_name: String,
    country_code: Option<String>,
    admin1: Option<String>,
    admin2: Option<String>,
    lon: f64,
    lat: f64,
    population: i32,
}

impl PlaceRow {
    fn into_domain(self) -> Place {
        Place {
            id: self.id,
            name: self.name,
            ascii_name: self.ascii_name,
            country_code: self.country_code,
            admin1: self.admin1,
            admin2: self.admin2,
            location: GeoPoint {
                lat: self.lat,
                lon: self.lon,
            },
            population: self.population,
        }
    }
}

impl<'a> PlaceRepo<'a> {
    #[must_use]
    pub const fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// Non prende un `AuthContext`: `GeoNames` è un catalogo globale, non un
    /// dato appartenente a un utente. La scrittura è riservata alla pipeline
    /// di import amministrativa.
    ///
    /// # Errors
    /// `Connection` se la scrittura fallisce.
    pub async fn upsert(&self, place: &Place) -> Result<(), DbError> {
        let mut connection = self.db.pool().acquire().await?;
        upsert_batch(&mut connection, std::slice::from_ref(place)).await
    }

    /// Reverse geocoding sul catalogo globale.
    ///
    /// Le località ordinarie hanno una soglia interpolata linearmente fra
    /// 3 km a 600 abitanti e 25 km a 500.000 abitanti, con clamp ai due
    /// estremi. Le righe amministrative hanno `population = 0`: `admin1`
    /// valorizzato identifica una regione, entrambi i campi amministrativi
    /// null identificano una nazione.
    ///
    /// Non prende un `AuthContext`: le località `GeoNames` sono dati globali,
    /// non dati di un utente.
    ///
    /// # Errors
    /// `Connection` se la query fallisce.
    pub async fn nearest(&self, point: GeoPoint) -> Result<Option<Place>, DbError> {
        let row: Option<PlaceRow> = sqlx::query_as(&format!(
            "WITH query_point AS (
                 SELECT ST_SetSRID(ST_MakePoint($1, $2), 4326)::geography AS location
             ),
             candidates AS (
                 SELECT {COLUMNS},
                        0 AS fallback_rank,
                        p.location <-> q.location AS distance
                 FROM places p
                 CROSS JOIN query_point q
                 WHERE p.population > 0
                   AND ST_DWithin(
                       p.location,
                       q.location,
                       3000.0 + (
                           LEAST(GREATEST(p.population, 600), 500000) - 600
                       )::double precision * (22000.0 / 499400.0)
                   )
                 UNION ALL
                 SELECT {COLUMNS},
                        1 AS fallback_rank,
                        p.location <-> q.location AS distance
                 FROM places p
                 CROSS JOIN query_point q
                 WHERE p.population = 0
                   AND p.admin1 IS NOT NULL
                   AND p.admin2 IS NULL
                   AND ST_DWithin(p.location, q.location, $3)
                 UNION ALL
                 SELECT {COLUMNS},
                        2 AS fallback_rank,
                        p.location <-> q.location AS distance
                 FROM places p
                 CROSS JOIN query_point q
                 WHERE p.population = 0
                   AND p.admin1 IS NULL
                   AND p.admin2 IS NULL
                   AND ST_DWithin(p.location, q.location, $4)
             )
             SELECT id, name, ascii_name, country_code, admin1, admin2,
                    lon, lat, population
             FROM candidates
             ORDER BY fallback_rank, distance, id
             LIMIT 1"
        ))
        .bind(point.lon)
        .bind(point.lat)
        .bind(REGION_FALLBACK_RADIUS_M)
        .bind(COUNTRY_FALLBACK_RADIUS_M)
        .fetch_optional(self.db.pool())
        .await?;
        Ok(row.map(PlaceRow::into_domain))
    }

    /// Cerca nel catalogo globale e, se richiesto, favorisce i risultati entro
    /// 250 km dal centroide delle ultime 50 posizioni assegnate dal chiamante.
    ///
    /// Prende `AuthContext` come primo parametro perché legge
    /// `asset_overrides.updated_by`; con cronologia vuota l'ordinamento torna
    /// alla popolazione.
    ///
    /// # Errors
    /// `Forbidden` per un attore che non è un utente; `Connection` se la query
    /// fallisce.
    pub async fn search(
        &self,
        ctx: &AuthContext,
        query: &str,
        near_user: bool,
        limit: usize,
    ) -> Result<Vec<Place>, DbError> {
        let user_id = ctx.user_id().ok_or(DbError::Forbidden)?;
        let limit = i64::try_from(limit.min(10))
            .map_err(|error| DbError::Corrupted(format!("invalid place search limit: {error}")))?;
        let rows: Vec<PlaceRow> = sqlx::query_as(&format!(
            "WITH recent AS (
                 SELECT location
                 FROM asset_overrides
                 WHERE $2
                   AND updated_by = $1
                   AND location IS NOT NULL
                 ORDER BY updated_at DESC
                 LIMIT 50
             ),
             history AS (
                 SELECT ST_Centroid(ST_Collect(location::geometry))::geography AS centroid
                 FROM recent
             )
             SELECT {COLUMNS}
             FROM places p
             CROSS JOIN history h
             WHERE p.population > 0
               AND (p.ascii_name ILIKE '%' || $3 || '%' OR p.ascii_name % $3)
             ORDER BY similarity(p.ascii_name, $3) DESC,
                      CASE
                          WHEN h.centroid IS NOT NULL
                           AND ST_DWithin(p.location, h.centroid, $4)
                          THEN 1
                          ELSE 0
                      END DESC,
                      p.population DESC,
                      p.id
             LIMIT $5"
        ))
        .bind(user_id.as_uuid())
        .bind(near_user)
        .bind(query)
        .bind(USER_HISTORY_BOOST_RADIUS_M)
        .bind(limit)
        .fetch_all(self.db.pool())
        .await?;
        Ok(rows.into_iter().map(PlaceRow::into_domain).collect())
    }

    /// Importa il CSV normalizzato solo se la tabella è vuota. Un file
    /// assente è normale fuori dall'immagine Docker e non è un errore.
    ///
    /// Non prende un `AuthContext`: è bootstrap di un catalogo globale da
    /// parte della pipeline amministrativa, non accesso a dati utente.
    ///
    /// # Errors
    /// `Connection` se il database fallisce; `Io` se un file presente non è
    /// leggibile; `Corrupted` se una riga non rispetta il formato atteso.
    pub async fn seed_from_csv_if_empty(&self, path: &Path) -> Result<usize, DbError> {
        let count: i64 = sqlx::query("SELECT count(*) AS count FROM places")
            .fetch_one(self.db.pool())
            .await?
            .try_get("count")?;
        if count != 0 {
            return Ok(0);
        }

        let file = match tokio::fs::File::open(path).await {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
            Err(error) => {
                return Err(DbError::Io(format!(
                    "cannot open GeoNames CSV {}: {error}",
                    path.display()
                )));
            }
        };

        let mut lines = BufReader::new(file).lines();
        let mut transaction = self.db.pool().begin().await?;
        let mut batch = Vec::with_capacity(IMPORT_BATCH_SIZE);
        let mut imported = 0_usize;
        let mut line_number = 0_usize;
        while let Some(line) = lines.next_line().await.map_err(|error| {
            DbError::Io(format!(
                "cannot read GeoNames CSV {}: {error}",
                path.display()
            ))
        })? {
            line_number += 1;
            if line.is_empty() {
                continue;
            }
            batch.push(parse_place(&line, line_number)?);
            if batch.len() == IMPORT_BATCH_SIZE {
                upsert_batch(&mut transaction, &batch).await?;
                imported += batch.len();
                batch.clear();
            }
        }
        if !batch.is_empty() {
            upsert_batch(&mut transaction, &batch).await?;
            imported += batch.len();
        }
        transaction.commit().await?;
        Ok(imported)
    }
}

fn parse_place(line: &str, line_number: usize) -> Result<Place, DbError> {
    let columns: Vec<&str> = line.split('\t').collect();
    if columns.len() != 9 {
        return Err(corrupted_line(
            line_number,
            format!("expected 9 tab-separated columns, found {}", columns.len()),
        ));
    }

    let id = parse_column(columns[0], line_number, "id")?;
    let country_code = optional(columns[3]);
    if country_code.as_ref().is_some_and(|code| code.len() != 2) {
        return Err(corrupted_line(
            line_number,
            "country_code must contain exactly two bytes",
        ));
    }
    let lat = parse_column(columns[6], line_number, "latitude")?;
    let lon = parse_column(columns[7], line_number, "longitude")?;
    if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) {
        return Err(corrupted_line(
            line_number,
            "coordinates are outside WGS84 bounds",
        ));
    }

    Ok(Place {
        id,
        name: columns[1].to_owned(),
        ascii_name: columns[2].to_owned(),
        country_code,
        admin1: optional(columns[4]),
        admin2: optional(columns[5]),
        location: GeoPoint { lat, lon },
        population: parse_column(columns[8], line_number, "population")?,
    })
}

fn parse_column<T>(raw: &str, line_number: usize, name: &str) -> Result<T, DbError>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    raw.parse().map_err(|error| {
        corrupted_line(
            line_number,
            format!("invalid {name} value {raw:?}: {error}"),
        )
    })
}

fn optional(raw: &str) -> Option<String> {
    (!raw.is_empty()).then(|| raw.to_owned())
}

fn corrupted_line(line_number: usize, detail: impl std::fmt::Display) -> DbError {
    DbError::Corrupted(format!("GeoNames CSV line {line_number}: {detail}"))
}

async fn upsert_batch(connection: &mut PgConnection, places: &[Place]) -> Result<(), DbError> {
    if places.is_empty() {
        return Ok(());
    }

    let ids: Vec<i64> = places.iter().map(|place| place.id).collect();
    let names: Vec<&str> = places.iter().map(|place| place.name.as_str()).collect();
    let ascii_names: Vec<&str> = places
        .iter()
        .map(|place| place.ascii_name.as_str())
        .collect();
    let country_codes: Vec<Option<&str>> = places
        .iter()
        .map(|place| place.country_code.as_deref())
        .collect();
    let admin1: Vec<Option<&str>> = places.iter().map(|place| place.admin1.as_deref()).collect();
    let admin2: Vec<Option<&str>> = places.iter().map(|place| place.admin2.as_deref()).collect();
    let longitudes: Vec<f64> = places.iter().map(|place| place.location.lon).collect();
    let latitudes: Vec<f64> = places.iter().map(|place| place.location.lat).collect();
    let populations: Vec<i32> = places.iter().map(|place| place.population).collect();

    sqlx::query(
        "INSERT INTO places (
             id, name, ascii_name, country_code, admin1, admin2, location, population
         )
         SELECT input.id, input.name, input.ascii_name, input.country_code,
                input.admin1, input.admin2,
                ST_SetSRID(ST_MakePoint(input.lon, input.lat), 4326)::geography,
                input.population
         FROM UNNEST(
             $1::bigint[], $2::text[], $3::text[], $4::text[],
             $5::text[], $6::text[], $7::double precision[],
             $8::double precision[], $9::int[]
         ) AS input(
             id, name, ascii_name, country_code, admin1, admin2,
             lon, lat, population
         )
         ON CONFLICT (id) DO UPDATE SET
             name = EXCLUDED.name,
             ascii_name = EXCLUDED.ascii_name,
             country_code = EXCLUDED.country_code,
             admin1 = EXCLUDED.admin1,
             admin2 = EXCLUDED.admin2,
             location = EXCLUDED.location,
             population = EXCLUDED.population",
    )
    .bind(ids)
    .bind(names)
    .bind(ascii_names)
    .bind(country_codes)
    .bind(admin1)
    .bind(admin2)
    .bind(longitudes)
    .bind(latitudes)
    .bind(populations)
    .execute(connection)
    .await?;
    Ok(())
}
