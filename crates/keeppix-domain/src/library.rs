use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::{LibraryId, UserId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LibraryStatus {
    Active,
    /// Il percorso radice non è raggiungibile. In questo stato la scansione
    /// si ferma e **nulla viene cancellato**: un disco non montato non è una
    /// libreria svuotata.
    Offline,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Library {
    pub id: LibraryId,
    pub name: String,
    pub owner_id: UserId,
    pub root_path: PathBuf,
    pub scan_enabled: bool,
    pub exclude_patterns: Vec<String>,
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
