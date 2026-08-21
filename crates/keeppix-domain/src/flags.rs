use serde::{Deserialize, Serialize};

use crate::error::DomainError;

/// Voto da 0 a 5 stelle. È **per utente**, non per asset (spec §4.1): il tuo
/// 5 stelle non è il 5 stelle di tua moglie.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Rating(u8);

impl Rating {
    pub const MAX: u8 = 5;

    /// # Errors
    /// `DomainError::InvalidRating` se `raw` supera [`Self::MAX`].
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

/// Selezione di culling. Guida il filtro "scarti" e — dal Task 5 — la
/// scrittura di `xmp:Label` (convenzione darktable).
///
/// `Pick::Pick` fa scattare `clippy::enum_variant_names` (il nome della
/// variante coincide con quello dell'enum): è il nome di dominio richiesto
/// dalla spec (`Pick::{None, Pick, Reject}`), non un refuso da rinominare.
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
    /// `DomainError::InvalidPick` se la stringa non è uno dei tre valori noti.
    pub fn parse(raw: &str) -> Result<Self, DomainError> {
        match raw {
            "none" => Ok(Self::None),
            "pick" => Ok(Self::Pick),
            "reject" => Ok(Self::Reject),
            other => Err(DomainError::InvalidPick(other.to_owned())),
        }
    }
}

/// Flag di culling di **un** utente su un asset.
///
/// `favorite` è un asse **indipendente** da `pick` (spec fase-10 §7bis.1):
/// non è un riuso di `Pick::Pick` con un altro nome. Scartare uno scatto nel
/// culling (`pick = Reject`) non tocca `favorite`, e viceversa — sono due
/// colonne separate, senza logica che le accoppi.
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
