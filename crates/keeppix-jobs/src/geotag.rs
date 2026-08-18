//! Matching temporale GPX: orchestra lettura DB, interpolatore puro dei media
//! e writer batch, senza SQL in questo crate.

use chrono::Duration;
use keeppix_db::{Db, DbError, OverrideRepo};
use keeppix_domain::{AssetId, AuthContext, BatchId, LocationSource};
use keeppix_media::gpx::{GpxError, interpolate, parse};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GeotagError {
    #[error(transparent)]
    Gpx(#[from] GpxError),
    #[error(transparent)]
    Db(#[from] DbError),
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
