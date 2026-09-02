//! Database access. This is the only crate in the workspace that contains SQL.

pub mod albums;
pub mod asset_tags;
pub mod assets;
pub mod audit;
pub mod backup;
pub mod changes;
pub mod credentials;
pub mod culling;
pub mod dav_locks;
pub mod duplicates;
pub mod embeddings;
pub mod error;
pub mod faces;
pub mod flags;
pub mod folders;
pub mod geo;
pub mod groups;
pub mod guest_uploads;
pub mod home;
pub mod idempotency;
pub mod jobs;
pub mod libraries;
pub mod operations;
pub mod overrides;
pub mod permissions;
pub mod person_groups;
pub mod persons;
pub mod pgvector;
pub mod places;
pub mod preferences;
pub mod problems;
pub mod regions;
pub mod rename;
mod row;
pub mod search;
pub mod sessions;
pub mod settings;
pub mod share_links;
pub mod stacks;
pub mod tags;
pub mod timeline;
pub mod totp;
pub mod trash;
pub mod uploads;
pub mod users;
pub mod visibility;

pub use albums::{Album, AlbumAsset, AlbumBadge, AlbumPatch, AlbumRefresh, AlbumRepo, NewAlbum};
pub use asset_tags::{AssetTagDetail, AssetTagRepo, ConfirmedTag, ProposalView, TAG_MATCH_BAND};
pub use assets::{AssetExifDetail, AssetRepo, DirectPutOutcome};
pub use audit::{AuditEntry, AuditRepo};
pub use backup::{
    BackupDestination, BackupKind, BackupPreferences, BackupRepo, BackupRetention, BackupRun,
    BackupRunStatus, BackupSchedule, NewBackupDestination,
};
pub use changes::{CHANGE_LOG_PAGE, ChangeLogRepo, ChangePage};
pub use credentials::AppPasswordRepo;
pub use culling::CullingRepo;
pub use dav_locks::DavLockRepo;
pub use duplicates::{DuplicateGroup, DuplicateRepo};
pub use embeddings::{
    AssetEmbedding, EmbeddingRepo, MODEL_VERSION, MODEL_VERSION as EMBEDDING_MODEL_VERSION,
    PendingEmbedding, vector_literal,
};
pub use error::DbError;
pub use faces::{ConfirmedFace, FaceRepo, NewDetectedFace, PendingFaceScan};
pub use flags::FlagRepo;
pub use folders::FolderRepo;
pub use geo::{
    GeoRepo, MAX_UNCLUSTERED_POINTS, MapBounds, MapCluster, MapScope, TimezoneChange,
    TimezoneChangePreview, UNCLUSTERED_ZOOM,
};
pub use groups::{GroupMember, GroupRepo, GroupView};
pub use guest_uploads::{GuestUploadRepo, GuestUploadRow};
pub use home::{HomeLocation, HomeRepo, PublicAssetLocation};
pub use idempotency::{
    CachedResponse as CachedIdempotencyResponse, IdempotencyRepo,
    LookupResult as IdempotencyLookupResult, RequestFingerprint as IdempotencyRequestFingerprint,
};
pub use jobs::JobRepo;
pub use libraries::{LibraryRepo, LibraryStorage};
pub use operations::{Operation, OperationsRepo};
pub use overrides::{OverrideRepo, SidecarSource};
pub use permissions::{
    ExplainResult, NewGrant, ObjectType, PermissionGrantView, PermissionRepo, SharedWithMeItem,
    SubjectType,
};
pub use person_groups::{NewPersonGroup, PersonGroupRepo};
pub use persons::{PersonRepo, PersonSummary};
pub use pgvector::{
    ENABLE_VECTOR_SQL, PgVectorStatus, ai_schema_available, persist_pgvector_status, probe_pgvector,
};
pub use places::PlaceRepo;
pub use preferences::{
    GridDensity, NotificationPreferences, PreferencesPatchError, PreferencesRepo, UserPreferences,
};
pub use problems::{
    ComposedProblem, ErrorAsset, FailedJob, OfflineLibrary, ProblemAction, ProblemLanguage,
    ProblemSet, ProblemSeverity, ProblemsRepo,
};
pub use regions::{MapRegion, NewMapRegion, RegionDownloadSource, RegionRepo, RegionStatus};
pub use rename::{RenameBatchOutcome, RenamePreviewItem, RenameRepo, RenameUndoOutcome};
pub use search::{IsoCmp, SavedSearch, SearchNode, SearchRepo, Suggestion, SuggestionKind};
pub use sessions::{SessionRepo, SessionSummary};
pub use settings::SettingsRepo;
pub use share_links::{NewShareLink, ShareLinkRepo, ShareLinkRow};
pub use stacks::{AssetWithStack, StackBadge, StackDetails, StackMember, StackRepo};
pub use tags::{NewTag, TagPatch, TagRepo, TagView};
pub use timeline::{
    Geometry, GeometryPage, GeometryRecord, GeometryStamp, MonthBucket, TimelineRepo,
};
pub use totp::{TotpConfirmed, TotpRepo, TotpSetup, TotpStatus};
pub use trash::{TRASH_DIR_NAME, TRASH_RETENTION_DAYS, TrashRepo};
pub use uploads::{
    FinalizeOutcome, NewUploadSession, UPLOAD_TMP_DIR_NAME, UploadSessionRepo, disk_usage,
    ensure_disk_space,
};
pub use users::UserRepo;
pub use visibility::VisibilityScope;

