//! Tipi ed entità pure di Keeppix. Nessun I/O, nessun SQL, nessuna rete.

pub mod asset;
pub mod auth;
pub mod error;
pub mod folder;
pub mod ids;
pub mod library;
pub mod password;
pub mod token;
pub mod user;

pub use asset::{Asset, AssetKind, AssetName, AssetStatus, LocationSource, NewAsset};
pub use auth::{Actor, AuthContext};
pub use error::DomainError;
pub use folder::{Folder, FolderPath};
pub use ids::{AssetId, FolderId, GroupId, LibraryId, UserId};
pub use library::{Library, LibraryStatus, NewLibrary};
pub use password::{Password, PasswordHash, hash_password, verify_password};
pub use token::SessionToken;
pub use user::{NewUser, SystemRole, User, Username};
