use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::DomainError;
use crate::ids::{FolderId, LibraryId};

/// Materialized path of a folder, in the form `1.7.42`.
///
/// The labels are sequential numbers assigned by the database, **never**
/// folder names: `ltree` only allows `[A-Za-z0-9_-]`, and a name like
/// "Wedding Album 2024" is not a valid label. Keeping names out of the path
/// also avoids having to interpolate user text into a query.
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
    /// `DomainError::InvalidFolderPath` if the path isn't a sequence of
    /// dot-separated numbers.
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

    /// Same semantics as ltree's `<@` operator: a path descends from
    /// itself.
    #[must_use]
    pub fn is_descendant_of(&self, other: &Self) -> bool {
        self.0 == other.0 || self.0.starts_with(&format!("{}.", other.0))
    }
}

/// Special role of a folder inside a culling lot. `NULL` (i.e. `None`) for
/// every normal folder, including the lot roots themselves
/// (`Vacation 2026-07/`): only the two subfolders that culling creates and
/// manages itself carry a role.
///
/// **It's a column, not the folder name**: recognizing `_taken`/`_skipped`
/// by name would make a folder that happens to be named that way behave
/// magically, and would break everything if renamed.
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
    /// Name on the filesystem. Empty for the library root.
    pub name: String,
    pub path: FolderPath,
    pub depth: i32,
    pub culling_role: Option<CullingRole>,
}

/// A culling lot: the top-level folder under the designated root —
/// `Vacation 2026-07/`, not `_taken`/`_skipped`, which are its two special
/// children. These three counts are **the only exact count left in the
/// whole application** (the other five were removed from lists) because
/// "how many do I have left to look at" is literally the question the user
/// is asking while looking at a lot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CullingLot {
    pub folder_id: FolderId,
    pub name: String,
    pub created_at: DateTime<Utc>,
    /// Photos still in the lot root, neither picked nor rejected yet.
    pub pending: i64,
    /// Photos in `_taken`.
    pub taken: i64,
    /// Photos in `_skipped`.
    pub skipped: i64,
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
        // The folder name NEVER enters the path: ltree doesn't allow spaces
        // or accented characters, and an interpolated name would also be an
        // injection vector.
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
        assert!(root.is_descendant_of(&root), "ltree <@ includes itself");
    }
}
