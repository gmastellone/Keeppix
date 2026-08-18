use std::collections::{BTreeMap, HashMap};

use chrono::{DateTime, Utc};
use keeppix_domain::{
    AssetId, AuthContext, BatchId, EffectiveMetadata, GeoPoint, JobKind, JobPriority,
    LocationSource, OverridePatch, Pick, Rating, UserId,
};
use serde::{Deserialize, Serialize};
use sqlx::PgConnection;

use crate::visibility::VisibilityScope;
use crate::{AssetRepo, Db, DbError, JobRepo};

pub struct OverrideRepo<'a> {
    db: &'a Db,
}

#[derive(sqlx::FromRow)]
struct OverrideRow {
    asset_id: uuid::Uuid,
    had_override: bool,
    title: Option<String>,
    description: Option<String>,
    taken_at: Option<DateTime<Utc>>,
    lon: Option<f64>,
    lat: Option<f64>,
    place_id: Option<i64>,
    orientation: Option<i16>,
    updated_by: Option<uuid::Uuid>,
    location_source: Option<String>,
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

/// Stato di `asset_overrides` e di `assets.location_source` prima di un batch.
/// `had_override` distingue una riga con tutti i campi `NULL` dall'assenza
/// della riga: in `undo_batch` diventano rispettivamente UPSERT e DELETE.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredOverride {
    /// `false` means that the asset had no `asset_overrides` row. Older batch
    /// payloads predate this field and only serialized `Some` for existing
    /// rows, so their correct default is `true`.
    #[serde(default = "default_true")]
    had_override: bool,
    title: Option<String>,
    description: Option<String>,
    taken_at: Option<DateTime<Utc>>,
    lon: Option<f64>,
    lat: Option<f64>,
    place_id: Option<i64>,
    orientation: Option<i16>,
    updated_by: Option<uuid::Uuid>,
    /// Compatibility flag for batches written before Task 4. `None` is a
    /// valid previous source, so presence cannot be represented by the source
    /// field alone.
    #[serde(default)]
    location_source_captured: bool,
    #[serde(default)]
    location_source: Option<String>,
}

/// Chiave: `asset_id` come stringa (i campi jsonb non possono avere chiavi
/// non testuali). I vecchi payload usano `None` per un asset senza override;
/// i nuovi usano `StoredOverride::had_override` per poter salvare anche la
/// precedente `location_source`.
type PreviousBatch = BTreeMap<String, Option<StoredOverride>>;

