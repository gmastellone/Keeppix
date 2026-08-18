//! Matching temporale GPX: orchestra lettura DB, interpolatore puro dei media
//! e writer batch, senza SQL in questo crate.

use chrono::{DateTime, Duration, Utc};
use keeppix_db::{Db, DbError, GeoRepo, OverrideRepo};
use keeppix_domain::{AssetId, AuthContext, BatchId, LibraryId, LocationSource};
use keeppix_media::gpx::{GpxError, interpolate, parse};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GeotagError {
    #[error(transparent)]
    Gpx(#[from] GpxError),
    #[error(transparent)]
    Db(#[from] DbError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimezoneExample {
    pub asset_id: AssetId,
    pub filename: String,
    pub before: DateTime<Utc>,
    pub after: DateTime<Utc>,
    pub timezone: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimezonePreview {
    pub count: usize,
    pub example: Option<TimezoneExample>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimezoneApply {
    pub changed_count: usize,
    pub batch_id: Option<BatchId>,
}

pub struct RecalculateTimezones<'a> {
    db: &'a Db,
}

impl<'a> RecalculateTimezones<'a> {
    #[must_use]
    pub const fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// Calcola conteggio ed esempio senza scrivere override o batch.
    ///
    /// # Errors
    /// Libreria non accessibile o errore database.
    pub async fn preview(
        &self,
        ctx: &AuthContext,
        library_id: LibraryId,
    ) -> Result<TimezonePreview, GeotagError> {
        let changes = GeoRepo::new(self.db)
            .timezone_changes(ctx, library_id)
            .await?;
        let example = changes.first().map(|change| TimezoneExample {
            asset_id: change.asset_id,
            filename: change.filename.clone(),
            before: change.before,
            after: change.after,
            timezone: change.timezone.clone(),
        });
        Ok(TimezonePreview {
            count: changes.len(),
            example,
        })
    }

    /// Ricalcola al momento della conferma e applica un solo batch annullabile.
    ///
    /// # Errors
    /// Libreria non accessibile, asset non modificabile o errore database.
    pub async fn apply(
        &self,
        ctx: &AuthContext,
        library_id: LibraryId,
    ) -> Result<TimezoneApply, GeotagError> {
        let changes = GeoRepo::new(self.db)
            .timezone_changes(ctx, library_id)
            .await?;
        let assignments: Vec<_> = changes
            .iter()
            .map(|change| (change.asset_id, change.after))
            .collect();
        let batch_id = OverrideRepo::new(self.db)
            .apply_taken_at_batch(ctx, &assignments)
            .await?;
        Ok(TimezoneApply {
            changed_count: assignments.len(),
            batch_id,
        })
    }
}

/// Abbina gli asset alla traccia e applica tutte le coordinate prodotte in
/// un'unica transazione annullabile.
///
/// # Errors
/// GPX malformato, asset non visibile/modificabile o errore database.
pub async fn import_gpx(
    db: &Db,
    ctx: &AuthContext,
    asset_ids: &[AssetId],
    bytes: &[u8],
    tolerance: Duration,
) -> Result<BatchId, GeotagError> {
    let track = parse(bytes)?;
    let repo = OverrideRepo::new(db);
    let times = repo.effective_taken_at(ctx, asset_ids).await?;
    let assignments = times
        .into_iter()
        .filter_map(|(asset_id, taken_at)| {
            interpolate(&track, taken_at, tolerance).map(|point| (asset_id, point))
        })
        .collect::<Vec<_>>();
    Ok(repo
        .apply_geotag_points(ctx, &assignments, LocationSource::Gpx)
        .await?)
}
