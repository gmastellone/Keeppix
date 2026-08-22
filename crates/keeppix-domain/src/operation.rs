//! Operazioni lunghe sul disco (Fase 10 Task 16): la scansione di libreria è
//! oggi il solo caso reale — la rinomina di massa (Fase 9) non esiste ancora
//! nel codice, quindi non compare qui. **Ruling**: l'infrastruttura è
//! generica apposta, così un futuro `OperationKind::BulkRename` si aggiunge
//! senza toccare il protocollo (`operation_id`, avanzamento sul WebSocket,
//! `cancel`).

use serde::{Deserialize, Serialize};

use crate::error::DomainError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationKind {
    LibraryScan,
    /// Finestra di analisi CLIP (Fase 7 Task 12): embedding + abbinamento tag.
    AiAnalysis,
    /// Passata di rilevamento/riconoscimento volti (Fase 8 Task 4) su tutta
    /// la libreria (culling escluso). Riusa lo stesso involucro `Operation`
    /// di `AiAnalysis`, non un sottosistema parallelo.
    FaceDetection,
}

impl OperationKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LibraryScan => "library_scan",
            Self::AiAnalysis => "ai_analysis",
            Self::FaceDetection => "face_detection",
        }
    }

    /// # Errors
    /// `DomainError::InvalidOperationKind` se la stringa non è un tipo noto.
    pub fn parse(raw: &str) -> Result<Self, DomainError> {
        match raw {
            "library_scan" => Ok(Self::LibraryScan),
            "ai_analysis" => Ok(Self::AiAnalysis),
            "face_detection" => Ok(Self::FaceDetection),
            other => Err(DomainError::InvalidOperationKind(other.to_owned())),
        }
    }
}

/// **Ruling (Task 16): annullare a metà produce una riuscita parziale, non
/// un rollback.** Non c'è uno stato "annullamento in corso" separato: il
/// worker vede `cancel_requested` sulla riga, si ferma al prossimo elemento e
/// scrive direttamente `Cancelled` — ciò che è già sul disco resta lì.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationStatus {
    Running,
    Done,
    Cancelled,
    Failed,
}

impl OperationStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Done => "done",
            Self::Cancelled => "cancelled",
            Self::Failed => "failed",
        }
    }

    #[must_use]
    pub const fn is_terminal(self) -> bool {
        !matches!(self, Self::Running)
    }

    /// # Errors
    /// `DomainError::InvalidOperationStatus` se la stringa non è uno dei
    /// quattro stati.
    pub fn parse(raw: &str) -> Result<Self, DomainError> {
        match raw {
            "running" => Ok(Self::Running),
            "done" => Ok(Self::Done),
            "cancelled" => Ok(Self::Cancelled),
            "failed" => Ok(Self::Failed),
            other => Err(DomainError::InvalidOperationStatus(other.to_owned())),
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn operation_kind_round_trips() {
        for kind in [
            OperationKind::LibraryScan,
            OperationKind::AiAnalysis,
            OperationKind::FaceDetection,
        ] {
            assert_eq!(
                OperationKind::parse(kind.as_str()).expect("round-trip"),
                kind
            );
        }
    }

    #[test]
    fn unknown_operation_kind_is_rejected() {
        assert!(OperationKind::parse("bulk_rename").is_err());
    }

    #[test]
    fn operation_status_round_trips() {
        for status in [
            OperationStatus::Running,
            OperationStatus::Done,
            OperationStatus::Cancelled,
            OperationStatus::Failed,
        ] {
            assert_eq!(
                OperationStatus::parse(status.as_str()).expect("round-trip"),
                status
            );
        }
    }

    #[test]
    fn only_running_is_non_terminal() {
        assert!(!OperationStatus::Running.is_terminal());
        assert!(OperationStatus::Done.is_terminal());
        assert!(OperationStatus::Cancelled.is_terminal());
        assert!(OperationStatus::Failed.is_terminal());
    }

    #[test]
    fn unknown_operation_status_is_rejected() {
        assert!(OperationStatus::parse("paused").is_err());
    }
}