const fn default_true() -> bool {
    true
}

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
        let filter = scope.filter("f.path", "f.library_id", "a.id", 2);
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
        .bind(filter.holes())
        .bind(filter.assets())
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
    /// `Forbidden` se il chiamante non vede l'asset, o lo vede solo come
    /// viewer (spec §1.2: modificare i metadati è editor+).
    pub async fn apply(
        &self,
        ctx: &AuthContext,
        asset_id: AssetId,
        patch: &OverridePatch,
    ) -> Result<(), DbError> {
        AssetRepo::new(self.db)
            .assert_visible(ctx, std::slice::from_ref(&asset_id))
            .await?;
        crate::PermissionRepo::new(self.db)
            .assert_can_edit_assets(ctx, std::slice::from_ref(&asset_id))
            .await?;
        let mut conn = self.db.pool().acquire().await?;
        apply_patch(
            &mut conn,
            &[asset_id.as_uuid()],
            patch,
            ctx.user_id().map(|id| id.as_uuid()),
        )
        .await?;
        enqueue_sidecar_sweep(self.db).await
    }

    /// Applica la **stessa** modifica a molti asset in un'unica transazione
    /// — non 500 round-trip — e registra i valori precedenti per
    /// [`Self::undo_batch`].
    ///
    /// # Errors
    /// `Forbidden` se anche un solo asset non è visibile al chiamante, o
    /// lo è solo come viewer.
    pub async fn apply_batch(
        &self,
        ctx: &AuthContext,
        asset_ids: &[AssetId],
        patch: &OverridePatch,
    ) -> Result<BatchId, DbError> {
        self.apply_batch_inner(ctx, asset_ids, patch, None).await
    }

    /// Applica una posizione uniforme e registra su `assets.location_source`
    /// chi l'ha assegnata. La sorgente fa parte della stessa transazione e
    /// dello stesso snapshot di annullamento degli override.
    ///
    /// # Errors
    /// Come [`Self::apply_batch`].
    pub async fn apply_location_batch(
        &self,
        ctx: &AuthContext,
        asset_ids: &[AssetId],
        patch: &OverridePatch,
        source: LocationSource,
    ) -> Result<BatchId, DbError> {
        self.apply_batch_inner(ctx, asset_ids, patch, Some(source))
            .await
    }

    async fn apply_batch_inner(
        &self,
        ctx: &AuthContext,
        asset_ids: &[AssetId],
        patch: &OverridePatch,
        source: Option<LocationSource>,
    ) -> Result<BatchId, DbError> {
        AssetRepo::new(self.db)
            .assert_visible(ctx, asset_ids)
            .await?;
        crate::PermissionRepo::new(self.db)
            .assert_can_edit_assets(ctx, asset_ids)
            .await?;
        let Some(actor) = ctx.user_id() else {
            return Err(DbError::Forbidden);
        };

        let ids: Vec<uuid::Uuid> = asset_ids.iter().map(AssetId::as_uuid).collect();
        let mut tx = self.db.pool().begin().await?;

        let captures_location_source =
            source.is_some() || patch.location.is_some() || patch.place_id.is_some();
        let previous = load_previous(&mut tx, &ids, captures_location_source).await?;
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
        if let Some(source) = source {
            apply_location_source(&mut tx, &ids, source).await?;
        }

        tx.commit().await?;
        enqueue_sidecar_sweep(self.db).await?;
        Ok(batch_id)
    }

    /// Legge in un solo giro il timestamp effettivo degli asset da abbinare a
    /// una traccia GPX. Gli asset senza data vengono omessi dal risultato.
    ///
    /// # Errors
    /// `Forbidden` se anche un solo asset non è visibile o modificabile.
    pub async fn effective_taken_at(
        &self,
        ctx: &AuthContext,
        asset_ids: &[AssetId],
    ) -> Result<Vec<(AssetId, DateTime<Utc>)>, DbError> {
        AssetRepo::new(self.db)
            .assert_visible(ctx, asset_ids)
            .await?;
        crate::PermissionRepo::new(self.db)
            .assert_can_edit_assets(ctx, asset_ids)
            .await?;
        if asset_ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids: Vec<uuid::Uuid> = asset_ids.iter().map(AssetId::as_uuid).collect();
        let rows: Vec<(uuid::Uuid, DateTime<Utc>)> = sqlx::query_as(
            "SELECT a.id, COALESCE(o.taken_at, a.taken_at_utc) \
               FROM assets a \
               LEFT JOIN asset_overrides o ON o.asset_id = a.id \
              WHERE a.id = ANY($1) \
                AND COALESCE(o.taken_at, a.taken_at_utc) IS NOT NULL",
        )
        .bind(&ids)
        .fetch_all(self.db.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|(id, taken_at)| (AssetId::from_uuid(id), taken_at))
            .collect())
    }

    /// Applica coordinate diverse per asset con un solo `UNNEST`, registrando
    /// un unico batch annullabile. È il writer parametrico usato dal matching
    /// GPX; non esegue un `apply()` per fotografia.
    ///
    /// # Errors
    /// `Forbidden` se anche un solo asset non è visibile o modificabile.
    pub async fn apply_geotag_points(
        &self,
        ctx: &AuthContext,
        assignments: &[(AssetId, GeoPoint)],
        source: LocationSource,
    ) -> Result<BatchId, DbError> {
        let asset_ids: Vec<AssetId> = assignments.iter().map(|(id, _)| *id).collect();
        AssetRepo::new(self.db)
            .assert_visible(ctx, &asset_ids)
            .await?;
        crate::PermissionRepo::new(self.db)
            .assert_can_edit_assets(ctx, &asset_ids)
            .await?;
        let Some(actor) = ctx.user_id() else {
            return Err(DbError::Forbidden);
        };

        let ids: Vec<uuid::Uuid> = assignments.iter().map(|(id, _)| id.as_uuid()).collect();
        let mut tx = self.db.pool().begin().await?;
        let previous = load_previous(&mut tx, &ids, true).await?;
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

        apply_points(&mut tx, assignments, actor.as_uuid()).await?;
        apply_location_source(&mut tx, &ids, source).await?;
        tx.commit().await?;
        enqueue_sidecar_sweep(self.db).await?;
        Ok(batch_id)
    }

    /// Applica timestamp diversi per asset in un unico batch annullabile.
    ///
    /// È il writer parametrico del ricalcolo dei fusi: conserva tutti gli
    /// altri campi dell'override e riusa `metadata_batches`/`undo_batch`.
    /// Un elenco vuoto è un no-op e non crea una riga batch vuota.
    ///
    /// # Errors
    /// `Forbidden` se anche un solo asset non è visibile o modificabile.
    pub async fn apply_taken_at_batch(
        &self,
        ctx: &AuthContext,
        assignments: &[(AssetId, DateTime<Utc>)],
    ) -> Result<Option<BatchId>, DbError> {
        if assignments.is_empty() {
            return Ok(None);
        }
        let asset_ids: Vec<AssetId> = assignments.iter().map(|(id, _)| *id).collect();
        AssetRepo::new(self.db)
            .assert_visible(ctx, &asset_ids)
            .await?;
        crate::PermissionRepo::new(self.db)
            .assert_can_edit_assets(ctx, &asset_ids)
            .await?;
        let Some(actor) = ctx.user_id() else {
            return Err(DbError::Forbidden);
        };

        let ids: Vec<uuid::Uuid> = assignments.iter().map(|(id, _)| id.as_uuid()).collect();
        let mut tx = self.db.pool().begin().await?;
        let previous = load_previous(&mut tx, &ids, false).await?;
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

        apply_taken_at_values(&mut tx, assignments, actor.as_uuid()).await?;
        tx.commit().await?;
        enqueue_sidecar_sweep(self.db).await?;
        Ok(Some(batch_id))
    }

    /// Ripristina esattamente i valori precedenti di un batch — anche
    /// quando il valore precedente era `NULL`, e anche quando l'asset non
    /// aveva ancora nessun override (in quel caso la riga viene cancellata,
    /// non lasciata con campi tutti `NULL`).
    ///
    /// Idempotente: annullare un batch già annullato non fa nulla.
    ///
    /// **Finestra di annullamento** (spec §8: «annulla ripristina i valori
    /// precedenti finché il sidecar non è stato scritto»): se il sidecar di
    /// **anche un solo** asset del batch è già stato scritto con i valori
    /// di questo batch (`xmp_written_at >= metadata_batches.applied_at`),
    /// l'annullamento è rifiutato con `Conflict` invece di riportare
    /// indietro il database lasciando il file — già consegnato, magari
    /// esportato altrove — con un valore che il database non ricorda più
    /// come "attuale". Prima di quel momento l'annullamento è sempre
    /// permesso: il file non ha ancora visto il valore sbagliato.
    ///
    /// # Errors
    /// `Forbidden` se il batch non è del chiamante — anche quando l'id non
    /// esiste. `NotFound` solo a un admin che chiede un id inesistente.
    /// `Conflict` se il sidecar è già stato scritto per questo batch.
    pub async fn undo_batch(&self, ctx: &AuthContext, batch_id: BatchId) -> Result<(), DbError> {
        let mut tx = self.db.pool().begin().await?;

        let row: Option<BatchRow> = sqlx::query_as(
            "SELECT actor_id, applied_at, undone_at, previous FROM metadata_batches \
              WHERE id = $1 FOR UPDATE",
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
        let asset_ids = previous_asset_ids(&previous)?;

        let already_synced: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM asset_overrides \
               WHERE asset_id = ANY($1) AND xmp_written_at IS NOT NULL \
                 AND xmp_written_at >= $2)",
        )
        .bind(&asset_ids)
        .bind(row.applied_at)
        .fetch_one(&mut *tx)
        .await?;
        if already_synced {
            return Err(DbError::Conflict(
                "the sidecar has already been written with this batch's values; \
                 undo is only available before that"
                    .to_owned(),
            ));
        }

        restore_previous(&mut tx, &previous).await?;

        sqlx::query("UPDATE metadata_batches SET undone_at = now() WHERE id = $1")
            .bind(batch_id.as_uuid())
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;
        Ok(())
    }

    /// **Scostamento di N ore** sulla data di scatto (spec §8): il rimedio
    /// per l'orologio della fotocamera sbagliato dopo un viaggio, offerto
    /// come operazione a sé — non calcolato dal client sottraendo due date
    /// assolute — perché ogni asset del batch può avere un `taken_at` di
    /// partenza diverso. Un solo statement calcola
    /// `COALESCE(override, exif) + N ore` per riga, così vale sia per un
    /// singolo asset sia per 5.000.
    ///
    /// Registra un batch di annullamento esattamente come
    /// [`Self::apply_batch`]: la stessa [`Self::undo_batch`] lo ripristina.
    ///
    /// # Errors
    /// `Forbidden` se anche un solo asset non è visibile al chiamante, o
    /// lo è solo come viewer.
    pub async fn shift_taken_at(
        &self,
        ctx: &AuthContext,
        asset_ids: &[AssetId],
        hours: i32,
    ) -> Result<BatchId, DbError> {
        AssetRepo::new(self.db)
            .assert_visible(ctx, asset_ids)
            .await?;
        crate::PermissionRepo::new(self.db)
            .assert_can_edit_assets(ctx, asset_ids)
            .await?;
        let Some(actor) = ctx.user_id() else {
            return Err(DbError::Forbidden);
        };

        let ids: Vec<uuid::Uuid> = asset_ids.iter().map(AssetId::as_uuid).collect();
        let mut tx = self.db.pool().begin().await?;

        let previous = load_previous(&mut tx, &ids, false).await?;
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

        apply_shift(&mut tx, &ids, hours, Some(actor.as_uuid())).await?;

        tx.commit().await?;
        enqueue_sidecar_sweep(self.db).await?;
        Ok(batch_id)
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

    /// Ciò che serve al job `WriteSidecar` per sincronizzare un asset: i
    /// valori effettivi (`COALESCE(override, exif)`) più il voto del
    /// **proprietario della libreria** — `xmp:Rating`/`xmp:Label` sono
    /// valori singoli, quindi solo il suo voto finisce sul file (spec
    /// §3.4, §4.1); gli altri restano solo in Keeppix.
    ///
    /// Non prende un `AuthContext`: la chiama il job in background su
    /// tutte le librerie, stessa giustificazione di
    /// [`Self::pending_sidecars`].
    ///
    /// # Errors
    /// `NotFound` se l'asset non esiste più — cancellato fra l'accodamento
    /// del job e la sua esecuzione, non un bug.
    pub async fn sidecar_source(&self, asset_id: AssetId) -> Result<SidecarSource, DbError> {
        let row: Option<SidecarRow> = sqlx::query_as(
            "SELECT o.title, o.description, \
                    COALESCE(o.taken_at, a.taken_at_utc) AS taken_at, \
                    ST_X(COALESCE(o.location, a.location)::geometry) AS lon, \
                    ST_Y(COALESCE(o.location, a.location)::geometry) AS lat, \
                    COALESCE(o.place_id, a.place_id) AS place_id, \
                    o.orientation, \
                    fl.rating AS owner_rating, fl.pick AS owner_pick \
             FROM assets a \
             JOIN folders f ON f.id = a.folder_id \
             JOIN libraries l ON l.id = f.library_id \
             LEFT JOIN asset_overrides o ON o.asset_id = a.id \
             LEFT JOIN asset_flags fl ON fl.asset_id = a.id AND fl.user_id = l.owner_id \
             WHERE a.id = $1",
        )
        .bind(asset_id.as_uuid())
        .fetch_optional(self.db.pool())
        .await?;

        row.ok_or(DbError::NotFound)?.into_domain()
    }

    /// Registra che il sidecar è stato scritto **e verificato** — mai
    /// prima: se il processo muore fra la scrittura e questa chiamata, il
    /// prossimo giro di [`Self::pending_sidecars`] lo ritenta, invece di
    /// dare per sincronizzato un file che potrebbe non esserlo.
    ///
    /// Non prende un `AuthContext`, stessa giustificazione di
    /// [`Self::sidecar_source`].
    ///
    /// # Errors
    /// `Connection` se l'aggiornamento fallisce.
    pub async fn mark_sidecar_written(&self, asset_id: AssetId) -> Result<(), DbError> {
        sqlx::query("UPDATE asset_overrides SET xmp_written_at = now() WHERE asset_id = $1")
            .bind(asset_id.as_uuid())
            .execute(self.db.pool())
            .await?;
        Ok(())
    }
}

