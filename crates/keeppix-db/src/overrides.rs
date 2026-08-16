use std::collections::{BTreeMap, HashMap};

use chrono::{DateTime, Utc};
use keeppix_domain::{
    AssetId, AuthContext, BatchId, EffectiveMetadata, GeoPoint, OverridePatch, UserId,
};
use serde::{Deserialize, Serialize};
use sqlx::PgConnection;

use crate::visibility::VisibilityScope;
use crate::{AssetRepo, Db, DbError};

pub struct OverrideRepo<'a> {
    db: &'a Db,
}

const COLUMNS: &str = "asset_id, title, description, taken_at, \
                       ST_X(location::geometry) AS lon, ST_Y(location::geometry) AS lat, \
                       place_id, orientation, updated_by";

#[derive(sqlx::FromRow)]
struct OverrideRow {
    asset_id: uuid::Uuid,
    title: Option<String>,
    description: Option<String>,
    taken_at: Option<DateTime<Utc>>,
    lon: Option<f64>,
    lat: Option<f64>,
    place_id: Option<i64>,
    orientation: Option<i16>,
    updated_by: Option<uuid::Uuid>,
}

#[derive(sqlx::FromRow)]
struct EffectiveRow {
    title: Option<String>,
    description: Option<String>,
    taken_at: Option<DateTime<Utc>>,
    lon: Option<f64>,
    lat: Option<f64>,
    place_id: Option<i64>,
    orientation: Option<i16>,
}

impl EffectiveRow {
    fn into_domain(self) -> EffectiveMetadata {
        EffectiveMetadata {
            title: self.title,
            description: self.description,
            taken_at: self.taken_at,
            location: point(self.lon, self.lat),
            place_id: self.place_id,
            orientation: self.orientation,
        }
    }
}

fn point(lon: Option<f64>, lat: Option<f64>) -> Option<GeoPoint> {
    match (lon, lat) {
        (Some(lon), Some(lat)) => Some(GeoPoint { lat, lon }),
        _ => None,
    }
}

fn wkt(point: Option<GeoPoint>) -> Option<String> {
    point.map(|p| format!("SRID=4326;POINT({} {})", p.lon, p.lat))
}

/// Uno dei sei campi di `asset_overrides`, così com'era prima di un batch —
/// `None` a livello di mappa (vedi [`PreviousBatch`]) significa "l'asset non
/// aveva ancora nessuna riga di override", non "i campi erano tutti NULL":
/// i due casi si comportano diversamente in `undo_batch` (DELETE contro
/// UPSERT), anche se producono lo stesso `effective()`.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredOverride {
    title: Option<String>,
    description: Option<String>,
    taken_at: Option<DateTime<Utc>>,
    lon: Option<f64>,
    lat: Option<f64>,
    place_id: Option<i64>,
    orientation: Option<i16>,
    updated_by: Option<uuid::Uuid>,
}

/// Chiave: `asset_id` come stringa (i campi jsonb non possono avere chiavi
/// non testuali). Valore: `None` se l'asset non aveva override.
type PreviousBatch = BTreeMap<String, Option<StoredOverride>>;

#[allow(clippy::option_option)]
fn touched<T>(field: Option<Option<T>>) -> (bool, Option<T>) {
    match field {
        Some(inner) => (true, inner),
        None => (false, None),
    }
}

impl<'a> OverrideRepo<'a> {
    #[must_use]
    pub const fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// `COALESCE(override, exif)` campo per campo (spec §3.2). Un override
    /// parziale non azzera i campi non toccati.
    ///
    /// # Errors
    /// `Forbidden` se il chiamante non vede l'asset — anche quando l'id non
    /// esiste. `NotFound` solo a un admin che chiede un id inesistente.
    pub async fn effective(
        &self,
        ctx: &AuthContext,
        asset_id: AssetId,
    ) -> Result<EffectiveMetadata, DbError> {
        let scope = VisibilityScope::resolve(self.db, ctx).await?;
        let filter = scope.filter("f.library_id", 2);
        let row: Option<EffectiveRow> = sqlx::query_as(&format!(
            "SELECT o.title, o.description, \
                    COALESCE(o.taken_at, a.taken_at_utc) AS taken_at, \
                    ST_X(COALESCE(o.location, a.location)::geometry) AS lon, \
                    ST_Y(COALESCE(o.location, a.location)::geometry) AS lat, \
                    COALESCE(o.place_id, a.place_id) AS place_id, \
                    o.orientation \
             FROM assets a \
             JOIN folders f ON f.id = a.folder_id \
             LEFT JOIN asset_overrides o ON o.asset_id = a.id \
             WHERE a.id = $1 AND {}",
            filter.sql()
        ))
        .bind(asset_id.as_uuid())
        .bind(filter.bind())
        .fetch_optional(self.db.pool())
        .await?;

        match row {
            Some(row) => Ok(row.into_domain()),
            None if ctx.is_admin() => Err(DbError::NotFound),
            None => Err(DbError::Forbidden),
        }
    }

