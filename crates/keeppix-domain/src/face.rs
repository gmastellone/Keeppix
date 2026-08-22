//! Volti, persone e gruppi di persone (Fase 8).
//!
//! Un volto (`Face`) è un rilevamento su UN asset: nasce senza persona,
//! l'assegnazione arriva dal raggruppamento incrementale o da un umano. Una
//! persona (`Person`) è un'identità che vive nel tempo attraverso più volti
//! e più asset — distinta dai `groups` della Fase 3 (permessi utenti):
//! `PersonGroup` raggruppa persone *fotografate*, non utenti che accedono
//! alla galleria.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::DomainError;
use crate::ids::{AssetId, FaceId, PersonGroupId, PersonId, UserId};

/// Riquadro del volto in coordinate RELATIVE (0..1): sopravvive a derivati
/// di dimensione diversa senza ricalcolo (spec fase-8 §3).
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
    /// Nitidezza/dimensione/posa: un volto sfocato di 20px non deve decidere
    /// l'identità di un cluster (spec §3).
    pub quality: Option<f32>,
    pub person_id: Option<PersonId>,
    /// Decisione umana su QUESTO volto. `None` = l'automatismo può ancora
    /// agire; una volta impostato non viene mai riassegnato da un ricalcolo
    /// (spec §4.3).
    pub assigned_by: Option<UserId>,
    pub assigned_at: Option<DateTime<Utc>>,
    /// Falso positivo dichiarato («non è un volto»): permanente, non
    /// riproposto da nessuna rianalisi successiva.
    pub rejected_at: Option<DateTime<Utc>>,
    /// Candidato del raggruppamento incrementale quando la distanza dal
    /// centroide più vicino è dubbia (spec §4.1): non assegnato
    /// (`person_id` resta `None`), ma proposto in coda di revisione.
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

/// Nome di una persona: se presente non può essere vuoto (Task 6 — il
/// prototipo non lo controlla, difetto segnalato da non replicare).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PersonName(String);

impl PersonName {
    /// # Errors
    /// `DomainError::BlankPersonName` se, tolti gli spazi iniziali/finali,
    /// la stringa è vuota.
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
    /// `None` = «Persona 4» con N foto è già utile (spec §3): il nome è
    /// opzionale, non un placeholder generato lato server.
    pub name: Option<String>,
    pub cover_face_id: Option<FaceId>,
    /// Per gli sconosciuti sullo sfondo che non interessano ma non sono
    /// falsi positivi (spec §5).
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

/// Coppia di persone che un umano ha separato: l'automatismo non le riunisce
/// mai più (spec §4.3) — è la tabella che distingue un sistema utilizzabile
/// da uno frustrante.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersonSeparation {
    pub person_a: PersonId,
    pub person_b: PersonId,
    pub created_by: UserId,
    pub created_at: DateTime<Utc>,
}

impl PersonSeparation {
    /// Normalizza la coppia in modo che `person_a < person_b`, come richiede
    /// il `CHECK` dello schema (spec §3): coppia non ordinata, memorizzata
    /// sempre nello stesso verso.
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
            model_version: "scrfd-500mf+arcface".to_owned(),
            created_at: Utc::now(),
        };
        assert!(face.is_human_assigned());
        assert!(!face.is_rejected());
    }
}
