use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::{FolderId, LibraryId, UserId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LibraryStatus {
    Active,
    /// The root path is unreachable. In this state scanning stops and
    /// **nothing gets deleted**: an unmounted disk is not an emptied
    /// library.
    Offline,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Library {
    pub id: LibraryId,
    pub name: String,
    pub owner_id: UserId,
    pub root_path: PathBuf,
    pub scan_enabled: bool,
    /// Face recognition switch for this library. Off means nothing gets
    /// detected — not "detects but doesn't show".
    pub faces_enabled: bool,
    pub exclude_patterns: Vec<String>,
    /// Root of the folder-based culling tree, `NULL` until the owner
    /// designates one in the library settings. The column has existed since
    /// migration 0044, which reads it to exclude the subtree from AI
    /// analysis — never exposed on the domain until it was actually needed.
    pub culling_root_folder_id: Option<FolderId>,
    pub status: LibraryStatus,
    pub last_scan_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewLibrary {
    pub name: String,
    pub owner_id: UserId,
    pub root_path: PathBuf,
    pub exclude_patterns: Vec<String>,
}
