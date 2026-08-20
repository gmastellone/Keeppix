//! TOTP (RFC 6238) secrets and recovery codes. Secrets are encrypted at rest
//! with AES-256-GCM using a key from [`SettingsRepo::get_or_create_secret`].

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use chrono::{DateTime, Utc};
use data_encoding::BASE32_NOPAD;
use hmac::{Hmac, Mac};
use keeppix_domain::{AuthContext, UserId};
use rand::Rng as _;
use sha1::Sha1;
use uuid::Uuid;

use crate::settings::SettingsRepo;
use crate::{Db, DbError};

const TOTP_KEY_SETTING: &str = "totp_key";
const TOTP_DIGITS: u32 = 1_000_000;
const TOTP_STEP_SECS: i64 = 30;
const TOTP_SECRET_LEN: usize = 20;
const RECOVERY_CODE_COUNT: usize = 10;
const NONCE_LEN: usize = 12;

type HmacSha1 = Hmac<Sha1>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TotpStatus {
    pub enabled: bool,
    pub pending: bool,
    pub unused_recovery_codes: i64,
}

#[derive(Debug, Clone)]
pub struct TotpSetup {
    /// Raw secret bytes — returned once so callers can build the otpauth URI
    /// / QR; never persisted in plaintext.
    pub secret_raw: Vec<u8>,
    pub secret_base32: String,
    pub otpauth_uri: String,
}

#[derive(Debug, Clone)]
pub struct TotpConfirmed {
    pub recovery_codes: Vec<String>,
}

#[derive(sqlx::FromRow)]
struct SecretRow {
    secret_enc: Vec<u8>,
    enabled_at: Option<DateTime<Utc>>,
    last_used_step: Option<i64>,
}

pub struct TotpRepo<'a> {
    db: &'a Db,
}

impl<'a> TotpRepo<'a> {
    #[must_use]
    pub const fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// # Errors
    /// `Forbidden` if the caller is not an authenticated user.
    pub async fn status(&self, ctx: &AuthContext) -> Result<TotpStatus, DbError> {
        let user_id = ctx.user_id().ok_or(DbError::Forbidden)?;
        let row: Option<SecretRow> = sqlx::query_as(
            "SELECT secret_enc, enabled_at, last_used_step FROM totp_secrets WHERE user_id = $1",
        )
        .bind(user_id.as_uuid())
        .fetch_optional(self.db.pool())
        .await?;

        let unused: i64 = sqlx::query_scalar(
            "SELECT count(*)::bigint FROM totp_recovery_codes \
              WHERE user_id = $1 AND used_at IS NULL",
        )
        .bind(user_id.as_uuid())
        .fetch_one(self.db.pool())
        .await?;

        Ok(match row {
            None => TotpStatus {
                enabled: false,
                pending: false,
                unused_recovery_codes: 0,
            },
            Some(r) => TotpStatus {
                enabled: r.enabled_at.is_some(),
                pending: r.enabled_at.is_none(),
                unused_recovery_codes: unused,
            },
        })
    }

    /// # Errors
    /// `Forbidden` if unauthenticated; `Conflict` if TOTP is already enabled.
    pub async fn begin_setup(
        &self,
        ctx: &AuthContext,
        username: &str,
    ) -> Result<TotpSetup, DbError> {
        let user_id = ctx.user_id().ok_or(DbError::Forbidden)?;

        let enabled: Option<DateTime<Utc>> =
            sqlx::query_scalar("SELECT enabled_at FROM totp_secrets WHERE user_id = $1")
                .bind(user_id.as_uuid())
                .fetch_optional(self.db.pool())
                .await?
                .flatten();
        if enabled.is_some() {
            return Err(DbError::Conflict(
                "TOTP is already enabled; disable it before starting setup again".to_owned(),
            ));
        }

        let mut secret = vec![0u8; TOTP_SECRET_LEN];
        rand::rng().fill_bytes(&mut secret);
        let secret_enc = self.encrypt(&secret).await?;
        let secret_base32 = BASE32_NOPAD.encode(&secret);
        let otpauth_uri = format!(
            "otpauth://totp/Keeppix:{account}?secret={secret}&issuer=Keeppix&algorithm=SHA1&digits=6&period=30",
            account = urlencoding_username(username),
            secret = secret_base32,
        );

        sqlx::query(
            "INSERT INTO totp_secrets (user_id, secret_enc, enabled_at, last_used_step) \
             VALUES ($1, $2, NULL, NULL) \
             ON CONFLICT (user_id) DO UPDATE \
             SET secret_enc = EXCLUDED.secret_enc, \
                 enabled_at = NULL, \
                 last_used_step = NULL, \
                 created_at = now()",
        )
        .bind(user_id.as_uuid())
        .bind(&secret_enc)
        .execute(self.db.pool())
        .await?;

        // Discard any leftover recovery codes from a previous enrolment.
        sqlx::query("DELETE FROM totp_recovery_codes WHERE user_id = $1")
            .bind(user_id.as_uuid())
            .execute(self.db.pool())
            .await?;

        Ok(TotpSetup {
            secret_raw: secret,
            secret_base32,
            otpauth_uri,
        })
    }

