use std::path::Path;

use keeppix_domain::{GeoPoint, Place};
use sqlx::{PgConnection, Row as _};
use tokio::io::{AsyncBufReadExt as _, BufReader};

use crate::{Db, DbError};

pub struct PlaceRepo<'a> {
    db: &'a Db,
}

const COLUMNS: &str = "id, name, ascii_name, country_code::text AS country_code, \
                       admin1, admin2, ST_X(location::geometry) AS lon, \
                       ST_Y(location::geometry) AS lat, population";
const IMPORT_BATCH_SIZE: usize = 1_000;

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

    /// Non prende un `AuthContext`: le località `GeoNames` sono dati globali,
    /// non dati di un utente.
    ///
    /// # Errors
    /// `Connection` se la query fallisce.
    pub async fn nearest(&self, point: GeoPoint) -> Result<Option<Place>, DbError> {
        let row: Option<PlaceRow> = sqlx::query_as(&format!(
            "SELECT {COLUMNS} FROM places \
             ORDER BY location <-> \
                 ST_SetSRID(ST_MakePoint($1, $2), 4326)::geography \
             LIMIT 1"
        ))
        .bind(point.lon)
        .bind(point.lat)
        .fetch_optional(self.db.pool())
        .await?;
        Ok(row.map(PlaceRow::into_domain))
    }

    /// Non prende un `AuthContext`: le località `GeoNames` sono dati globali,
    /// non dati di un utente.
    ///
    /// # Errors
    /// `Connection` se la query fallisce.
    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<Place>, DbError> {
        let limit = i64::try_from(limit.min(100))
            .map_err(|error| DbError::Corrupted(format!("invalid place search limit: {error}")))?;
        let rows: Vec<PlaceRow> = sqlx::query_as(&format!(
            "SELECT {COLUMNS} FROM places \
             WHERE ascii_name ILIKE '%' || $1 || '%' OR ascii_name % $1 \
             ORDER BY similarity(ascii_name, $1) DESC, population DESC, id \
             LIMIT $2"
        ))
        .bind(query)
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