    /// Applica una modifica a un solo asset, senza registrarla per
    /// l'annullamento — quello lo fa solo [`Self::apply_batch`].
    ///
    /// # Errors
    /// `Forbidden` se il chiamante non vede l'asset.
    pub async fn apply(
        &self,
        ctx: &AuthContext,
        asset_id: AssetId,
        patch: &OverridePatch,
    ) -> Result<(), DbError> {
        AssetRepo::new(self.db)
            .assert_visible(ctx, std::slice::from_ref(&asset_id))
            .await?;
        let mut conn = self.db.pool().acquire().await?;
        apply_patch(
            &mut conn,
            &[asset_id.as_uuid()],
            patch,
            ctx.user_id().map(|id| id.as_uuid()),
        )
        .await
    }

    /// Applica la **stessa** modifica a molti asset in un'unica transazione
    /// — non 500 round-trip — e registra i valori precedenti per
    /// [`Self::undo_batch`].
    ///
    /// # Errors
    /// `Forbidden` se anche un solo asset non è visibile al chiamante.
    pub async fn apply_batch(
        &self,
        ctx: &AuthContext,
        asset_ids: &[AssetId],
        patch: &OverridePatch,
    ) -> Result<BatchId, DbError> {
        AssetRepo::new(self.db)
            .assert_visible(ctx, asset_ids)
            .await?;
        let Some(actor) = ctx.user_id() else {
            return Err(DbError::Forbidden);
        };

        let ids: Vec<uuid::Uuid> = asset_ids.iter().map(AssetId::as_uuid).collect();
        let mut tx = self.db.pool().begin().await?;

        let previous = load_previous(&mut tx, &ids).await?;
        let batch_id = BatchId::new();
        sqlx::query("INSERT INTO metadata_batches (id, actor_id, previous) VALUES ($1, $2, $3)")
            .bind(batch_id.as_uuid())
            .bind(actor.as_uuid())
            .bind(
                serde_json::to_value(&previous)
                    .map_err(|e| DbError::Corrupted(format!("previous batch state: {e}")))?,
            )
            .execute(&mut *tx)
            .await?;

        apply_patch(&mut tx, &ids, patch, Some(actor.as_uuid())).await?;

        tx.commit().await?;
        Ok(batch_id)
    }

    /// Ripristina esattamente i valori precedenti di un batch — anche
    /// quando il valore precedente era `NULL`, e anche quando l'asset non
    /// aveva ancora nessun override (in quel caso la riga viene cancellata,
    /// non lasciata con campi tutti `NULL`).
    ///
    /// Idempotente: annullare un batch già annullato non fa nulla.
    ///
    /// # Errors
    /// `Forbidden` se il batch non è del chiamante — anche quando l'id non
    /// esiste. `NotFound` solo a un admin che chiede un id inesistente.
    pub async fn undo_batch(&self, ctx: &AuthContext, batch_id: BatchId) -> Result<(), DbError> {
        let mut tx = self.db.pool().begin().await?;

        let row: Option<BatchRow> = sqlx::query_as(
            "SELECT actor_id, undone_at, previous FROM metadata_batches WHERE id = $1 FOR UPDATE",
        )
        .bind(batch_id.as_uuid())
        .fetch_optional(&mut *tx)
        .await?;

        let Some(row) = row else {
            return Err(if ctx.is_admin() {
                DbError::NotFound
            } else {
                DbError::Forbidden
            });
        };

        let owner = UserId::from_uuid(row.actor_id);
        if !ctx.is_admin() && Some(owner) != ctx.user_id() {
            return Err(DbError::Forbidden);
        }

        if row.undone_at.is_some() {
            // Già annullato: nessun lavoro da fare, non un errore.
            tx.commit().await?;
            return Ok(());
        }

        let previous: PreviousBatch = serde_json::from_value(row.previous)
            .map_err(|e| crate::row::corrupted("metadata_batches.previous", e))?;
        restore_previous(&mut tx, &previous).await?;

        sqlx::query("UPDATE metadata_batches SET undone_at = now() WHERE id = $1")
            .bind(batch_id.as_uuid())
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;
        Ok(())
    }

