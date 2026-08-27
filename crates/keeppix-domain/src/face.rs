//! Faces, people, and groups of people.
//!
//! A face (`Face`) is a detection on ONE asset: it's born without a person,
//! and the assignment comes from incremental clustering or from a human. A
//! person (`Person`) is an identity that persists over time across multiple
//! faces and multiple assets — distinct from the `groups` used for user
//! permissions: `PersonGroup` groups *photographed* people, not users who
//! access the gallery.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::DomainError;
use crate::ids::{AssetId, FaceId, PersonGroupId, PersonId, UserId};

/// Face bounding box in RELATIVE coordinates (0..1): survives derivatives
/// of a different size without recomputation.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FaceBBox {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Face {
    pub id: FaceId,
    pub asset_id: AssetId,
    pub bbox: FaceBBox,
    pub landmarks: Option<serde_json::Value>,
    pub detect_score: f32,
    /// Sharpness/size/pose: a blurry 20px face must not decide a cluster's
    /// identity.
    pub quality: Option<f32>,
    pub person_id: Option<PersonId>,
    /// Human decision on THIS face. `None` = the automation can still act;
    /// once set, it is never reassigned by a recomputation.
    pub assigned_by: Option<UserId>,
    pub assigned_at: Option<DateTime<Utc>>,
    /// Declared false positive ("not a face"): permanent, never re-proposed
    /// by any later reanalysis.
    pub rejected_at: Option<DateTime<Utc>>,
    /// Incremental clustering candidate when the distance to the nearest
    /// centroid is uncertain: not assigned (`person_id` stays `None`), but
    /// proposed in the review queue.
    pub proposed_person_id: Option<PersonId>,
    pub proposed_score: Option<f32>,
    pub model_version: String,
    pub created_at: DateTime<Utc>,
}

impl Face {
    #[must_use]
    pub const fn is_rejected(&self) -> bool {
        self.rejected_at.is_some()
    }

    #[must_use]
    pub const fn is_human_assigned(&self) -> bool {
        self.assigned_by.is_some()
    }
}

/// A person's name: if present, it cannot be blank (a defect flagged in an
/// earlier prototype that didn't check this — not to be repeated).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PersonName(String);

impl PersonName {
    /// # Errors
    /// `DomainError::BlankPersonName` if, after trimming leading/trailing
    /// whitespace, the string is empty.
    pub fn parse(raw: &str) -> Result<Self, DomainError> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(DomainError::BlankPersonName);
        }
        Ok(Self(trimmed.to_owned()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub fn into_string(self) -> String {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Person {
    pub id: PersonId,
    /// `None` = "Person 4" with N photos is already useful: the name is
    /// optional, not a server-generated placeholder.
    pub name: Option<String>,
    pub cover_face_id: Option<FaceId>,
    /// For background strangers who aren't of interest but aren't false
    /// positives either.
    pub hidden_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl Person {
    #[must_use]
    pub const fn is_hidden(&self) -> bool {
        self.hidden_at.is_some()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersonGroup {
    pub id: PersonGroupId,
    pub name: String,
    pub created_by: UserId,
    pub created_at: DateTime<Utc>,
}

/// A pair of people a human has separated: the automation never reunites
/// them again — this is the table that separates a usable system from a
/// frustrating one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersonSeparation {
    pub person_a: PersonId,
    pub person_b: PersonId,
    pub created_by: UserId,
    pub created_at: DateTime<Utc>,
}

impl PersonSeparation {
    /// Normalizes the pair so that `person_a < person_b`, as required by
    /// the schema's `CHECK`: an unordered pair, always stored the same way
    /// round.
    #[must_use]
    pub fn ordered(a: PersonId, b: PersonId) -> (PersonId, PersonId) {
        if a.as_uuid() < b.as_uuid() {
            (a, b)
        } else {
            (b, a)
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn person_name_rejects_blank() {
        assert!(PersonName::parse("").is_err());
        assert!(PersonName::parse("   ").is_err());
    }

    #[test]
    fn person_name_trims() {
        assert_eq!(PersonName::parse("  Marta  ").unwrap().as_str(), "Marta");
    }

    #[test]
    fn separation_pair_is_ordered() {
        let a = PersonId::new();
        let b = PersonId::new();
        let (lo, hi) = PersonSeparation::ordered(a, b);
        assert!(lo.as_uuid() < hi.as_uuid());
        let (lo2, hi2) = PersonSeparation::ordered(b, a);
        assert_eq!((lo, hi), (lo2, hi2));
    }

    #[test]
    fn face_human_assignment_is_visible() {
        let face = Face {
            id: FaceId::new(),
            asset_id: AssetId::new(),
            bbox: FaceBBox {
                x: 0.1,
                y: 0.1,
                w: 0.2,
                h: 0.2,
            },
            landmarks: None,
            detect_score: 0.9,
            quality: Some(0.8),
            person_id: Some(PersonId::new()),
            assigned_by: Some(UserId::new()),
            assigned_at: Some(Utc::now()),
            rejected_at: None,
            proposed_person_id: None,
            proposed_score: None,
            model_version: "yunet+sface".to_owned(),
            created_at: Utc::now(),
        };
        assert!(face.is_human_assigned());
        assert!(!face.is_rejected());
    }
}
