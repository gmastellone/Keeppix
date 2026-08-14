//! Accesso al database. È l'unico crate del workspace che contiene SQL.

pub mod assets;
pub mod changes;
pub mod error;
pub mod folders;
pub mod jobs;
pub mod libraries;
mod row;
pub mod sessions;
pub mod settings;
pub mod users;
pub mod visibility;

pub use assets::AssetRepo;
pub use changes::{ChangeLogRepo, ChangePage};
pub use error::DbError;
pub use folders::FolderRepo;
pub use jobs::JobRepo;
pub use libraries::LibraryRepo;
pub use sessions::SessionRepo;
pub use settings::SettingsRepo;
pub use users::UserRepo;
pub use visibility::VisibilityScope;

use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

// sqlx::migrate! incorpora i file a compile time: toccare questo modulo
// quando si aggiunge o si modifica una migrazione, altrimenti cargo non
// rivede la directory. 0009_month_counts_trigger.
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