    /// Asset con override non ancora scritti su file:
    /// `updated_at > COALESCE(xmp_written_at, '-infinity')`.
    ///
    /// Non prende un `AuthContext`: la chiama il job `WriteSidecar` (Task 5),
    /// che attraversa tutte le librerie in background — come
    /// `LibraryRepo::mark_scanned`.
    ///
    /// # Errors
    /// `Connection` se la query fallisce.
    pub async fn pending_sidecars(&self, limit: i64) -> Result<Vec<AssetId>, DbError> {
        let rows: Vec<uuid::Uuid> = sqlx::query_scalar(
            "SELECT asset_id FROM asset_overrides \
              WHERE updated_at > COALESCE(xmp_written_at, '-infinity') \
              ORDER BY updated_at \
              LIMIT $1",
        )
        .bind(limit)
        .fetch_all(self.db.pool())
        .await?;
        Ok(rows.into_iter().map(AssetId::from_uuid).collect())
    }
}

#[derive(sqlx::FromRow)]
struct BatchRow {
    actor_id: uuid::Uuid,
    undone_at: Option<DateTime<Utc>>,
    previous: serde_json::Value,
}

/// Upsert di `patch` su `asset_ids`, preservando i campi non toccati di
/// ciascuna riga esistente. Un solo statement: `CASE WHEN <touched> THEN
/// <nuovo valore> ELSE <colonna esistente> END` per campo, così lo stesso
/// giro serve tanto un singolo asset (`apply`) quanto 500 (`apply_batch`).
async fn apply_patch(
    conn: &mut PgConnection,
    asset_ids: &[uuid::Uuid],
    patch: &OverridePatch,
    updated_by: Option<uuid::Uuid>,
) -> Result<(), DbError> {
    if asset_ids.is_empty() {
        return Ok(());
    }
    let (title_touched, title) = touched(patch.title.clone());
    let (description_touched, description) = touched(patch.description.clone());
    let (taken_at_touched, taken_at) = touched(patch.taken_at);
    let (location_touched, location) = touched(patch.location);
    let location_wkt = wkt(location);
    let (place_id_touched, place_id) = touched(patch.place_id);
    let (orientation_touched, orientation) = touched(patch.orientation);

    sqlx::query(
        "INSERT INTO asset_overrides \
            (asset_id, title, description, taken_at, location, place_id, orientation, \
             updated_by, updated_at) \
         SELECT aid, \
                CASE WHEN $2 THEN $3 ELSE NULL END, \
                CASE WHEN $4 THEN $5 ELSE NULL END, \
                CASE WHEN $6 THEN $7 ELSE NULL END, \
                CASE WHEN $8 THEN $9::geography ELSE NULL END, \
                CASE WHEN $10 THEN $11 ELSE NULL END, \
                CASE WHEN $12 THEN $13 ELSE NULL END, \
                $14, now() \
           FROM unnest($1::uuid[]) AS aid \
         ON CONFLICT (asset_id) DO UPDATE SET \
                title = CASE WHEN $2 THEN EXCLUDED.title ELSE asset_overrides.title END, \
                description = CASE WHEN $4 THEN EXCLUDED.description \
                                    ELSE asset_overrides.description END, \
                taken_at = CASE WHEN $6 THEN EXCLUDED.taken_at ELSE asset_overrides.taken_at END, \
                location = CASE WHEN $8 THEN EXCLUDED.location ELSE asset_overrides.location END, \
                place_id = CASE WHEN $10 THEN EXCLUDED.place_id ELSE asset_overrides.place_id END, \
                orientation = CASE WHEN $12 THEN EXCLUDED.orientation \
                                   ELSE asset_overrides.orientation END, \
                updated_by = $14, \
                updated_at = now()",
    )
    .bind(asset_ids)
    .bind(title_touched)
    .bind(title)
    .bind(description_touched)
    .bind(description)
    .bind(taken_at_touched)
    .bind(taken_at)
    .bind(location_touched)
    .bind(location_wkt)
    .bind(place_id_touched)
    .bind(place_id)
    .bind(orientation_touched)
    .bind(orientation)
    .bind(updated_by)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Legge lo stato attuale di `asset_ids` prima di sovrascriverlo, per
/// popolare `metadata_batches.previous`.
async fn load_previous(
    conn: &mut PgConnection,
    asset_ids: &[uuid::Uuid],
) -> Result<PreviousBatch, DbError> {
    let rows: Vec<OverrideRow> = sqlx::query_as(&format!(
        "SELECT {COLUMNS} FROM asset_overrides WHERE asset_id = ANY($1)"
    ))
    .bind(asset_ids)
    .fetch_all(&mut *conn)
    .await?;

    let mut existing: HashMap<uuid::Uuid, StoredOverride> = HashMap::new();
    for row in rows {
        existing.insert(
            row.asset_id,
            StoredOverride {
                title: row.title,
                description: row.description,
                taken_at: row.taken_at,
                lon: row.lon,
                lat: row.lat,
                place_id: row.place_id,
                orientation: row.orientation,
                updated_by: row.updated_by,
            },
        );
    }

    Ok(asset_ids
        .iter()
        .map(|id| (id.to_string(), existing.get(id).cloned()))
        .collect())
}

/// Contrario di [`apply_patch`]: per gli asset senza override precedente,
/// cancella la riga; per gli altri, riscrive esattamente i valori
/// catturati — compresi quelli `NULL`, che un `COALESCE` a valle
/// confonderebbe con "non toccare" se si saltasse la riscrittura.
async fn restore_previous(
    conn: &mut PgConnection,
    previous: &PreviousBatch,
) -> Result<(), DbError> {
    let mut delete_ids = Vec::new();
    let mut restore_ids = Vec::new();
    let mut titles = Vec::new();
    let mut descriptions = Vec::new();
    let mut taken_ats = Vec::new();
    let mut locations = Vec::new();
    let mut place_ids = Vec::new();
    let mut orientations = Vec::new();
    let mut updated_bys = Vec::new();

    for (key, value) in previous {
        let id = uuid::Uuid::parse_str(key)
            .map_err(|e| crate::row::corrupted("metadata_batches.previous key", e))?;
        match value {
            None => delete_ids.push(id),
            Some(stored) => {
                restore_ids.push(id);
                titles.push(stored.title.clone());
                descriptions.push(stored.description.clone());
                taken_ats.push(stored.taken_at);
                locations.push(wkt(point(stored.lon, stored.lat)));
                place_ids.push(stored.place_id);
                orientations.push(stored.orientation);
                updated_bys.push(stored.updated_by);
            }
        }
    }

    if !delete_ids.is_empty() {
        sqlx::query("DELETE FROM asset_overrides WHERE asset_id = ANY($1)")
            .bind(&delete_ids)
            .execute(&mut *conn)
            .await?;
    }

    if !restore_ids.is_empty() {
        sqlx::query(
            "INSERT INTO asset_overrides \
                (asset_id, title, description, taken_at, location, place_id, orientation, \
                 updated_by, updated_at) \
             SELECT aid, title, description, taken_at, loc::geography, place_id, orientation, \
                    updated_by, now() \
               FROM unnest($1::uuid[], $2::text[], $3::text[], $4::timestamptz[], $5::text[], \
                           $6::bigint[], $7::smallint[], $8::uuid[]) \
                 AS t(aid, title, description, taken_at, loc, place_id, orientation, updated_by) \
             ON CONFLICT (asset_id) DO UPDATE SET \
                title = EXCLUDED.title, \
                description = EXCLUDED.description, \
                taken_at = EXCLUDED.taken_at, \
                location = EXCLUDED.location, \
                place_id = EXCLUDED.place_id, \
                orientation = EXCLUDED.orientation, \
                updated_by = EXCLUDED.updated_by, \
                updated_at = now()",
        )
        .bind(&restore_ids)
        .bind(&titles)
        .bind(&descriptions)
        .bind(&taken_ats)
        .bind(&locations)
        .bind(&place_ids)
        .bind(&orientations)
        .bind(&updated_bys)
        .execute(&mut *conn)
        .await?;
    }

    Ok(())
}