use moka::future::Cache;
use sqlx::PgPool;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use std::str::FromStr;
use std::time::Duration;

const LIBRARY_STORAGE_CACHE_TTL: Duration = Duration::from_secs(60);

// sqlx::migrate! embeds the migration files at compile time: touch this
// module whenever a migration is added or modified, otherwise cargo will
// not rescan the migrations directory.
static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

#[derive(Clone, Debug)]
pub struct Db {
    pool: PgPool,
    permission_cache: Cache<uuid::Uuid, VisibilityScope>,
    settings_cache: Cache<String, Option<serde_json::Value>>,
    library_storage_cache: Cache<uuid::Uuid, LibraryStorage>,
}

impl Db {
    /// # Errors
    /// `DbError::Connection` if the pool cannot reach the database.
    pub async fn connect(url: &str, max_connections: u32) -> Result<Self, DbError> {
        let options = PgConnectOptions::from_str(url)
            .map_err(|err| DbError::Connection(sqlx::Error::Configuration(err.into())))?;
        Self::connect_with(options, max_connections).await
    }

    /// Pool with explicit options — used by tests that need to trace queries
    /// (`log_statements`) without touching the production connection.
    ///
    /// # Errors
    /// Same as [`Self::connect`].
    #[doc(hidden)]
    pub async fn connect_with(
        options: PgConnectOptions,
        max_connections: u32,
    ) -> Result<Self, DbError> {
        let pool = PgPoolOptions::new()
            .max_connections(max_connections)
            .acquire_timeout(std::time::Duration::from_secs(10))
            .connect_with(options)
            .await?;
        Ok(Self {
            pool,
            permission_cache: Cache::builder().max_capacity(10_000).build(),
            settings_cache: Cache::builder().max_capacity(1_000).build(),
            library_storage_cache: Cache::builder()
                .max_capacity(1_000)
                .time_to_live(LIBRARY_STORAGE_CACHE_TTL)
                .build(),
        })
    }

    /// Applies all migrations that have not yet been run.
    ///
    /// # Errors
    /// `DbError::Migration` if a migration fails or has been modified after
    /// already being applied.
    pub async fn migrate(&self) -> Result<(), DbError> {
        MIGRATOR.run(&self.pool).await?;
        Ok(())
    }

    #[must_use]
    pub const fn pool(&self) -> &PgPool {
        &self.pool
    }

    #[must_use]
    pub fn permission_cache(&self) -> &Cache<uuid::Uuid, VisibilityScope> {
        &self.permission_cache
    }

    #[must_use]
    pub fn settings_cache(&self) -> &Cache<String, Option<serde_json::Value>> {
        &self.settings_cache
    }

    #[must_use]
    pub fn library_storage_cache(&self) -> &Cache<uuid::Uuid, LibraryStorage> {
        &self.library_storage_cache
    }

    pub async fn invalidate_permission_cache_for_user(&self, user_id: keeppix_domain::UserId) {
        self.permission_cache.invalidate(&user_id.as_uuid()).await;
    }

    pub async fn invalidate_setting_cache(&self, key: &str) {
        self.settings_cache.invalidate(key).await;
    }

    /// # Errors
    /// `DbError::Connection` if the database does not respond.
    pub async fn ping(&self) -> Result<(), DbError> {
        sqlx::query_scalar::<_, i32>("SELECT 1")
            .fetch_one(&self.pool)
            .await?;
        Ok(())
    }
}
