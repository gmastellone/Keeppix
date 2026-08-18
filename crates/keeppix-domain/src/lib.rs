//! Tipi ed entità pure di Keeppix. Nessun I/O, nessun SQL, nessuna rete.

pub mod asset;
pub mod auth;
pub mod error;
pub mod exif;
pub mod flags;
pub mod folder;
pub mod ids;
pub mod job;
pub mod library;
pub mod overrides;
pub mod password;
pub mod permission;
pub mod token;
pub mod trash;
pub mod user;

pub use asset::{Asset, AssetKind, AssetName, AssetStatus, LocationSource, NewAsset};
pub use auth::{Actor, AuthContext, ShareLinkParams};
pub use error::DomainError;
pub use exif::ExifData;
pub use flags::{AssetFlags, Pick, Rating};
pub use folder::{Folder, FolderPath};
pub use ids::{
    AlbumId, AssetId, BatchId, FolderId, GroupId, LibraryId, StackId, TrashEntryId, UserId,
};
pub use job::{Job, JobKind, JobPriority, JobStatus};
pub use library::{Library, LibraryStatus, NewLibrary};
pub use overrides::{EffectiveMetadata, GeoPoint, OverridePatch};
pub use password::{Password, PasswordHash, hash_password, verify_password};
pub use permission::ObjectRole;
pub use token::{SessionToken, ShareToken};
pub use trash::{DiskAction, TrashEntry};
pub use user::{NewUser, SystemRole, User, Username};
