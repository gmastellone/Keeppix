use thiserror::Error;

#[derive(Debug, Error)]
pub enum DbError {
    #[error("database error: {0}")]
    Connection(#[from] sqlx::Error),
    #[error("migration failed: {0}")]
    Migration(String),
    #[error("not found")]
    NotFound,
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("forbidden")]
    Forbidden,
}

impl From<sqlx::migrate::MigrateError> for DbError {
    fn from(e: sqlx::migrate::MigrateError) -> Self {
        Self::Migration(e.to_string())
    }
}