/// Dati che il job `WriteSidecar` (keeppix-jobs) traduce in un
/// `keeppix_media::xmp::SidecarData`. Vive qui, non in keeppix-media,
/// perché porta tipi di dominio (`Rating`, `Pick`) che il crate dei media
/// non deve conoscere altrettanto quanto non deve conoscere il database.
#[derive(Debug, Clone, PartialEq)]
pub struct SidecarSource {
    pub effective: EffectiveMetadata,
    pub owner_rating: Option<Rating>,
    pub owner_pick: Pick,
}

#[derive(sqlx::FromRow)]
struct SidecarRow {
    title: Option<String>,
    description: Option<String>,
    taken_at: Option<DateTime<Utc>>,
    lon: Option<f64>,
    lat: Option<f64>,
    place_id: Option<i64>,
    orientation: Option<i16>,
    owner_rating: Option<i16>,
    owner_pick: Option<String>,
}

impl SidecarRow {
    fn into_domain(self) -> Result<SidecarSource, DbError> {
        let owner_rating = self
            .owner_rating
            .map(|raw| {
                u8::try_from(raw)
                    .map_err(|e| crate::row::corrupted("asset_flags.rating", e))
                    .and_then(|raw| {
                        Rating::parse(raw)
                            .map_err(|e| crate::row::corrupted("asset_flags.rating", e))
                    })
            })
            .transpose()?;
        let owner_pick = self
            .owner_pick
            .as_deref()
            .map(|raw| Pick::parse(raw).map_err(|e| crate::row::corrupted("asset_flags.pick", e)))
            .transpose()?
            .unwrap_or_default();

        Ok(SidecarSource {
            effective: EffectiveMetadata {
                title: self.title,
                description: self.description,
                taken_at: self.taken_at,
                location: point(self.lon, self.lat),
                place_id: self.place_id,
                orientation: self.orientation,
            },
            owner_rating,
            owner_pick,
        })
    }
}

