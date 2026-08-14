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
    /// Trovato dal walker, nient'altro letto.
    Discovered,
    /// Metadati letti e derivati generati.
    Indexed,
    /// Il file non è più sul disco. Non è una cancellazione: se il disco
    /// torna, l'asset torna con i suoi rating e album.
    Offline,
    /// Illeggibile o corrotto. Compare nella pagina Problemi.
    Error,
    Trashed,
}

/// Da dove arrivano le coordinate di un asset. Serve dalla Fase 4 in poi,
/// ed è qui perché aggiungere una colonna a `assets` dopo l'indicizzazione
/// di 200.000 righe costa molto più che prevederla.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocationSource {
    Exif,
    User,
    MapPin,
    Copied,
    Gpx,
}

/// Nome di file dentro una cartella. Rifiuta i separatori di percorso, così
/// un nome non può mai far uscire dalla cartella che lo contiene.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AssetName(String);

impl AssetName {
    /// # Errors
    /// `DomainError::InvalidAssetName` se vuoto, se contiene `/`, `\` o un
    /// byte nullo, o se è `.` / `..`.
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
    /// blake3. `None` finché la fase di hash non è passata.
    pub content_hash: Option<[u8; 32]>,
    pub size_bytes: i64,
    pub mtime: DateTime<Utc>,
    pub inode: Option<i64>,
    pub kind: AssetKind,
    pub status: AssetStatus,
    /// Data di scatto normalizzata in UTC. `None` finché gli EXIF non sono
    /// stati letti; a quel punto si ripiega su `mtime` se il file non ne ha.
    pub taken_at_utc: Option<DateTime<Utc>>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub created_at: DateTime<Utc>,
}

/// Ciò che il walker sa di un file appena trovato: nient'altro che quello
/// che `stat()` restituisce.
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
}
