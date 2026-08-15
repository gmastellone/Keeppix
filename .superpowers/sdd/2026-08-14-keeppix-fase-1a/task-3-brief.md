## Task 3: Tipi di dominio per librerie, cartelle e asset

Tutti i tipi in un solo task, perché sono puri, si definiscono a vicenda e
non hanno senso separati. Nessun I/O: `keeppix-domain` resta senza database.

**Files:**
- Create: `crates/keeppix-domain/src/library.rs`, `folder.rs`, `asset.rs`
- Modify: `crates/keeppix-domain/src/ids.rs`, `lib.rs`

**Interfaces:**
- Consumes: la macro `id_type!` e `UserId` (Fase 0).
- Produces:
  - `LibraryId`, `FolderId`, `AssetId` — stessa macro, UUID v7.
  - `Library { id, name, owner_id, root_path: PathBuf, scan_enabled, exclude_patterns, status, last_scan_at, created_at }`, `LibraryStatus::{Active, Offline}`, `NewLibrary`.
  - `Folder { id, library_id, parent_id, name, path: FolderPath, depth }`.
  - `FolderPath` — newtype sul percorso `ltree`, con `root(seq) -> Self`, `child(&self, seq) -> Self`, `as_str()`, `depth()`, `parse(&str) -> Result<Self, DomainError>`.
  - `Asset { id, folder_id, filename, content_hash: Option<[u8;32]>, size_bytes, mtime, inode, kind, status, taken_at_utc, width, height, created_at }`, `AssetKind::{Image, RawImage, Video, Unknown}`, `AssetStatus::{Discovered, Indexed, Offline, Error, Trashed}`, `LocationSource::{Exif, User, MapPin, Copied, Gpx}`, `NewAsset`.
  - `DomainError::{InvalidFolderPath(String), InvalidAssetName(String)}` aggiunti.

- [ ] **Step 1: Scrivere i test che falliscono**

`crates/keeppix-domain/src/folder.rs`, in fondo:

```rust
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
```

- [ ] **Step 2: Eseguire e verificare il fallimento**

Run: `cargo test -p keeppix-domain folder`
Expected: FAIL — `cannot find type FolderPath`.

- [ ] **Step 3: Aggiungere gli id**

In `crates/keeppix-domain/src/ids.rs`, sotto quelli esistenti:

```rust
id_type!(LibraryId);
id_type!(FolderId);
id_type!(AssetId);
```

- [ ] **Step 4: Implementare `folder.rs`**

```rust
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Folder {
    pub id: FolderId,
    pub library_id: LibraryId,
    pub parent_id: Option<FolderId>,
    /// Nome sul filesystem. Vuoto per la radice della libreria.
    pub name: String,
    pub path: FolderPath,
    pub depth: i32,
}
```

- [ ] **Step 5: Implementare `library.rs`**

```rust
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
```

- [ ] **Step 6: Implementare `asset.rs`**

```rust
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
```

Aggiungere i test per `AssetName` in fondo a `asset.rs`:

```rust
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
```

- [ ] **Step 7: Aggiungere le varianti d'errore ed esportare**

In `error.rs`:

```rust
    #[error("invalid folder path: {0}")]
    InvalidFolderPath(String),
    #[error("invalid asset name: {0}")]
    InvalidAssetName(String),
```

In `lib.rs`:

```rust
pub mod asset;
pub mod folder;
pub mod library;

pub use asset::{Asset, AssetKind, AssetName, AssetStatus, LocationSource, NewAsset};
pub use folder::{Folder, FolderPath};
pub use ids::{AssetId, FolderId, LibraryId};
pub use library::{Library, LibraryStatus, NewLibrary};
```

- [ ] **Step 8: Eseguire i test**

Run: `cargo test -p keeppix-domain && cargo clippy -p keeppix-domain --all-targets -- -D warnings`
Expected: PASS — 22 test esistenti più 9 nuovi.

- [ ] **Step 9: Commit**

```bash
git add crates/keeppix-domain
git commit -m "feat(domain): add library, folder and asset types"
```

---

