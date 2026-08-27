use serde::{Deserialize, Serialize};

use crate::error::DomainError;

/// Rating from 0 to 5 stars. It's **per user**, not per asset: your 5 stars
/// isn't the same as your spouse's 5 stars.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Rating(u8);

impl Rating {
    pub const MAX: u8 = 5;

    /// # Errors
    /// `DomainError::InvalidRating` if `raw` exceeds [`Self::MAX`].
    pub fn parse(raw: u8) -> Result<Self, DomainError> {
        if raw > Self::MAX {
            return Err(DomainError::InvalidRating(raw));
        }
        Ok(Self(raw))
    }

    #[must_use]
    pub const fn value(self) -> u8 {
        self.0
    }
}

/// Culling selection. Drives the "rejects" filter and the writing of
/// `xmp:Label` (darktable convention).
///
/// `Pick::Pick` triggers `clippy::enum_variant_names` (the variant name
/// matches the enum name): it's the domain name the design calls for
/// (`Pick::{None, Pick, Reject}`), not a typo to rename.
#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Pick {
    #[default]
    None,
    Pick,
    Reject,
}

impl Pick {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Pick => "pick",
            Self::Reject => "reject",
        }
    }

    /// # Errors
    /// `DomainError::InvalidPick` if the string isn't one of the three known values.
    pub fn parse(raw: &str) -> Result<Self, DomainError> {
        match raw {
            "none" => Ok(Self::None),
            "pick" => Ok(Self::Pick),
            "reject" => Ok(Self::Reject),
            other => Err(DomainError::InvalidPick(other.to_owned())),
        }
    }
}

/// Culling flags for **one** user on an asset.
///
/// `favorite` is an axis **independent** from `pick`: it isn't a reuse of
/// `Pick::Pick` under another name. Rejecting a shot in culling
/// (`pick = Reject`) doesn't touch `favorite`, and vice versa — they're two
/// separate columns, with no logic coupling them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetFlags {
    pub rating: Option<Rating>,
    pub pick: Pick,
    pub color_label: Option<String>,
    pub favorite: bool,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn rating_accepts_the_full_range() {
        for raw in 0..=5u8 {
            assert_eq!(Rating::parse(raw).expect("in range").value(), raw);
        }
    }

    #[test]
    fn rating_rejects_out_of_range() {
        assert!(Rating::parse(6).is_err());
        assert!(Rating::parse(255).is_err());
    }

    #[test]
    fn pick_round_trips_through_its_string_form() {
        for pick in [Pick::None, Pick::Pick, Pick::Reject] {
            assert_eq!(Pick::parse(pick.as_str()).expect("round-trip"), pick);
        }
    }

    #[test]
    fn unknown_pick_is_rejected() {
        assert!(Pick::parse("maybe").is_err());
    }

    #[test]
    fn default_flags_are_unvoted() {
        let flags = AssetFlags::default();
        assert_eq!(flags.rating, None);
        assert_eq!(flags.pick, Pick::None);
        assert_eq!(flags.color_label, None);
        assert!(!flags.favorite);
    }
}
