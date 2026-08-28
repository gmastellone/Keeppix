//! Partial-success envelope for bulk operations.
//!
//! HTTP **200** even with partial failures: the body lists `succeeded` and
//! `failed` by id. `reason` is a closed set (kebab-case) so the frontend
//! can decide whether to show "Retry" without parsing free-form text.

use keeppix_db::DbError;
use keeppix_domain::{AssetId, BatchId, FaceId};
use serde::{Deserialize, Serialize};

/// Outcome of a bulk operation: each element is its own transaction (not
/// all-or-nothing over the whole batch).
#[derive(Debug, Clone, Default, Serialize, Deserialize, utoipa::ToSchema)]
pub struct BulkOutcome {
    #[schema(value_type = Vec<String>)]
    pub succeeded: Vec<AssetId>,
    pub failed: Vec<BulkFailure>,
    /// Present when the operation recorded an undoable batch.
    /// `flags/batch` leaves it `null`: votes aren't undoable via
    /// `metadata/batch/{id}/undo`.
    #[schema(value_type = Option<String>)]
    pub batch_id: Option<BatchId>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct BulkFailure {
    #[schema(value_type = String)]
    pub id: AssetId,
    pub reason: FailureReason,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// Closed taxonomy. The fifth value `Unknown` is honest: better one extra
/// case than pretending it's one of the other four.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "kebab-case")]
#[schema(rename_all = "kebab-case")]
pub enum FailureReason {
    Unreachable,
    PermissionDenied,
    FileMissing,
    Timeout,
    /// Destination of a move/rename already occupied by another asset
    /// (`DbError::Collision`) — distinct from `Unknown`: a 500-file batch
    /// needs to be able to say "these collided", not just "something
    /// didn't work".
    Collision,
    Unknown,
}

impl FailureReason {
    /// Maps a [`DbError`] to the closest nature **without inventing one**.
    /// Connection errors → `unreachable`; permissions / probing →
    /// `permission-denied`; IO for a missing file → `file-missing`;
    /// name/folder collision → `collision`; the rest (Conflict, Corrupted,
    /// …) → `unknown`.
    #[must_use]
    pub fn from_db_error(error: &DbError) -> Self {
        match error {
            DbError::Connection(_) => Self::Unreachable,
            DbError::Forbidden | DbError::NotFound => Self::PermissionDenied,
            DbError::Io(message) if looks_like_missing(message) => Self::FileMissing,
            DbError::Io(message) if looks_like_permission(message) => Self::PermissionDenied,
            DbError::Collision(_) => Self::Collision,
            DbError::Io(_)
            | DbError::Migration(_)
            | DbError::Conflict(_)
            | DbError::Corrupted(_)
            | DbError::InsufficientStorage
            | DbError::Gone => Self::Unknown,
        }
    }
}

impl BulkFailure {
    #[must_use]
    pub fn from_db_error(id: AssetId, error: &DbError) -> Self {
        Self {
            id,
            reason: FailureReason::from_db_error(error),
            detail: detail_for(error),
        }
    }
}

impl BulkOutcome {
    #[must_use]
    pub fn from_partition(
        succeeded: Vec<AssetId>,
        failed: &[(AssetId, DbError)],
        batch_id: Option<BatchId>,
    ) -> Self {
        Self {
            succeeded,
            failed: failed
                .iter()
                .map(|(id, error)| BulkFailure::from_db_error(*id, error))
                .collect(),
            batch_id,
        }
    }
}

/// Twin of [`BulkOutcome`] typed on [`FaceId`] — the face review queue
/// works on faces, not assets. Same shape and same [`FailureReason`] as
/// `BulkOutcome`: `FailureReason::FileMissing` doesn't make sense here (no
/// filesystem path is involved), but keeping it in the shared enum costs
/// less than a second taxonomy — the caller simply never produces it for a
/// face.
#[derive(Debug, Clone, Default, Serialize, Deserialize, utoipa::ToSchema)]
pub struct FaceBulkOutcome {
    #[schema(value_type = Vec<String>)]
    pub succeeded: Vec<FaceId>,
    pub failed: Vec<FaceBulkFailure>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct FaceBulkFailure {
    #[schema(value_type = String)]
    pub id: FaceId,
    pub reason: FailureReason,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl FaceBulkOutcome {
    #[must_use]
    pub fn from_partition(succeeded: Vec<FaceId>, failed: &[(FaceId, DbError)]) -> Self {
        Self {
            succeeded,
            failed: failed
                .iter()
                .map(|(id, error)| FaceBulkFailure {
                    id: *id,
                    reason: FailureReason::from_db_error(error),
                    detail: detail_for(error),
                })
                .collect(),
        }
    }
}

fn detail_for(error: &DbError) -> Option<String> {
    match error {
        DbError::Connection(error) => Some(error.to_string()),
        DbError::Migration(message)
        | DbError::Io(message)
        | DbError::Conflict(message)
        | DbError::Corrupted(message)
        | DbError::Collision(message) => Some(message.clone()),
        DbError::Forbidden | DbError::NotFound | DbError::InsufficientStorage | DbError::Gone => {
            None
        }
    }
}

fn looks_like_missing(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("no such file")
        || lower.contains("not found")
        || lower.contains("enoent")
        || lower.contains("does not exist")
}

fn looks_like_permission(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("permission denied")
        || lower.contains("read-only")
        || lower.contains("readonly")
        || lower.contains("eacces")
        || lower.contains("eperm")
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn failure_reason_serializes_as_kebab_case() {
        let json = serde_json::to_string(&FailureReason::PermissionDenied).expect("serialize");
        assert_eq!(json, "\"permission-denied\"");
    }

    #[test]
    fn forbidden_maps_to_permission_denied() {
        assert_eq!(
            FailureReason::from_db_error(&DbError::Forbidden),
            FailureReason::PermissionDenied
        );
    }
}
