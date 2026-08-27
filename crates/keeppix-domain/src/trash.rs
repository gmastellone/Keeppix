use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::DomainError;
use crate::ids::{AssetId, TrashEntryId, UserId};

/// The three options presented on every deletion: no implicit behavior, the
/// user always chooses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiskAction {
    /// The file stays on disk; the asset disappears from the index and
    /// will return on the next scan.
    Kept,
    /// `rename()` into `.keeppix-trash/` within the same library.
    /// Recoverable for 30 days.
    MovedToTrash,
    /// Deletion from disk. Irreversible: owner and admin only.
    Purged,
}

impl DiskAction {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Kept => "kept",
            Self::MovedToTrash => "moved_to_trash",
            Self::Purged => "purged",
        }
    }

    /// # Errors
    /// `DomainError::InvalidDiskAction` if the string isn't one of the three.
    pub fn parse(raw: &str) -> Result<Self, DomainError> {
        match raw {
            "kept" => Ok(Self::Kept),
            "moved_to_trash" => Ok(Self::MovedToTrash),
            "purged" => Ok(Self::Purged),
            other => Err(DomainError::InvalidDiskAction(other.to_owned())),
        }
    }
}

/// Audit/restore row for a deletion action. `trash_path` is `Some` only for
/// [`DiskAction::MovedToTrash`]: it's the place from which restoration and
/// the 30-day cleanup pick the file back up.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrashEntry {
    pub id: TrashEntryId,
    pub asset_id: AssetId,
    pub deleted_by: Option<UserId>,
    pub deleted_at: DateTime<Utc>,
    pub original_path: String,
    pub trash_path: Option<String>,
    pub disk_action: DiskAction,
    pub restored_at: Option<DateTime<Utc>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(clippy::expect_used)]
    fn disk_action_round_trips_through_its_string_form() {
        for action in [
            DiskAction::Kept,
            DiskAction::MovedToTrash,
            DiskAction::Purged,
        ] {
            assert_eq!(
                DiskAction::parse(action.as_str()).expect("round-trip"),
                action
            );
        }
    }

    #[test]
    fn unknown_disk_action_is_rejected() {
        assert!(DiskAction::parse("deleted").is_err());
    }
}
