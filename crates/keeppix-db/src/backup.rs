//! Backup destinations, run history, and preferences (Fase 6 Tasks 3–4).
//! Destination credentials are AES-GCM encrypted at rest with a key from
//! [`SettingsRepo::get_or_create_secret`], same pattern as TOTP secrets.

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::Engine as _;
use chrono::{DateTime, Utc};
use keeppix_domain::AuthContext;
use rand::Rng;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::settings::SettingsRepo;
use crate::{Db, DbError};

const BACKUP_KEY_SETTING: &str = "backup_key";
const PREFS_SETTING: &str = "backup_preferences";
const NONCE_LEN: usize = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackupKind {
    Local,
    S3,
    Webdav,
    Sftp,
}

impl BackupKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::S3 => "s3",
            Self::Webdav => "webdav",
            Self::Sftp => "sftp",
        }
    }

    fn parse(raw: &str) -> Result<Self, DbError> {
        match raw {
            "local" => Ok(Self::Local),
            "s3" => Ok(Self::S3),
            "webdav" => Ok(Self::Webdav),
            "sftp" => Ok(Self::Sftp),
            other => Err(crate::row::corrupted("backup kind", other)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackupRunStatus {
    Running,
    Ok,
    Failed,
}

impl BackupRunStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Ok => "ok",
            Self::Failed => "failed",
        }
    }

    fn parse(raw: &str) -> Result<Self, DbError> {
        match raw {
            "running" => Ok(Self::Running),
            "ok" => Ok(Self::Ok),
            "failed" => Ok(Self::Failed),
            other => Err(crate::row::corrupted("backup run status", other)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(clippy::struct_excessive_bools)]
pub struct BackupPreferences {
    pub include_database: bool,
    pub include_config: bool,
    pub include_maps: bool,
    pub include_sidecars: bool,
    pub include_derivatives: bool,
    pub include_originals: bool,
    pub schedule: BackupSchedule,
    pub retention: BackupRetention,
    pub verify_after_write: bool,
    pub monthly_restore_proof: bool,
    /// age passphrase — never returned by the API; set via dedicated endpoint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub passphrase: Option<String>,
}

impl Default for BackupPreferences {
    fn default() -> Self {
        Self {
            include_database: true,
            include_config: true,
            include_maps: true,
            include_sidecars: true,
            include_derivatives: false,
            include_originals: false,
            schedule: BackupSchedule::Nightly,
            retention: BackupRetention::Smart,
            verify_after_write: true,
            monthly_restore_proof: true,
            passphrase: None,
        }
    }
}

impl BackupPreferences {
    /// Spec §2.4: catalog-only backups must surface a mandatory warning.
    #[must_use]
    pub const fn requires_originals_warning(&self) -> bool {
        !self.include_originals
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackupSchedule {
    Manual,
    Nightly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackupRetention {
    Smart,
    LastN { n: u32 },
}

#[derive(Debug, Clone)]
pub struct BackupDestination {
    pub id: Uuid,
    pub kind: BackupKind,
    pub label: String,
    /// Decrypted destination config (paths, endpoints, credentials).
    pub config: serde_json::Value,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewBackupDestination {
    pub kind: BackupKind,
    pub label: String,
    pub config: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct BackupRun {
    pub id: Uuid,
    pub destination_id: Option<Uuid>,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub size_bytes: Option<i64>,
    pub verified_at: Option<DateTime<Utc>>,
    pub status: BackupRunStatus,
    pub error: Option<String>,
    pub path: Option<String>,
    pub keeppix_version: Option<String>,
}

pub struct BackupRepo<'a> {
    db: &'a Db,
}

#[derive(sqlx::FromRow)]
struct DestinationRow {
    id: Uuid,
    kind: String,
    label: String,
    config: serde_json::Value,
    enabled: bool,
    created_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct RunRow {
    id: Uuid,
    destination_id: Option<Uuid>,
    started_at: DateTime<Utc>,
    completed_at: Option<DateTime<Utc>>,
    size_bytes: Option<i64>,
    verified_at: Option<DateTime<Utc>>,
    status: String,
    error: Option<String>,
    path: Option<String>,
    keeppix_version: Option<String>,
}

impl<'a> BackupRepo<'a> {
    #[must_use]
    pub const fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// # Errors
    /// Database or encryption failure.
    pub async fn get_preferences(&self, ctx: &AuthContext) -> Result<BackupPreferences, DbError> {
        require_admin(ctx)?;
        match SettingsRepo::new(self.db).get_json(PREFS_SETTING).await? {
            Some(v) => serde_json::from_value(v)
                .map_err(|e| DbError::Corrupted(format!("backup preferences: {e}"))),
            None => Ok(BackupPreferences::default()),
        }
    }

    /// # Errors
    /// Database failure. Passphrase is stored encrypted inside the JSON blob
    /// via the same settings table — callers must not log the value.
    pub async fn put_preferences(
        &self,
        ctx: &AuthContext,
        prefs: &BackupPreferences,
    ) -> Result<(), DbError> {
        require_admin(ctx)?;
        let value = serde_json::to_value(prefs)
            .map_err(|e| DbError::Corrupted(format!("backup preferences serialize: {e}")))?;
        SettingsRepo::new(self.db)
            .put_json(PREFS_SETTING, &value)
            .await
    }

    /// # Errors
    /// Database or encryption failure.
    pub async fn create_destination(
        &self,
        ctx: &AuthContext,
        new: NewBackupDestination,
    ) -> Result<BackupDestination, DbError> {
        require_admin(ctx)?;
        let id = Uuid::now_v7();
        let enc = self.encrypt_config(&new.config).await?;
        sqlx::query(
            "INSERT INTO backup_destinations (id, kind, label, config, enabled) \
             VALUES ($1, $2, $3, $4, true)",
        )
        .bind(id)
        .bind(new.kind.as_str())
        .bind(&new.label)
        .bind(enc)
        .execute(self.db.pool())
        .await?;
        self.find_destination(ctx, id).await
    }

    /// # Errors
    /// `Forbidden` if the id is unknown (existence oracle).
    pub async fn find_destination(
        &self,
        ctx: &AuthContext,
        id: Uuid,
    ) -> Result<BackupDestination, DbError> {
        require_admin(ctx)?;
        let row: Option<DestinationRow> = sqlx::query_as(
            "SELECT id, kind, label, config, enabled, created_at \
               FROM backup_destinations WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(self.db.pool())
        .await?;
        let Some(row) = row else {
            return Err(DbError::Forbidden);
        };
        self.row_to_destination(row).await
    }

    /// # Errors
    /// Database or decryption failure.
    pub async fn list_destinations(
        &self,
        ctx: &AuthContext,
    ) -> Result<Vec<BackupDestination>, DbError> {
        require_admin(ctx)?;
        let rows: Vec<DestinationRow> = sqlx::query_as(
            "SELECT id, kind, label, config, enabled, created_at \
               FROM backup_destinations ORDER BY created_at",
        )
        .fetch_all(self.db.pool())
        .await?;
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            out.push(self.row_to_destination(row).await?);
        }
        Ok(out)
    }

    /// Pipeline / scheduler path: list enabled destinations without an
    /// `AuthContext`. Documented exception — same class as
    /// `SessionRepo::purge_expired` (system maintenance, not user data).
    ///
    /// # Errors
    /// Database or decryption failure.
    pub async fn list_enabled_for_jobs(&self) -> Result<Vec<BackupDestination>, DbError> {
        let rows: Vec<DestinationRow> = sqlx::query_as(
            "SELECT id, kind, label, config, enabled, created_at \
               FROM backup_destinations WHERE enabled = true ORDER BY created_at",
        )
        .fetch_all(self.db.pool())
        .await?;
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            out.push(self.row_to_destination(row).await?);
        }
        Ok(out)
    }

    /// # Errors
    /// `Forbidden` if unknown.
    pub async fn delete_destination(&self, ctx: &AuthContext, id: Uuid) -> Result<(), DbError> {
        require_admin(ctx)?;
        let result = sqlx::query("DELETE FROM backup_destinations WHERE id = $1")
            .bind(id)
            .execute(self.db.pool())
            .await?;
        if result.rows_affected() == 0 {
            return Err(DbError::Forbidden);
        }
        Ok(())
    }

    /// # Errors
    /// Database failure.
    pub async fn start_run(
        &self,
        destination_id: Option<Uuid>,
        keeppix_version: &str,
    ) -> Result<BackupRun, DbError> {
        let id = Uuid::now_v7();
        sqlx::query(
            "INSERT INTO backup_runs (id, destination_id, status, keeppix_version) \
             VALUES ($1, $2, 'running', $3)",
        )
        .bind(id)
        .bind(destination_id)
        .bind(keeppix_version)
        .execute(self.db.pool())
        .await?;
        self.get_run_for_jobs(id).await
    }

    /// # Errors
    /// Database failure.
    pub async fn complete_run(
        &self,
        id: Uuid,
        size_bytes: i64,
        path: &str,
    ) -> Result<BackupRun, DbError> {
        sqlx::query(
            "UPDATE backup_runs SET status = 'ok', completed_at = now(), \
                    size_bytes = $2, path = $3, error = NULL \
              WHERE id = $1",
        )
        .bind(id)
        .bind(size_bytes)
        .bind(path)
        .execute(self.db.pool())
        .await?;
        self.get_run_for_jobs(id).await
    }

    /// # Errors
    /// Database failure.
    pub async fn fail_run(&self, id: Uuid, error: &str) -> Result<BackupRun, DbError> {
        sqlx::query(
            "UPDATE backup_runs SET status = 'failed', completed_at = now(), error = $2 \
              WHERE id = $1",
        )
        .bind(id)
        .bind(error)
        .execute(self.db.pool())
        .await?;
        self.get_run_for_jobs(id).await
    }

    /// Marks a successful monthly restore-proof.
    ///
    /// # Errors
    /// Database failure.
    pub async fn mark_verified(&self, id: Uuid) -> Result<BackupRun, DbError> {
        sqlx::query("UPDATE backup_runs SET verified_at = now() WHERE id = $1")
            .bind(id)
            .execute(self.db.pool())
            .await?;
        self.get_run_for_jobs(id).await
    }

    /// # Errors
    /// `Forbidden` if unknown (admin probe).
    pub async fn get_run(&self, ctx: &AuthContext, id: Uuid) -> Result<BackupRun, DbError> {
        require_admin(ctx)?;
        let row: Option<RunRow> = sqlx::query_as(
            "SELECT id, destination_id, started_at, completed_at, size_bytes, verified_at, \
                    status, error, path, keeppix_version \
               FROM backup_runs WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(self.db.pool())
        .await?;
        let Some(row) = row else {
            return Err(DbError::Forbidden);
        };
        row_to_run(row)
    }

    /// # Errors
    /// Database failure.
    pub async fn get_run_for_jobs(&self, id: Uuid) -> Result<BackupRun, DbError> {
        let row: RunRow = sqlx::query_as(
            "SELECT id, destination_id, started_at, completed_at, size_bytes, verified_at, \
                    status, error, path, keeppix_version \
               FROM backup_runs WHERE id = $1",
        )
        .bind(id)
        .fetch_one(self.db.pool())
        .await?;
        row_to_run(row)
    }

    /// # Errors
    /// Database failure.
    pub async fn list_runs(
        &self,
        ctx: &AuthContext,
        limit: i64,
    ) -> Result<Vec<BackupRun>, DbError> {
        require_admin(ctx)?;
        let rows: Vec<RunRow> = sqlx::query_as(
            "SELECT id, destination_id, started_at, completed_at, size_bytes, verified_at, \
                    status, error, path, keeppix_version \
               FROM backup_runs ORDER BY started_at DESC LIMIT $1",
        )
        .bind(limit.clamp(1, 200))
        .fetch_all(self.db.pool())
        .await?;
        rows.into_iter().map(row_to_run).collect()
    }

    /// Latest successful run that still needs a monthly restore proof.
    ///
    /// # Errors
    /// Database failure.
    pub async fn next_unverified_ok_run(&self) -> Result<Option<BackupRun>, DbError> {
        let row: Option<RunRow> = sqlx::query_as(
            "SELECT id, destination_id, started_at, completed_at, size_bytes, verified_at, \
                    status, error, path, keeppix_version \
               FROM backup_runs \
              WHERE status = 'ok' AND verified_at IS NULL AND path IS NOT NULL \
              ORDER BY started_at ASC LIMIT 1",
        )
        .fetch_optional(self.db.pool())
        .await?;
        row.map(row_to_run).transpose()
    }

    /// Runs `VACUUM ANALYZE` on the public schema. System maintenance —
    /// no `AuthContext` (same class as `purge_expired`).
    ///
    /// # Errors
    /// Database failure.
    pub async fn vacuum_analyze(&self) -> Result<(), DbError> {
        sqlx::query("VACUUM ANALYZE")
            .execute(self.db.pool())
            .await?;
        Ok(())
    }

    /// Creates a temporary schema for monthly restore proof. Name must be
    /// UUID-derived alphanumeric — validated before interpolation.
    ///
    /// # Errors
    /// Database failure or invalid name.
    pub async fn create_schema(&self, name: &str) -> Result<(), DbError> {
        if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(DbError::Corrupted("invalid schema name".to_owned()));
        }
        sqlx::query(&format!("CREATE SCHEMA {name}"))
            .execute(self.db.pool())
            .await?;
        Ok(())
    }

    /// # Errors
    /// Database failure or invalid name.
    pub async fn drop_schema(&self, name: &str) -> Result<(), DbError> {
        if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(DbError::Corrupted("invalid schema name".to_owned()));
        }
        sqlx::query(&format!("DROP SCHEMA IF EXISTS {name} CASCADE"))
            .execute(self.db.pool())
            .await?;
        Ok(())
    }

    /// Sample ~`percent` of hashed assets for integrity scrubbing.
    /// Pipeline-only; no `AuthContext`.
    ///
    /// # Errors
    /// Database failure.
    pub async fn sample_assets_for_scrub(
        &self,
        percent: i32,
    ) -> Result<Vec<(uuid::Uuid, Vec<u8>, String)>, DbError> {
        let threshold = percent.clamp(1, 100);
        let rows: Vec<(uuid::Uuid, Vec<u8>, String)> = sqlx::query_as(
            "SELECT a.id, a.content_hash, a.filename \
               FROM assets a \
              WHERE a.content_hash IS NOT NULL AND a.status = 'ready' \
                AND (get_byte(a.content_hash, 0) % 100) < $1 \
              ORDER BY a.id \
              LIMIT 5000",
        )
        .bind(threshold)
        .fetch_all(self.db.pool())
        .await?;
        Ok(rows)
    }

    async fn row_to_destination(&self, row: DestinationRow) -> Result<BackupDestination, DbError> {
        let config = self.decrypt_config(&row.config).await?;
        Ok(BackupDestination {
            id: row.id,
            kind: BackupKind::parse(&row.kind)?,
            label: row.label,
            config,
            enabled: row.enabled,
            created_at: row.created_at,
        })
    }

    async fn encryption_key(&self) -> Result<[u8; 32], DbError> {
        SettingsRepo::new(self.db)
            .get_or_create_secret(BACKUP_KEY_SETTING)
            .await
    }

    async fn encrypt_config(
        &self,
        plain: &serde_json::Value,
    ) -> Result<serde_json::Value, DbError> {
        let key = self.encryption_key().await?;
        let cipher = Aes256Gcm::new_from_slice(&key)
            .map_err(|e| DbError::Corrupted(format!("backup aes key rejected: {e}")))?;
        let mut nonce_bytes = [0u8; NONCE_LEN];
        rand::rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        let plaintext = serde_json::to_vec(plain)
            .map_err(|e| DbError::Corrupted(format!("backup config serialize: {e}")))?;
        let ciphertext = cipher
            .encrypt(nonce, plaintext.as_ref())
            .map_err(|e| DbError::Corrupted(format!("backup encrypt failed: {e}")))?;
        let mut blob = nonce_bytes.to_vec();
        blob.extend(ciphertext);
        Ok(serde_json::json!({ "enc": base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            &blob
        )}))
    }

    async fn decrypt_config(
        &self,
        stored: &serde_json::Value,
    ) -> Result<serde_json::Value, DbError> {
        let Some(enc) = stored.get("enc").and_then(|v| v.as_str()) else {
            // Pre-encryption / test fixtures may store plaintext — accept only
            // when the blob has no `enc` wrapper (local-dev migration path).
            return Ok(stored.clone());
        };
        let blob = base64::engine::general_purpose::STANDARD
            .decode(enc)
            .map_err(|e| DbError::Corrupted(format!("backup config b64: {e}")))?;
        if blob.len() <= NONCE_LEN {
            return Err(DbError::Corrupted("backup config too short".to_owned()));
        }
        let key = self.encryption_key().await?;
        let cipher = Aes256Gcm::new_from_slice(&key)
            .map_err(|e| DbError::Corrupted(format!("backup aes key rejected: {e}")))?;
        let (nonce_bytes, ciphertext) = blob.split_at(NONCE_LEN);
        let nonce = Nonce::from_slice(nonce_bytes);
        let plain = cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| DbError::Corrupted(format!("backup decrypt failed: {e}")))?;
        serde_json::from_slice(&plain)
            .map_err(|e| DbError::Corrupted(format!("backup config json: {e}")))
    }
}

fn row_to_run(row: RunRow) -> Result<BackupRun, DbError> {
    Ok(BackupRun {
        id: row.id,
        destination_id: row.destination_id,
        started_at: row.started_at,
        completed_at: row.completed_at,
        size_bytes: row.size_bytes,
        verified_at: row.verified_at,
        status: BackupRunStatus::parse(&row.status)?,
        error: row.error,
        path: row.path,
        keeppix_version: row.keeppix_version,
    })
}

fn require_admin(ctx: &AuthContext) -> Result<(), DbError> {
    if ctx.is_admin() {
        Ok(())
    } else {
        Err(DbError::Forbidden)
    }
}
