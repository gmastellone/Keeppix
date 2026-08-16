use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::DomainError;
use crate::ids::{AssetId, TrashEntryId, UserId};

/// Le tre opzioni presentate ad ogni cancellazione (spec §6): nessun
/// comportamento implicito, l'utente scelge sempre.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiskAction {
    /// Il file resta sul disco; l'asset sparisce dall'indice e tornerà alla
    /// prossima scansione.
    Kept,
    /// `rename()` in `.keeppix-trash/` dentro la stessa libreria. Recuperabile
    /// per 30 giorni.
    MovedToTrash,
    /// Cancellazione dal disco. Irreversibile: solo owner e admin.
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
    /// `DomainError::InvalidDiskAction` se la stringa non è una delle tre.
    pub fn parse(raw: &str) -> Result<Self, DomainError> {
        match raw {
            "kept" => Ok(Self::Kept),
            "moved_to_trash" => Ok(Self::MovedToTrash),
            "purged" => Ok(Self::Purged),
            other => Err(DomainError::InvalidDiskAction(other.to_owned())),
        }
    }
}

/// Riga di audit/ripristino di un'azione di cancellazione (spec §6).
/// `trash_path` è `Some` solo per [`DiskAction::MovedToTrash`]: è il posto
/// da cui il ripristino e la pulizia dei 30 giorni riprendono il file.
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
