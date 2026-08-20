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
    /// L'operazione sul filesystem che accompagna la scrittura sul database
    /// (`rename()` nel cestino, cancellazione da disco) è fallita. Distinta
    /// da `Connection`: qui il database non ha nulla a che fare, è il
    /// percorso che non risponde — disco pieno, permessi, mount caduto.
    #[error("filesystem error: {0}")]
    Io(String),
    /// Spazio su disco insufficiente per `expected_size` alla creazione di
    /// una sessione di upload: rifiutata subito, non scoperta a metà.
    #[error("insufficient storage")]
    InsufficientStorage,
    /// La risorsa esisteva ma non è più utilizzabile — una sessione di
    /// upload scaduta. Distinta da `NotFound`: il chiamante l'aveva vista,
    /// non ha sbagliato id.
    #[error("gone")]
    Gone,
}

impl From<sqlx::migrate::MigrateError> for DbError {
    fn from(e: sqlx::migrate::MigrateError) -> Self {
        Self::Migration(e.to_string())
    }
}
