//! Accesso al database. È l'unico crate del workspace che contiene SQL.

pub mod albums;
pub mod assets;
pub mod audit;
pub mod changes;
pub mod duplicates;
pub mod error;
pub mod flags;
pub mod folders;
pub mod geo;
pub mod groups;
pub mod guest_uploads;
pub mod jobs;
pub mod libraries;
pub mod overrides;
pub mod permissions;
pub mod places;
pub mod problems;
mod row;
pub mod search;
pub mod sessions;
pub mod settings;
pub mod share_links;
pub mod stacks;
pub mod timeline;
pub mod trash;
pub mod users;
pub mod visibility;

pub use albums::{Album, AlbumAsset, AlbumPatch, AlbumRepo, NewAlbum};
pub use assets::AssetRepo;
pub use audit::{AuditEntry, AuditRepo};
pub use changes::{ChangeLogRepo, ChangePage};
pub use duplicates::{DuplicateGroup, DuplicateRepo};
pub use error::DbError;
pub use flags::FlagRepo;
pub use folders::FolderRepo;
pub use geo::{GeoRepo, MAX_UNCLUSTERED_POINTS, MapBounds, MapCluster, MapScope, UNCLUSTERED_ZOOM};
pub use groups::{GroupMember, GroupRepo, GroupView};
pub use guest_uploads::{GuestUploadRepo, GuestUploadRow};
pub use jobs::JobRepo;
pub use libraries::LibraryRepo;
pub use overrides::{OverrideRepo, SidecarSource};
pub use permissions::{
    ExplainResult, NewGrant, ObjectType, PermissionGrantView, PermissionRepo, SubjectType,
};
pub use places::PlaceRepo;
pub use problems::{ProblemSet, ProblemsRepo};
pub use search::{IsoCmp, SavedSearch, SearchNode, SearchRepo};
pub use sessions::SessionRepo;
pub use settings::SettingsRepo;
pub use share_links::{NewShareLink, ShareLinkRepo, ShareLinkRow};
pub use stacks::{StackDetails, StackMember, StackRepo};
pub use timeline::{MonthBucket, TimelineRepo};
pub use trash::{TRASH_DIR_NAME, TRASH_RETENTION_DAYS, TrashRepo};
pub use users::UserRepo;
pub use visibility::VisibilityScope;

use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

// sqlx::migrate! incorpora i file a compile time: toccare questo modulo
// quando si aggiunge o si modifica una migrazione, altrimenti cargo non
// rivede la directory. 0020_places.
static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

#[derive(Clone, Debug)]
pub struct Db {
    pool: PgPool,
}

impl Db {
    /// # Errors
    /// `DbError::Connection` se il pool non riesce a raggiungere il database.
    pub async fn connect(url: &str, max_connections: u32) -> Result<Self, DbError> {
        let pool = PgPoolOptions::new()
            .max_connections(max_connections)
            .acquire_timeout(std::time::Duration::from_secs(10))
            .connect(url)
            .await?;
        Ok(Self { pool })
    }

    /// Applica tutte le migrazioni non ancora eseguite.
    ///
    /// # Errors
    /// `DbError::Migration` se una migrazione fallisce o è stata modificata
    /// dopo essere stata applicata.
    pub async fn migrate(&self) -> Result<(), DbError> {
        MIGRATOR.run(&self.pool).await?;
        Ok(())
    }

    #[must_use]
    pub const fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// # Errors
    /// `DbError::Connection` se il database non risponde.
    pub async fn ping(&self) -> Result<(), DbError> {
        sqlx::query_scalar::<_, i32>("SELECT 1")
            .fetch_one(&self.pool)
            .await?;
        Ok(())
    }
}
