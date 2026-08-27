//! Long-running disk operations. The infrastructure is deliberately
//! generic — `BulkRename` was added as a fourth variant without touching
//! the protocol (`operation_id`, WebSocket progress, `cancel`), the same way
//! `AiAnalysis`/`FaceDetection` had already arrived as the first two after
//! `LibraryScan`.

use serde::{Deserialize, Serialize};

use crate::error::DomainError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationKind {
    LibraryScan,
    /// CLIP analysis pass: embedding + tag matching.
    AiAnalysis,
    /// Face detection/recognition pass over the whole library (culling
    /// excluded). Reuses the same `Operation` wrapper as `AiAnalysis`, not a
    /// parallel subsystem.
    FaceDetection,
    /// Bulk rename/move: unlike the other three, driven **synchronously
    /// inside the HTTP request**, not by a `keeppix-jobs` job — each step is
    /// a `move_asset`, fast, no model inference, doesn't need to survive a
    /// process restart. The same `Operation`/WebSocket/`cancel` wrapper
    /// stays identical: only who drives it changes.
    BulkRename,
}

impl OperationKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LibraryScan => "library_scan",
            Self::AiAnalysis => "ai_analysis",
            Self::FaceDetection => "face_detection",
            Self::BulkRename => "bulk_rename",
        }
    }

    /// # Errors
    /// `DomainError::InvalidOperationKind` if the string isn't a known kind.
    pub fn parse(raw: &str) -> Result<Self, DomainError> {
        match raw {
            "library_scan" => Ok(Self::LibraryScan),
            "ai_analysis" => Ok(Self::AiAnalysis),
            "face_detection" => Ok(Self::FaceDetection),
            "bulk_rename" => Ok(Self::BulkRename),
            other => Err(DomainError::InvalidOperationKind(other.to_owned())),
        }
    }
}

/// **Cancelling midway produces a partial success, not a rollback.** There
/// is no separate "cancelling" state: the worker sees `cancel_requested` on
/// the row, stops at the next element, and writes `Cancelled` directly —
/// whatever is already on disk stays there.
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
    /// `DomainError::InvalidOperationStatus` if the string isn't one of the
    /// four statuses.
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
            OperationKind::BulkRename,
        ] {
            assert_eq!(
                OperationKind::parse(kind.as_str()).expect("round-trip"),
                kind
            );
        }
    }

    #[test]
    fn unknown_operation_kind_is_rejected() {
        assert!(OperationKind::parse("bulk_delete").is_err());
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
