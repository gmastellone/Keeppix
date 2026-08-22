use serde::{Deserialize, Serialize};

use crate::error::DomainError;
use crate::ids::{FolderId, LibraryId};

/// Percorso materializzato di una cartella, nella forma `1.7.42`.
///
/// Le etichette sono numeri progressivi assegnati dal database, **mai** i
/// nomi delle cartelle: `ltree` ammette solo `[A-Za-z0-9_-]`, e un nome come
/// "Matrimonio Rossi 2024" non è un'etichetta valida. Tenere i nomi fuori dal
/// percorso evita anche di dover interpolare testo dell'utente in una query.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FolderPath(String);

impl FolderPath {
    #[must_use]
    pub fn root(seq: i64) -> Self {
        Self(seq.to_string())
    }

    #[must_use]
    pub fn child(&self, seq: i64) -> Self {
        Self(format!("{}.{seq}", self.0))
    }

    /// # Errors
    /// `DomainError::InvalidFolderPath` se il percorso non è una sequenza di
    /// numeri separati da punti.
    pub fn parse(raw: &str) -> Result<Self, DomainError> {
        if raw.is_empty() {
            return Err(DomainError::InvalidFolderPath("empty".to_owned()));
        }
        for label in raw.split('.') {
            if label.is_empty() || !label.bytes().all(|b| b.is_ascii_digit()) {
                return Err(DomainError::InvalidFolderPath(format!(
                    "label {label:?} is not a number"
                )));
            }
        }
        Ok(Self(raw.to_owned()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub fn depth(&self) -> usize {
        self.0.split('.').count()
    }

    /// Stessa semantica dell'operatore `<@` di ltree: un percorso discende da
    /// sé stesso.
    #[must_use]
    pub fn is_descendant_of(&self, other: &Self) -> bool {
        self.0 == other.0 || self.0.starts_with(&format!("{}.", other.0))
    }
}

/// Ruolo speciale di una cartella dentro un lotto di culling (Fase 9 Task
/// 2, spec §2.2/§2.7). `NULL` (cioè `None`) per ogni cartella normale,
/// incluse le radici dei lotti stesse (`Vacanze 2026-07/`): solo le due
/// sottocartelle che il culling crea e gestisce da sé portano un ruolo.
///
/// **È una colonna, non il nome della cartella** (Ruling nel ledger di
/// fase): riconoscere `_taken`/`_skipped` dal nome renderebbe magica una
/// cartella chiamata così per caso, e romperebbe tutto se rinominata.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CullingRole {
    Taken,
    Skipped,
}

impl CullingRole {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Taken => "taken",
            Self::Skipped => "skipped",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Folder {
    pub id: FolderId,
    pub library_id: LibraryId,
    pub parent_id: Option<FolderId>,
    /// Nome sul filesystem. Vuoto per la radice della libreria.
    pub name: String,
    pub path: FolderPath,
    pub depth: i32,
    pub culling_role: Option<CullingRole>,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn root_path_is_a_single_label() {
        let root = FolderPath::root(1);
        assert_eq!(root.as_str(), "1");
        assert_eq!(root.depth(), 1);
    }

    #[test]
    fn children_extend_the_parent() {
        let root = FolderPath::root(1);
        let child = root.child(7);
        let grandchild = child.child(42);
        assert_eq!(grandchild.as_str(), "1.7.42");
        assert_eq!(grandchild.depth(), 3);
    }

    #[test]
    fn parsing_accepts_a_numeric_path() {
        assert_eq!(FolderPath::parse("1.7.42").unwrap().as_str(), "1.7.42");
    }

    #[test]
    fn parsing_rejects_non_numeric_labels() {
        // Il nome della cartella non entra MAI nel percorso: ltree non
        // ammette spazi e accenti, e un nome interpolato sarebbe anche una
        // via di iniezione.
        assert!(FolderPath::parse("1.Matrimonio Rossi").is_err());
        assert!(FolderPath::parse("1.foto").is_err());
    }

    #[test]
    fn parsing_rejects_malformed_separators() {
        assert!(FolderPath::parse("").is_err());
        assert!(FolderPath::parse("1..7").is_err());
        assert!(FolderPath::parse(".1").is_err());
        assert!(FolderPath::parse("1.").is_err());
    }

    #[test]
    fn a_path_is_its_own_ancestor_check() {
        let root = FolderPath::root(1);
        let child = root.child(7);
        assert!(child.is_descendant_of(&root));
        assert!(!root.is_descendant_of(&child));
        assert!(root.is_descendant_of(&root), "ltree <@ include se stesso");
    }
}
