//! Per-user "home" point and geofencing on public payloads.

use keeppix_domain::{AssetId, AuthContext, GeoPoint};

use crate::{Db, DbError};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HomeLocation {
    pub point: GeoPoint,
    pub radius_m: i32,
}

/// Effective location metadata for public serialization.
#[derive(Debug, Clone, PartialEq)]
pub struct PublicAssetLocation {
    pub asset_id: AssetId,
    pub location: Option<GeoPoint>,
    pub place_id: Option<i64>,
    pub redact: bool,
}

pub struct HomeRepo<'a> {
    db: &'a Db,
}

impl<'a> HomeRepo<'a> {
    #[must_use]
    pub const fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// # Errors
    /// `Forbidden` without a user session; `Connection` on DB error.
    pub async fn get(&self, ctx: &AuthContext) -> Result<Option<HomeLocation>, DbError> {
        let Some(user_id) = ctx.user_id() else {
            return Err(DbError::Forbidden);
        };
        let row: Option<(f64, f64, i32)> = sqlx::query_as(
            "SELECT ST_Y(location::geometry), ST_X(location::geometry), radius_m \
             FROM user_home_locations WHERE user_id = $1",
        )
        .bind(user_id.as_uuid())
        .fetch_optional(self.db.pool())
        .await?;
        Ok(row.map(|(lat, lon, radius_m)| HomeLocation {
            point: GeoPoint { lat, lon },
            radius_m,
        }))
    }

    /// Sets or updates home for the authenticated user.
    ///
    /// # Errors
    /// `Forbidden` without a session; `Connection` on DB error.
    pub async fn set(
        &self,
        ctx: &AuthContext,
        point: GeoPoint,
        radius_m: i32,
    ) -> Result<HomeLocation, DbError> {
        if radius_m <= 0 {
            return Err(DbError::Conflict("radius must be positive".to_owned()));
        }
        let Some(user_id) = ctx.user_id() else {
            return Err(DbError::Forbidden);
        };
        sqlx::query(
            "INSERT INTO user_home_locations (user_id, location, radius_m) \
             VALUES ($1, ST_SetSRID(ST_MakePoint($2, $3), 4326)::geography, $4) \
             ON CONFLICT (user_id) DO UPDATE \
             SET location = EXCLUDED.location, radius_m = EXCLUDED.radius_m",
        )
        .bind(user_id.as_uuid())
        .bind(point.lon)
        .bind(point.lat)
        .bind(radius_m)
        .execute(self.db.pool())
        .await?;
        Ok(HomeLocation { point, radius_m })
    }

    /// Removes home; no geofence until it is set again.
    ///
    /// # Errors
    /// `Forbidden` without a session; `Connection` on DB error.
    pub async fn delete(&self, ctx: &AuthContext) -> Result<(), DbError> {
        let Some(user_id) = ctx.user_id() else {
            return Err(DbError::Forbidden);
        };
        sqlx::query("DELETE FROM user_home_locations WHERE user_id = $1")
            .bind(user_id.as_uuid())
            .execute(self.db.pool())
            .await?;
        Ok(())
    }

    /// Effective location and redaction flag for public links. The
    /// geofence uses the **library owner's** home, not the viewer's.
    ///
    /// Does not take an `AuthContext`: this only serves share
    /// serialization already authorized by the token.
    ///
    /// # Errors
    /// `Connection` if the query fails.
    pub async fn public_locations_for_assets(
        &self,
        asset_ids: &[AssetId],
    ) -> Result<Vec<PublicAssetLocation>, DbError> {
        if asset_ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids: Vec<uuid::Uuid> = asset_ids.iter().map(AssetId::as_uuid).collect();
        let rows: Vec<LocationRow> = sqlx::query_as(
            "SELECT a.id, \
                    ST_Y(COALESCE(o.location, a.location)::geometry) AS lat, \
                    ST_X(COALESCE(o.location, a.location)::geometry) AS lon, \
                    COALESCE(o.place_id, a.place_id) AS place_id, \
                    (h.user_id IS NOT NULL \
                     AND COALESCE(o.location, a.location) IS NOT NULL \
                     AND ST_DWithin( \
                           COALESCE(o.location, a.location), \
                           h.location, \
                           h.radius_m)) AS redact \
               FROM assets a \
               JOIN folders f ON f.id = a.folder_id \
               JOIN libraries l ON l.id = f.library_id \
               LEFT JOIN asset_overrides o ON o.asset_id = a.id \
               LEFT JOIN user_home_locations h ON h.user_id = l.owner_id \
              WHERE a.id = ANY($1)",
        )
        .bind(&ids)
        .fetch_all(self.db.pool())
        .await?;
        rows.into_iter()
            .map(PublicAssetLocation::try_from)
            .collect()
    }
}

#[derive(sqlx::FromRow)]
struct LocationRow {
    id: uuid::Uuid,
    lat: Option<f64>,
    lon: Option<f64>,
    place_id: Option<i64>,
    redact: bool,
}

impl TryFrom<LocationRow> for PublicAssetLocation {
    type Error = DbError;

    fn try_from(row: LocationRow) -> Result<Self, DbError> {
        let location = match (row.lat, row.lon) {
            (Some(lat), Some(lon)) => Some(GeoPoint { lat, lon }),
            (None, None) => None,
            _ => {
                return Err(DbError::Corrupted("asset location coordinates".to_owned()));
            }
        };
        Ok(Self {
            asset_id: AssetId::from_uuid(row.id),
            location,
            place_id: row.place_id,
            redact: row.redact,
        })
    }
}
