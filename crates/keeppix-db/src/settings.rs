use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use rand::Rng;
use sqlx::Row;

use crate::{Db, DbError};

pub struct SettingsRepo<'a> {
    db: &'a Db,
}

impl<'a> SettingsRepo<'a> {
    #[must_use]
    pub const fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// Restituisce il segreto associato alla chiave, generandolo al primo
    /// accesso. `ON CONFLICT DO NOTHING` più rilettura rende l'operazione
    /// sicura anche se due processi partono insieme.
    ///
    /// # Errors
    /// `DbError::Connection` se la query fallisce; `DbError::Corrupted` se il
    /// valore memorizzato non è decodificabile.
    pub async fn get_or_create_secret(&self, key: &str) -> Result<[u8; 32], DbError> {
        let mut fresh = [0u8; 32];
        rand::rng().fill_bytes(&mut fresh);
        let encoded = STANDARD.encode(fresh);

        sqlx::query(
            "INSERT INTO system_settings (key, value) VALUES ($1, to_jsonb($2::text)) \
             ON CONFLICT (key) DO NOTHING",
        )
        .bind(key)
        .bind(&encoded)
        .execute(self.db.pool())
        .await?;

        let row = sqlx::query("SELECT value #>> '{}' AS value FROM system_settings WHERE key = $1")
            .bind(key)
            .fetch_one(self.db.pool())
            .await?;

        let stored: String = row.try_get("value")?;
        let bytes = STANDARD
            .decode(&stored)
            .map_err(|e| DbError::Corrupted(format!("stored secret is not base64: {e}")))?;

        bytes
            .try_into()
            .map_err(|_| DbError::Corrupted("stored secret is not 32 bytes".to_owned()))
    }
}
