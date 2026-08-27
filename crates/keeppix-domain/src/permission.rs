//! Role on the object, distinct from the system role (`admin` / `user`).

use serde::{Deserialize, Serialize};

/// `viewer` or `editor`. `owner` is not a value in `permissions`: it's
/// whoever owns the library. The highest applicable permission wins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObjectRole {
    Viewer,
    Editor,
}

impl ObjectRole {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Viewer => "viewer",
            Self::Editor => "editor",
        }
    }

    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "viewer" => Some(Self::Viewer),
            "editor" => Some(Self::Editor),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ObjectRole;

    #[test]
    fn editor_outranks_viewer() {
        assert!(ObjectRole::Editor > ObjectRole::Viewer);
    }
}
