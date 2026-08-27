//! GPX temporal matching: orchestrates DB reads, the pure media
//! interpolator, and batch writes, with no SQL in this crate.

use chrono::{DateTime, Duration, Utc};
use keeppix_db::{Db, DbError, GeoRepo, OverrideRepo, PermissionRepo};
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

/// Outcome of a partially-successful GPX import.
#[derive(Debug)]
pub struct GpxImportOutcome {
    pub batch_id: Option<BatchId>,
    pub succeeded: Vec<AssetId>,
    pub failed: Vec<(AssetId, DbError)>,
}

pub struct RecalculateTimezones<'a> {
    db: &'a Db,
}

impl<'a> RecalculateTimezones<'a> {
    #[must_use]
    pub const fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// Computes the count and example without writing overrides or a batch.
    ///
    /// # Errors
    /// Library not accessible, or database error.
    pub async fn preview(
        &self,
        ctx: &AuthContext,
        library_id: LibraryId,
    ) -> Result<TimezonePreview, GeotagError> {
        let preview = GeoRepo::new(self.db)
            .timezone_change_preview(ctx, library_id)
            .await?;
        let example = preview.example.map(|change| TimezoneExample {
            asset_id: change.asset_id,
            filename: change.filename,
            before: change.before,
            after: change.after,
            timezone: change.timezone,
        });
        Ok(TimezonePreview {
            count: preview.count,
            example,
        })
    }

    /// Recomputes at confirmation time and applies a single undoable batch.
    ///
    /// # Errors
    /// Library not accessible, asset not editable, or database error.
    pub async fn apply(
        &self,
        ctx: &AuthContext,
        library_id: LibraryId,
    ) -> Result<(TimezoneApply, Vec<AssetId>), GeotagError> {
        let (changed_count, batch_id, succeeded) = GeoRepo::new(self.db)
            .apply_timezone_changes(ctx, library_id)
            .await?;
        Ok((
            TimezoneApply {
                changed_count,
                batch_id,
            },
            succeeded,
        ))
    }
}

/// Matches assets against the track and applies the produced coordinates.
/// Non-editable assets end up in `failed`; those with no temporal match are
/// neither succeeded nor failed (no write, no reason recorded).
///
/// # Errors
/// Malformed GPX, or a database error not attributable to a single id.
pub async fn import_gpx(
    db: &Db,
    ctx: &AuthContext,
    asset_ids: &[AssetId],
    bytes: &[u8],
    tolerance: Duration,
) -> Result<GpxImportOutcome, GeotagError> {
    let track = parse(bytes)?;
    let (editable, failed) = PermissionRepo::new(db)
        .partition_editable_assets(ctx, asset_ids)
        .await?;
    let repo = OverrideRepo::new(db);
    let times = if editable.is_empty() {
        Vec::new()
    } else {
        repo.effective_taken_at(ctx, &editable).await?
    };
    let assignments = times
        .into_iter()
        .filter_map(|(asset_id, taken_at)| {
            interpolate(&track, taken_at, tolerance).map(|point| (asset_id, point))
        })
        .collect::<Vec<_>>();
    let succeeded: Vec<AssetId> = assignments.iter().map(|(id, _)| *id).collect();
    let batch_id = if assignments.is_empty() {
        None
    } else {
        Some(
            repo.apply_geotag_points(ctx, &assignments, LocationSource::Gpx)
                .await?,
        )
    };
    Ok(GpxImportOutcome {
        batch_id,
        succeeded,
        failed,
    })
}
