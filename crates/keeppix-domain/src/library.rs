use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::{FolderId, LibraryId, UserId};

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
    /// Interruttore del riconoscimento facciale per questa libreria (Fase 8
    /// Task 10). Spento non rileva nulla — non "rileva ma non mostra".
    pub faces_enabled: bool,
    pub exclude_patterns: Vec<String>,
    /// Radice del culling a cartelle (Fase 9 Task 2), `NULL` finché il
    /// proprietario non ne designa una nelle impostazioni della libreria.
    /// Colonna esistente dalla migrazione 0044 (Fase 7 Task 5, che la legge
    /// per escludere il sottoalbero dall'analisi IA) — mai esposta sul
    /// dominio finché questa fase non ne aveva bisogno per davvero.
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
