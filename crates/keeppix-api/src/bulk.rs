//! Involucro di riuscita parziale per le operazioni di massa (spec Fase 10 §3).
//!
//! HTTP **200** anche con fallimenti parziali: il corpo elenca `succeeded` e
//! `failed` per id. `reason` è un insieme chiuso (kebab-case) così il
//! frontend decide se mostrare "Riprova" senza parsare testo libero.

use keeppix_db::DbError;
use keeppix_domain::{AssetId, BatchId};
use serde::{Deserialize, Serialize};

/// Esito di un'operazione di massa: ogni elemento è una transazione a sé
/// (spec §3: non all-or-nothing sul lotto intero).
#[derive(Debug, Clone, Default, Serialize, Deserialize, utoipa::ToSchema)]
pub struct BulkOutcome {
    #[schema(value_type = Vec<String>)]
    pub succeeded: Vec<AssetId>,
    pub failed: Vec<BulkFailure>,
    /// Presente quando l'operazione ha registrato un batch annullabile.
    /// `flags/batch` lo lascia `null`: i voti non sono annullabili via
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

/// Tassonomia chiusa (spec §7). Il quinto valore `Unknown` è onesto: meglio
/// un caso in più che fingere una delle quattro nature.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "kebab-case")]
#[schema(rename_all = "kebab-case")]
pub enum FailureReason {
    Unreachable,
    PermissionDenied,
    FileMissing,
    Timeout,
    Unknown,
}

impl FailureReason {
    /// Mappa un [`DbError`] sulla natura più vicina **senza inventare**.
    /// Errori di connessione → `unreachable`; permessi / probing →
    /// `permission-denied`; IO di file assente → `file-missing`; il resto
    /// (Conflict, Corrupted, …) → `unknown`.
    #[must_use]
    pub fn from_db_error(error: &DbError) -> Self {
        match error {
            DbError::Connection(_) => Self::Unreachable,
            DbError::Forbidden | DbError::NotFound => Self::PermissionDenied,
            DbError::Io(message) if looks_like_missing(message) => Self::FileMissing,
            DbError::Io(message) if looks_like_permission(message) => Self::PermissionDenied,
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

fn detail_for(error: &DbError) -> Option<String> {
    match error {
        DbError::Io(message) | DbError::Conflict(message) | DbError::Corrupted(message) => {
            Some(message.clone())
        }
        DbError::Connection(error) => Some(error.to_string()),
        DbError::Migration(message) => Some(message.clone()),
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
