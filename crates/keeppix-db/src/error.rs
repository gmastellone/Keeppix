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
    #[error("corrupted row: {0}")]
    Corrupted(String),
    /// The filesystem operation accompanying a database write (`rename()`
    /// into trash, deletion from disk) failed. Distinct from `Connection`:
    /// the database has nothing to do with it here, it's the filesystem
    /// path that isn't responding — full disk, permissions, a dropped
    /// mount.
    #[error("filesystem error: {0}")]
    Io(String),
    /// Insufficient disk space for `expected_size` when creating an upload
    /// session: rejected immediately, not discovered halfway through.
    #[error("insufficient storage")]
    InsufficientStorage,
    /// The resource existed but is no longer usable — an expired upload
    /// session. Distinct from `NotFound`: the caller had seen it, they
    /// did not get the id wrong.
    #[error("gone")]
    Gone,
    /// Destination `(folder_id, filename)` already occupied by another
    /// asset — `AssetRepo::move_asset`. Distinct from `Conflict`: a bulk
    /// operation must be able to recognize it without parsing the message
    /// text (`FailureReason::Collision`, `crates/keeppix-api/src/bulk.rs`)
    /// — "never overwrite" is structured information, not a detail for
    /// humans to read.
    #[error("destination already occupied: {0}")]
    Collision(String),
}

impl From<sqlx::migrate::MigrateError> for DbError {
    fn from(e: sqlx::migrate::MigrateError) -> Self {
        Self::Migration(e.to_string())
    }
}