    /// Confirms provisioning with the first authenticator code and enables
    /// TOTP. Does **not** revoke sessions — the current session must survive.
    ///
    /// # Errors
    /// `Forbidden` if unauthenticated; `Conflict` if nothing is pending;
    /// `NotFound`-mapped refusal via `Conflict` when the code is wrong.
    pub async fn confirm(&self, ctx: &AuthContext, code: &str) -> Result<TotpConfirmed, DbError> {
        let user_id = ctx.user_id().ok_or(DbError::Forbidden)?;
        let row: Option<SecretRow> = sqlx::query_as(
            "SELECT secret_enc, enabled_at, last_used_step FROM totp_secrets WHERE user_id = $1",
        )
        .bind(user_id.as_uuid())
        .fetch_optional(self.db.pool())
        .await?;

        let Some(row) = row else {
            return Err(DbError::Conflict("no TOTP setup in progress".to_owned()));
        };
        if row.enabled_at.is_some() {
            return Err(DbError::Conflict("TOTP is already enabled".to_owned()));
        }

        let secret = self.decrypt(&row.secret_enc).await?;
        let Some(step) = matching_step(&secret, code, row.last_used_step)? else {
            return Err(DbError::Conflict("invalid TOTP code".to_owned()));
        };

        let recovery_codes = generate_recovery_codes();
        let key = self.encryption_key().await?;

        let mut tx = self.db.pool().begin().await?;
        sqlx::query(
            "UPDATE totp_secrets \
                SET enabled_at = now(), last_used_step = $2 \
              WHERE user_id = $1 AND enabled_at IS NULL",
        )
        .bind(user_id.as_uuid())
        .bind(step)
        .execute(&mut *tx)
        .await?;

        sqlx::query("DELETE FROM totp_recovery_codes WHERE user_id = $1")
            .bind(user_id.as_uuid())
            .execute(&mut *tx)
            .await?;

        for plain in &recovery_codes {
            let hash = hash_recovery_code(&key, &normalize_recovery_code(plain));
            sqlx::query(
                "INSERT INTO totp_recovery_codes (id, user_id, code_hash) VALUES ($1, $2, $3)",
            )
            .bind(Uuid::now_v7())
            .bind(user_id.as_uuid())
            .bind(&hash)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;

        Ok(TotpConfirmed { recovery_codes })
    }

    /// # Errors
    /// `Forbidden` if unauthenticated.
    pub async fn is_enabled(&self, ctx: &AuthContext) -> Result<bool, DbError> {
        Ok(self.status(ctx).await?.enabled)
    }

    /// Pre-session check used by login. Documented exception to the
    /// `AuthContext`-first rule: there is no session yet, same class as
    /// `UserRepo::find_by_username` / `AppPasswordRepo::verify`.
    ///
    /// # Errors
    /// `Connection` on query failure.
    pub async fn is_enabled_for_user(&self, user_id: UserId) -> Result<bool, DbError> {
        let enabled: Option<DateTime<Utc>> =
            sqlx::query_scalar("SELECT enabled_at FROM totp_secrets WHERE user_id = $1")
                .bind(user_id.as_uuid())
                .fetch_optional(self.db.pool())
                .await?
                .flatten();
        Ok(enabled.is_some())
    }

    /// Login-path verification: TOTP code or unused recovery code.
    ///
    /// # Errors
    /// `Connection` / `Corrupted` on storage problems.
    pub async fn verify_login(&self, user_id: UserId, code: &str) -> Result<bool, DbError> {
        if looks_like_totp_digits(code) && self.verify_for_user(user_id, code).await? {
            return Ok(true);
        }
        self.consume_recovery_for_user(user_id, code).await
    }

    /// # Errors
    /// `Forbidden` if unauthenticated; `Conflict` if TOTP is not enabled or
    /// the authenticator code is wrong.
    pub async fn regenerate_recovery_codes(
        &self,
        ctx: &AuthContext,
        totp_code: &str,
    ) -> Result<Vec<String>, DbError> {
        let user_id = ctx.user_id().ok_or(DbError::Forbidden)?;
        if !self.is_enabled(ctx).await? {
            return Err(DbError::Conflict("TOTP is not enabled".to_owned()));
        }
        if !self.verify_for_user(user_id, totp_code).await? {
            return Err(DbError::Conflict("invalid TOTP code".to_owned()));
        }

        let recovery_codes = generate_recovery_codes();
        let key = self.encryption_key().await?;
        let mut tx = self.db.pool().begin().await?;
        sqlx::query("DELETE FROM totp_recovery_codes WHERE user_id = $1")
            .bind(user_id.as_uuid())
            .execute(&mut *tx)
            .await?;
        for plain in &recovery_codes {
            let hash = hash_recovery_code(&key, &normalize_recovery_code(plain));
            sqlx::query(
                "INSERT INTO totp_recovery_codes (id, user_id, code_hash) VALUES ($1, $2, $3)",
            )
            .bind(Uuid::now_v7())
            .bind(user_id.as_uuid())
            .bind(&hash)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(recovery_codes)
    }

    /// Disables TOTP after a valid authenticator or recovery code.
    ///
    /// # Errors
    /// `Forbidden` if unauthenticated; `Conflict` if not enabled or code bad.
    pub async fn disable(&self, ctx: &AuthContext, code: &str) -> Result<(), DbError> {
        let user_id = ctx.user_id().ok_or(DbError::Forbidden)?;
        if !self.is_enabled(ctx).await? {
            return Err(DbError::Conflict("TOTP is not enabled".to_owned()));
        }
        let ok = self.verify_login(user_id, code).await?;
        if !ok {
            return Err(DbError::Conflict(
                "invalid TOTP or recovery code".to_owned(),
            ));
        }
        let mut tx = self.db.pool().begin().await?;
        sqlx::query("DELETE FROM totp_recovery_codes WHERE user_id = $1")
            .bind(user_id.as_uuid())
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM totp_secrets WHERE user_id = $1")
            .bind(user_id.as_uuid())
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    async fn verify_for_user(&self, user_id: UserId, code: &str) -> Result<bool, DbError> {
        let row: Option<SecretRow> = sqlx::query_as(
            "SELECT secret_enc, enabled_at, last_used_step FROM totp_secrets WHERE user_id = $1",
        )
        .bind(user_id.as_uuid())
        .fetch_optional(self.db.pool())
        .await?;

        let Some(row) = row else {
            return Ok(false);
        };
        if row.enabled_at.is_none() {
            return Ok(false);
        }

        let secret = self.decrypt(&row.secret_enc).await?;
        let Some(step) = matching_step(&secret, code, row.last_used_step)? else {
            return Ok(false);
        };

        let updated = sqlx::query(
            "UPDATE totp_secrets SET last_used_step = $2 \
              WHERE user_id = $1 \
                AND enabled_at IS NOT NULL \
                AND (last_used_step IS DISTINCT FROM $2)",
        )
        .bind(user_id.as_uuid())
        .bind(step)
        .execute(self.db.pool())
        .await?
        .rows_affected();

        Ok(updated == 1)
    }

    async fn consume_recovery_for_user(
        &self,
        user_id: UserId,
        code: &str,
    ) -> Result<bool, DbError> {
        let key = self.encryption_key().await?;
        let normalized = normalize_recovery_code(code);
        if normalized.is_empty() {
            return Ok(false);
        }
        let want = hash_recovery_code(&key, &normalized);

        let rows: Vec<(Uuid, String)> = sqlx::query_as(
            "SELECT id, code_hash FROM totp_recovery_codes \
              WHERE user_id = $1 AND used_at IS NULL",
        )
        .bind(user_id.as_uuid())
        .fetch_all(self.db.pool())
        .await?;

        for (id, hash) in rows {
            if hash == want {
                let updated = sqlx::query(
                    "UPDATE totp_recovery_codes SET used_at = now() \
                      WHERE id = $1 AND used_at IS NULL",
                )
                .bind(id)
                .execute(self.db.pool())
                .await?
                .rows_affected();
                return Ok(updated == 1);
            }
        }
        Ok(false)
    }

    async fn encryption_key(&self) -> Result<[u8; 32], DbError> {
        SettingsRepo::new(self.db)
            .get_or_create_secret(TOTP_KEY_SETTING)
            .await
    }

    async fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, DbError> {
        let key = self.encryption_key().await?;
        let cipher = Aes256Gcm::new_from_slice(&key)
            .map_err(|e| DbError::Corrupted(format!("totp aes key rejected: {e}")))?;
        let mut nonce_bytes = [0u8; NONCE_LEN];
        rand::rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        let mut out = nonce_bytes.to_vec();
        let ciphertext = cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| DbError::Corrupted(format!("totp encrypt failed: {e}")))?;
        out.extend(ciphertext);
        Ok(out)
    }

    async fn decrypt(&self, blob: &[u8]) -> Result<Vec<u8>, DbError> {
        if blob.len() <= NONCE_LEN {
            return Err(DbError::Corrupted("totp secret_enc too short".to_owned()));
        }
        let key = self.encryption_key().await?;
        let cipher = Aes256Gcm::new_from_slice(&key)
            .map_err(|e| DbError::Corrupted(format!("totp aes key rejected: {e}")))?;
        let (nonce_bytes, ciphertext) = blob.split_at(NONCE_LEN);
        let nonce = Nonce::from_slice(nonce_bytes);
        cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| DbError::Corrupted(format!("totp decrypt failed: {e}")))
    }
}

fn current_step() -> i64 {
    Utc::now().timestamp() / TOTP_STEP_SECS
}

fn hotp(secret: &[u8], step: i64) -> Result<u32, DbError> {
    let mut mac = <HmacSha1 as Mac>::new_from_slice(secret)
        .map_err(|e| DbError::Corrupted(format!("hmac key: {e}")))?;
    let counter = u64::try_from(step.max(0)).unwrap_or(0);
    mac.update(&counter.to_be_bytes());
    let result = mac.finalize().into_bytes();
    let offset = (result[19] & 0x0f) as usize;
    let bin = ((u32::from(result[offset]) & 0x7f) << 24)
        | ((u32::from(result[offset + 1]) & 0xff) << 16)
        | ((u32::from(result[offset + 2]) & 0xff) << 8)
        | (u32::from(result[offset + 3]) & 0xff);
    Ok(bin % TOTP_DIGITS)
}

fn matching_step(
    secret: &[u8],
    code: &str,
    last_used: Option<i64>,
) -> Result<Option<i64>, DbError> {
    let trimmed = code.trim();
    if trimmed.len() != 6 || !trimmed.chars().all(|c| c.is_ascii_digit()) {
        return Ok(None);
    }
    let now = current_step();
    for delta in [-1_i64, 0, 1] {
        let step = now + delta;
        if last_used == Some(step) {
            continue;
        }
        let expected = format!("{:06}", hotp(secret, step)?);
        if expected == trimmed {
            return Ok(Some(step));
        }
    }
    Ok(None)
}

fn generate_recovery_codes() -> Vec<String> {
    let mut codes = Vec::with_capacity(RECOVERY_CODE_COUNT);
    for _ in 0..RECOVERY_CODE_COUNT {
        let mut bytes = [0u8; 10];
        rand::rng().fill_bytes(&mut bytes);
        let encoded = BASE32_NOPAD.encode(&bytes);
        // 16 chars → XXXX-XXXX-XXXX-XXXX
        let formatted = format!(
            "{}-{}-{}-{}",
            &encoded[0..4],
            &encoded[4..8],
            &encoded[8..12],
            &encoded[12..16]
        );
        codes.push(formatted);
    }
    codes
}

fn normalize_recovery_code(raw: &str) -> String {
    raw.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_uppercase())
        .collect()
}

fn hash_recovery_code(key: &[u8; 32], normalized: &str) -> String {
    let hash = blake3::keyed_hash(key, normalized.as_bytes());
    hash.to_hex().to_string()
}

fn looks_like_totp_digits(code: &str) -> bool {
    let t = code.trim();
    t.len() == 6 && t.chars().all(|c| c.is_ascii_digit())
}

fn urlencoding_username(username: &str) -> String {
    // Username is already domain-validated ([a-z0-9_-]); percent-encode only
    // what otpauth URIs require for safety.
    let mut out = String::with_capacity(username.len());
    for b in username.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' => {
                out.push(char::from(b));
            }
            _ => {
                use std::fmt::Write as _;
                let _ = write!(out, "%{b:02X}");
            }
        }
    }
    out
}