/// Sveglia il job `WriteSidecar` dopo che un override ha reso qualche asset
/// "pendente" (spec §3.3: «DB prima, file poi»). Un'unica chiave di dedup
/// (`write_sidecar`) fa sì che 500 asset in un `apply_batch` producano un
/// solo job, non 500: il job stesso rilegge `pending_sidecars` e processa
/// tutto ciò che trova, ri-accodandosi da solo se ce n'è più di un batch.
///
/// # Errors
/// `Connection` se l'inserimento in coda fallisce.
async fn enqueue_sidecar_sweep(db: &Db) -> Result<(), DbError> {
    JobRepo::new(db)
        .enqueue(
            JobKind::WriteSidecar,
            serde_json::json!({}),
            JobPriority::Background,
            Some("write_sidecar"),
        )
        .await?;
    Ok(())
}

#[derive(sqlx::FromRow)]
struct BatchRow {
    actor_id: uuid::Uuid,
    applied_at: DateTime<Utc>,
    undone_at: Option<DateTime<Utc>>,
    previous: serde_json::Value,
}

/// Le chiavi di [`PreviousBatch`] sono `asset_id` come stringa (vincolo di
/// JSONB); qui si torna a `Uuid` per interrogare `asset_overrides`.
fn previous_asset_ids(previous: &PreviousBatch) -> Result<Vec<uuid::Uuid>, DbError> {
    previous
        .keys()
        .map(|key| {
            uuid::Uuid::parse_str(key)
                .map_err(|e| crate::row::corrupted("metadata_batches.previous key", e))
        })
        .collect()
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

async fn apply_points(
    conn: &mut PgConnection,
    assignments: &[(AssetId, GeoPoint)],
    updated_by: uuid::Uuid,
) -> Result<(), DbError> {
    if assignments.is_empty() {
        return Ok(());
    }
    let ids: Vec<uuid::Uuid> = assignments.iter().map(|(id, _)| id.as_uuid()).collect();
    let lons: Vec<f64> = assignments.iter().map(|(_, point)| point.lon).collect();
    let lats: Vec<f64> = assignments.iter().map(|(_, point)| point.lat).collect();
    sqlx::query(
        "INSERT INTO asset_overrides \
            (asset_id, location, place_id, updated_by, updated_at) \
         SELECT aid, ST_SetSRID(ST_MakePoint(lon, lat), 4326)::geography, NULL, $4, now() \
           FROM unnest($1::uuid[], $2::float8[], $3::float8[]) AS t(aid, lon, lat) \
         ON CONFLICT (asset_id) DO UPDATE SET \
            location = EXCLUDED.location, \
            place_id = NULL, \
            updated_by = EXCLUDED.updated_by, \
            updated_at = now()",
    )
    .bind(&ids)
    .bind(&lons)
    .bind(&lats)
    .bind(updated_by)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

async fn apply_taken_at_values(
    conn: &mut PgConnection,
    assignments: &[(AssetId, DateTime<Utc>)],
    updated_by: uuid::Uuid,
) -> Result<(), DbError> {
    let ids: Vec<uuid::Uuid> = assignments.iter().map(|(id, _)| id.as_uuid()).collect();
    let values: Vec<DateTime<Utc>> = assignments.iter().map(|(_, value)| *value).collect();
    sqlx::query(
        "INSERT INTO asset_overrides (asset_id, taken_at, updated_by, updated_at) \
         SELECT asset_id, taken_at, $3, now() \
           FROM unnest($1::uuid[], $2::timestamptz[]) AS input(asset_id, taken_at) \
         ON CONFLICT (asset_id) DO UPDATE SET \
            taken_at = EXCLUDED.taken_at, \
            updated_by = EXCLUDED.updated_by, \
            updated_at = now()",
    )
    .bind(&ids)
    .bind(&values)
    .bind(updated_by)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

async fn apply_location_source(
    conn: &mut PgConnection,
    asset_ids: &[uuid::Uuid],
    source: LocationSource,
) -> Result<(), DbError> {
    if asset_ids.is_empty() {
        return Ok(());
    }
    sqlx::query("UPDATE assets SET location_source = $2, updated_at = now() WHERE id = ANY($1)")
        .bind(asset_ids)
        .bind(source.as_str())
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// Applica lo scostamento di `hours` ore al `taken_at` effettivo
/// (`COALESCE(override, exif)`) di ciascun asset, in un solo statement.
/// Un asset senza alcuna data di scatto nota (né override né exif) resta
/// senza data: uno scostamento non può inventare un'origine.
async fn apply_shift(
    conn: &mut PgConnection,
    asset_ids: &[uuid::Uuid],
    hours: i32,
    updated_by: Option<uuid::Uuid>,
) -> Result<(), DbError> {
    if asset_ids.is_empty() {
        return Ok(());
    }
    sqlx::query(
        "INSERT INTO asset_overrides \
            (asset_id, title, description, taken_at, location, place_id, orientation, \
             updated_by, updated_at) \
         SELECT a.id, o.title, o.description, \
                COALESCE(o.taken_at, a.taken_at_utc) + make_interval(hours => $2), \
                o.location, o.place_id, o.orientation, \
                $3, now() \
           FROM assets a \
           LEFT JOIN asset_overrides o ON o.asset_id = a.id \
          WHERE a.id = ANY($1) \
         ON CONFLICT (asset_id) DO UPDATE SET \
                taken_at = EXCLUDED.taken_at, \
                updated_by = EXCLUDED.updated_by, \
                updated_at = now()",
    )
    .bind(asset_ids)
    .bind(hours)
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
    capture_location_source: bool,
) -> Result<PreviousBatch, DbError> {
    let rows: Vec<OverrideRow> = sqlx::query_as(
        "SELECT a.id AS asset_id, o.asset_id IS NOT NULL AS had_override, \
                o.title, o.description, o.taken_at, \
                ST_X(o.location::geometry) AS lon, ST_Y(o.location::geometry) AS lat, \
                o.place_id, o.orientation, o.updated_by, \
                CASE WHEN $2 THEN a.location_source ELSE NULL END AS location_source \
           FROM assets a \
           LEFT JOIN asset_overrides o ON o.asset_id = a.id \
          WHERE a.id = ANY($1)",
    )
    .bind(asset_ids)
    .bind(capture_location_source)
    .fetch_all(&mut *conn)
    .await?;

    let mut existing: HashMap<uuid::Uuid, StoredOverride> = HashMap::new();
    for row in rows {
        existing.insert(
            row.asset_id,
            StoredOverride {
                had_override: row.had_override,
                title: row.title,
                description: row.description,
                taken_at: row.taken_at,
                lon: row.lon,
                lat: row.lat,
                place_id: row.place_id,
                orientation: row.orientation,
                updated_by: row.updated_by,
                location_source_captured: capture_location_source,
                location_source: row.location_source,
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
    let mut source_ids = Vec::new();
    let mut location_sources = Vec::new();

    for (key, value) in previous {
        let id = uuid::Uuid::parse_str(key)
            .map_err(|e| crate::row::corrupted("metadata_batches.previous key", e))?;
        match value {
            None => delete_ids.push(id),
            Some(stored) => {
                if stored.had_override {
                    restore_ids.push(id);
                    titles.push(stored.title.clone());
                    descriptions.push(stored.description.clone());
                    taken_ats.push(stored.taken_at);
                    locations.push(wkt(point(stored.lon, stored.lat)));
                    place_ids.push(stored.place_id);
                    orientations.push(stored.orientation);
                    updated_bys.push(stored.updated_by);
                } else {
                    delete_ids.push(id);
                }
                if stored.location_source_captured {
                    source_ids.push(id);
                    location_sources.push(stored.location_source.clone());
                }
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

    if !source_ids.is_empty() {
        sqlx::query(
            "UPDATE assets AS a \
                SET location_source = previous.location_source, updated_at = now() \
               FROM unnest($1::uuid[], $2::text[]) AS previous(asset_id, location_source) \
              WHERE a.id = previous.asset_id",
        )
        .bind(&source_ids)
        .bind(&location_sources)
        .execute(&mut *conn)
        .await?;
    }

    Ok(())
}
