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
    /// `(folder_id, filename)` di destinazione già occupato da un altro
    /// asset — `AssetRepo::move_asset` (Fase 9 Task 1). Distinta da
    /// `Conflict`: un'operazione di massa deve poterla riconoscere senza
    /// analizzare il testo del messaggio (`FailureReason::Collision`,
    /// `crates/keeppix-api/src/bulk.rs`) — "non si sovrascrive mai" è
    /// un'informazione strutturata, non un dettaglio per umani.
    #[error("destination already occupied: {0}")]
    Collision(String),
}

impl From<sqlx::migrate::MigrateError> for DbError {
    fn from(e: sqlx::migrate::MigrateError) -> Self {
        Self::Migration(e.to_string())
    }
}
