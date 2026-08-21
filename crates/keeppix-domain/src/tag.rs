//! Vocabolario condiviso di tag e categorie (Fase 7).

use std::fmt;
use std::str::FromStr;

use crate::error::DomainError;

/// `tags.kind`: categoria (contenitore) o tag (assegnabile alle foto).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TagKind {
    Category,
    Tag,
}

impl TagKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Category => "category",
            Self::Tag => "tag",
        }
    }
}

impl fmt::Display for TagKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for TagKind {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "category" => Ok(Self::Category),
            "tag" => Ok(Self::Tag),
            other => Err(DomainError::InvalidTagKind(other.to_owned())),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn kind_roundtrips() {
        assert_eq!(
            TagKind::Tag.as_str().parse::<TagKind>().unwrap(),
            TagKind::Tag
        );
        assert_eq!(
            TagKind::Category.as_str().parse::<TagKind>().unwrap(),
            TagKind::Category
        );
    }
}
