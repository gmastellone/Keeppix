use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// WGS84 geographic point, in decimal degrees — the same system as
/// `assets.location` (`geography(Point, 4326)`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GeoPoint {
    pub lat: f64,
    pub lon: f64,
}

/// A patch to apply to `asset_overrides`. Each field distinguishes "don't
/// touch" from "clear": `None` = leave alone, `Some(None)` = clear,
/// `Some(Some(v))` = set. A plain `Option<T>` alone would conflate these two
/// intents — that's exactly the bug this distinction avoids when a previous
/// value was `NULL`. The three combinations are deliberate:
/// `clippy::option_option` is disabled here on purpose, not out of laziness.
#[allow(clippy::option_option)]
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OverridePatch {
    pub title: Option<Option<String>>,
    pub description: Option<Option<String>>,
    pub taken_at: Option<Option<DateTime<Utc>>>,
    pub location: Option<Option<GeoPoint>>,
    pub place_id: Option<Option<i64>>,
    pub orientation: Option<Option<i16>>,
}

impl OverridePatch {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.title.is_none()
            && self.description.is_none()
            && self.taken_at.is_none()
            && self.location.is_none()
            && self.place_id.is_none()
            && self.orientation.is_none()
    }
}

/// The `COALESCE(override, exif)` view shown to the user.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EffectiveMetadata {
    pub title: Option<String>,
    pub description: Option<String>,
    pub taken_at: Option<DateTime<Utc>>,
    pub location: Option<GeoPoint>,
    pub place_id: Option<i64>,
    pub orientation: Option<i16>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_patch_touches_nothing() {
        assert!(OverridePatch::default().is_empty());
    }

    #[test]
    fn setting_a_field_makes_the_patch_non_empty() {
        let patch = OverridePatch {
            title: Some(Some("Matrimonio Rossi".to_owned())),
            ..Default::default()
        };
        assert!(!patch.is_empty());
    }

    #[test]
    fn clearing_a_field_also_counts_as_touched() {
        let patch = OverridePatch {
            description: Some(None),
            ..Default::default()
        };
        assert!(!patch.is_empty());
    }
}
