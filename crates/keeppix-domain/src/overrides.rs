use chrono::{DateTime, Utc};

/// Punto geografico WGS84, in gradi decimali — lo stesso sistema di
/// `assets.location` (`geography(Point, 4326)`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GeoPoint {
    pub lat: f64,
    pub lon: f64,
}

/// Modifica da applicare a `asset_overrides`. Ogni campo distingue "non
/// toccare" da "azzera": `None` = lascia stare, `Some(None)` = azzera,
/// `Some(Some(v))` = imposta. Un `Option<T>` da solo confonderebbe le due
/// intenzioni — è esattamente il bug che l'annullamento con valore
/// precedente `NULL` andrebbe a colpire senza questa distinzione. Le tre
/// combinazioni sono deliberate: `clippy::option_option` è disattivato qui
/// di proposito, non per pigrizia.
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

/// La vista `COALESCE(override, exif)` mostrata all'utente (spec §3.2).
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
