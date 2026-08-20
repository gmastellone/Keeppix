use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use rand::Rng;

use crate::{Db, DbError};

pub struct SettingsRepo<'a> {
    db: &'a Db,
}

#[derive(sqlx::FromRow)]
struct SecretRow {
    value: String,
}

impl SecretRow {
    fn into_domain(self) -> Result<[u8; 32], DbError> {
        let bytes = STANDARD
            .decode(&self.value)
            .map_err(|e| DbError::Corrupted(format!("stored secret is not base64: {e}")))?;
        bytes
            .try_into()
            .map_err(|_| DbError::Corrupted("stored secret is not 32 bytes".to_owned()))
    }
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

        let row: SecretRow =
            sqlx::query_as("SELECT value #>> '{}' AS value FROM system_settings WHERE key = $1")
                .bind(key)
                .fetch_one(self.db.pool())
                .await?;

        row.into_domain()
    }

    /// # Errors
    /// `Connection` se la query fallisce.
    pub async fn put_json(&self, key: &str, value: &serde_json::Value) -> Result<(), DbError> {
        sqlx::query(
            "INSERT INTO system_settings (key, value) VALUES ($1, $2) \
             ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = now()",
        )
        .bind(key)
        .bind(value)
        .execute(self.db.pool())
        .await?;
        self.db.invalidate_setting_cache(key).await;
        Ok(())
    }

    /// # Errors
    /// `Connection` se la query fallisce.
    pub async fn get_json(&self, key: &str) -> Result<Option<serde_json::Value>, DbError> {
        if let Some(cached) = self.db.settings_cache().get(key).await {
            return Ok(cached);
        }

        let v: Option<serde_json::Value> =
            sqlx::query_scalar("SELECT value FROM system_settings WHERE key = $1")
                .bind(key)
                .fetch_optional(self.db.pool())
                .await?;
        self.db
            .settings_cache()
            .insert(key.to_owned(), v.clone())
            .await;
        Ok(v)
    }
}
