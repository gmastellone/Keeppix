use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::DomainError;
use crate::ids::UserId;

const USERNAME_MIN: usize = 3;
const USERNAME_MAX: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Username(String);

impl Username {
    /// Normalizza in minuscolo e valida lunghezza e alfabeto consentito.
    ///
    /// # Errors
    /// Restituisce `DomainError::InvalidUsername` se fuori lunghezza o con
    /// caratteri non ammessi.
    pub fn parse(raw: &str) -> Result<Self, DomainError> {
        let normalised = raw.trim().to_lowercase();

        if normalised.len() < USERNAME_MIN || normalised.len() > USERNAME_MAX {
            return Err(DomainError::InvalidUsername(format!(
                "must be between {USERNAME_MIN} and {USERNAME_MAX} characters"
            )));
        }

        if !normalised
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-'))
        {
            return Err(DomainError::InvalidUsername(
                "only a-z, 0-9, dot, underscore and hyphen are allowed".to_owned(),
            ));
        }

        Ok(Self(normalised))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SystemRole {
    Admin,
    User,
}

impl SystemRole {
    #[must_use]
    pub const fn is_admin(self) -> bool {
        matches!(self, Self::Admin)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct User {
    pub id: UserId,
    pub username: Username,
    pub email: Option<String>,
    pub display_name: String,
    pub role: SystemRole,
    pub locale: Option<String>,
    pub created_at: DateTime<Utc>,
    pub disabled_at: Option<DateTime<Utc>>,
}

impl User {
    #[must_use]
    pub const fn is_active(&self) -> bool {
        self.disabled_at.is_none()
    }
}

/// Dati necessari a creare un utente. La password arriva già come hash:
/// il dominio non conosce l'algoritmo.
#[derive(Debug, Clone)]
pub struct NewUser {
    pub username: Username,
    pub email: Option<String>,
    pub display_name: String,
    pub password_hash: String,
    pub role: SystemRole,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(clippy::unwrap_used)]
    fn username_is_normalised_to_lowercase() {
        let u = Username::parse("Giovanni").unwrap();
        assert_eq!(u.as_str(), "giovanni");
    }

    #[test]
    fn username_rejects_too_short() {
        assert!(Username::parse("ab").is_err());
    }

    #[test]
    fn username_rejects_invalid_characters() {
        assert!(Username::parse("gio vanni").is_err());
        assert!(Username::parse("gio@vanni").is_err());
    }

    #[test]
    fn username_accepts_allowed_punctuation() {
        assert!(Username::parse("gio.mastellone_94-x").is_ok());
    }
}
