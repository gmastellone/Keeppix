use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::DomainError;
use crate::ids::{AssetId, FolderId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetKind {
    Image,
    RawImage,
    Video,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetStatus {
    /// Found by the walker, nothing else read yet.
    Discovered,
    /// Metadata read and derivatives generated.
    Indexed,
    /// The file is no longer on disk. Not a deletion: if the disk comes
    /// back, the asset comes back with its ratings and albums.
    Offline,
    /// Unreadable or corrupted. Shows up on the Problems page.
    Error,
    Trashed,
}

/// Where an asset's coordinates come from. It's here because adding a
/// column to `assets` after indexing 200,000 rows costs a lot more than
/// planning for it up front.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocationSource {
    Exif,
    User,
    MapPin,
    Copied,
    Gpx,
}

impl LocationSource {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Exif => "exif",
            Self::User => "user",
            Self::MapPin => "map_pin",
            Self::Copied => "copied",
            Self::Gpx => "gpx",
        }
    }
}

/// A filename inside a folder. Rejects path separators, so a name can never
/// escape the folder that contains it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AssetName(String);

impl AssetName {
    /// # Errors
    /// `DomainError::InvalidAssetName` if empty, if it contains `/`, `\`, or
    /// a null byte, or if it's `.` / `..`.
    pub fn parse(raw: &str) -> Result<Self, DomainError> {
        let invalid = raw.is_empty()
            || raw.contains('/')
            || raw.contains('\\')
            || raw.contains('\0')
            || raw == "."
            || raw == "..";
        if invalid {
            return Err(DomainError::InvalidAssetName(format!("{raw:?}")));
        }
        Ok(Self(raw.to_owned()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Asset {
    pub id: AssetId,
    pub folder_id: FolderId,
    pub filename: AssetName,
    /// blake3. `None` until the hashing step has run.
    pub content_hash: Option<[u8; 32]>,
    pub size_bytes: i64,
    pub mtime: DateTime<Utc>,
    pub inode: Option<i64>,
    pub kind: AssetKind,
    pub status: AssetStatus,
    /// Capture date normalized to UTC. `None` until EXIF has been read; at
    /// that point it falls back to `mtime` if the file has none.
    pub taken_at_utc: Option<DateTime<Utc>>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub thumbhash: Option<Vec<u8>>,
    pub created_at: DateTime<Utc>,
}

/// What the walker knows about a freshly found file: nothing more than
/// what `stat()` returns.
#[derive(Debug, Clone)]
pub struct NewAsset {
    pub folder_id: FolderId,
    pub filename: AssetName,
    pub size_bytes: i64,
    pub mtime: DateTime<Utc>,
    pub inode: Option<i64>,
    pub kind: AssetKind,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_name_accepts_ordinary_filenames() {
        assert!(AssetName::parse("DSC_0042.ARW").is_ok());
        assert!(AssetName::parse("foto di famiglia.jpg").is_ok());
        assert!(AssetName::parse("émoji 🎉.png").is_ok());
    }

    #[test]
    fn asset_name_rejects_path_separators() {
        assert!(AssetName::parse("../etc/passwd").is_err());
        assert!(AssetName::parse("a/b.jpg").is_err());
        assert!(AssetName::parse("a\\b.jpg").is_err());
    }

    #[test]
    fn asset_name_rejects_dot_entries_and_empty() {
        assert!(AssetName::parse(".").is_err());
        assert!(AssetName::parse("..").is_err());
        assert!(AssetName::parse("").is_err());
    }

    #[test]
    fn location_source_strings_match_the_database_constraint() {
        assert_eq!(LocationSource::Exif.as_str(), "exif");
        assert_eq!(LocationSource::User.as_str(), "user");
        assert_eq!(LocationSource::MapPin.as_str(), "map_pin");
        assert_eq!(LocationSource::Copied.as_str(), "copied");
        assert_eq!(LocationSource::Gpx.as_str(), "gpx");
    }
}
