//! App passwords: dedicated credentials for non-interactive clients
//! (`WebDAV`). Not the login password: name, last-used date, and revocation
//! are independent for each one, so a compromised `WebDAV` client doesn't
//! carry away access to the account.

use std::str::FromStr;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{DateTime, Utc};
use rand::Rng;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::UserId;

/// Opaque ID for an app password. `#[serde(transparent)]` so axum extracts
/// `Path<AppPasswordId>` from the same string-form `Uuid` that every other
/// id uses (see `crate::ids::id_type!`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AppPasswordId(Uuid);

impl AppPasswordId {
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }

    #[must_use]
    pub const fn from_uuid(id: Uuid) -> Self {
        Self(id)
    }

    #[must_use]
    pub const fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl Default for AppPasswordId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for AppPasswordId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for AppPasswordId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::from_str(s)?))
    }
}

/// The plaintext secret of an app password — exists only at creation time,
/// never saved to the database, never serialized afterward. 32 random bytes
/// encoded in base64url, same scheme as `SessionToken`/`ShareToken` in
/// `crate::token`.
pub struct AppPasswordSecret(String);

impl AppPasswordSecret {
    const SECRET_BYTES: usize = 32;

    #[must_use]
    pub fn generate() -> Self {
        let mut bytes = [0_u8; Self::SECRET_BYTES];
        rand::rng().fill_bytes(&mut bytes);
        Self(URL_SAFE_NO_PAD.encode(bytes))
    }

    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

// Prevents a secret from ending up in logs by accident, like `Password`.
impl std::fmt::Debug for AppPasswordSecret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AppPasswordSecret(***)")
    }
}

/// Public metadata for an app password — never the secret, never its hash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppPasswordSummary {
    pub id: AppPasswordId,
    pub user_id: UserId,
    pub label: String,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_secrets_are_unique() {
        assert_ne!(
            AppPasswordSecret::generate().expose(),
            AppPasswordSecret::generate().expose()
        );
    }

    #[test]
    fn generated_secret_is_long_enough_to_pass_the_password_length_floor() {
        // `Password::parse` requires at least 10 characters: a secret that's
        // too short could never be hashed by `hash_password`.
        assert!(AppPasswordSecret::generate().expose().chars().count() >= 10);
    }

    #[test]
    fn debug_does_not_leak_the_secret() {
        let secret = AppPasswordSecret::generate();
        let rendered = format!("{secret:?}");
        assert!(
            !rendered.contains(secret.expose()),
            "the secret must not end up in logs"
        );
    }

    #[test]
    fn ids_are_time_ordered() {
        let a = AppPasswordId::new();
        let b = AppPasswordId::new();
        assert!(a.as_uuid() < b.as_uuid(), "UUID v7 must be monotonic");
    }
}
