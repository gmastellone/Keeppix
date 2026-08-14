## Task 2: Tipi di dominio

**Files:**
- Create: `crates/keeppix-domain/src/ids.rs`
- Create: `crates/keeppix-domain/src/user.rs`
- Create: `crates/keeppix-domain/src/auth.rs`
- Create: `crates/keeppix-domain/src/error.rs`
- Modify: `crates/keeppix-domain/src/lib.rs`
- Modify: `crates/keeppix-domain/Cargo.toml`

**Interfaces:**
- Consumes: nulla.
- Produces:
  - `UserId(Uuid)` e `GroupId(Uuid)` con `UserId::new() -> Self` (UUID v7), `Display`, `FromStr`, `Serialize`/`Deserialize`.
  - `SystemRole::{Admin, User}` con `is_admin(&self) -> bool`.
  - `User { id: UserId, username: String, email: Option<String>, display_name: String, role: SystemRole, locale: Option<String>, created_at: DateTime<Utc>, disabled_at: Option<DateTime<Utc>> }` con `User::is_active(&self) -> bool`.
  - `Username::parse(&str) -> Result<Username, DomainError>` — normalizza in minuscolo, 3-32 caratteri, `[a-z0-9._-]`.
  - `Actor::{User { id: UserId, role: SystemRole }, ShareLink { .. }}` e `AuthContext { actor: Actor }` con `AuthContext::user_id(&self) -> Option<UserId>` e `is_admin(&self) -> bool`.
  - `DomainError::{InvalidUsername(String), InvalidPassword(String)}`.

- [ ] **Step 1: Aggiungere le dipendenze**

In `crates/keeppix-domain/Cargo.toml`:

```toml
[dependencies]
thiserror.workspace = true
serde.workspace = true
uuid.workspace = true
chrono.workspace = true
```

- [ ] **Step 2: Scrivere il test che fallisce per `Username`**

`crates/keeppix-domain/src/user.rs`, in fondo:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
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
```

- [ ] **Step 3: Eseguire il test e verificare che fallisca**

Run: `cargo test -p keeppix-domain`
Expected: FAIL — `cannot find type Username in this scope`.

- [ ] **Step 4: Scrivere `error.rs`**

```rust
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum DomainError {
    #[error("invalid username: {0}")]
    InvalidUsername(String),
    #[error("invalid password: {0}")]
    InvalidPassword(String),
}
```

- [ ] **Step 5: Scrivere `ids.rs`**

```rust
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

macro_rules! id_type {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
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

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl FromStr for $name {
            type Err = uuid::Error;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Ok(Self(Uuid::from_str(s)?))
            }
        }
    };
}

id_type!(UserId);
id_type!(GroupId);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_time_ordered() {
        let a = UserId::new();
        let b = UserId::new();
        assert!(a.as_uuid() < b.as_uuid(), "UUID v7 must be monotonic");
    }

    #[test]
    fn id_roundtrips_through_string() {
        let a = UserId::new();
        assert_eq!(a, a.to_string().parse().unwrap());
    }
}
```

- [ ] **Step 6: Scrivere `user.rs` (sopra il blocco di test già presente)**

```rust
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
```

- [ ] **Step 7: Scrivere `auth.rs`**

```rust
use crate::ids::UserId;
use crate::user::SystemRole;

/// Chi sta effettuando la richiesta. In Fase 0 esiste solo `User`;
/// `ShareLink` arriva in Fase 3 e passerà per lo stesso `AuthContext`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Actor {
    User { id: UserId, role: SystemRole },
}

/// Contesto richiesto da ogni repository che legge dati di un utente.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthContext {
    pub actor: Actor,
}

impl AuthContext {
    #[must_use]
    pub const fn user(id: UserId, role: SystemRole) -> Self {
        Self { actor: Actor::User { id, role } }
    }

    #[must_use]
    pub const fn user_id(&self) -> Option<UserId> {
        match self.actor {
            Actor::User { id, .. } => Some(id),
        }
    }

    #[must_use]
    pub const fn is_admin(&self) -> bool {
        match self.actor {
            Actor::User { role, .. } => role.is_admin(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admin_context_reports_admin() {
        let ctx = AuthContext::user(UserId::new(), SystemRole::Admin);
        assert!(ctx.is_admin());
    }

    #[test]
    fn plain_user_context_is_not_admin() {
        let ctx = AuthContext::user(UserId::new(), SystemRole::User);
        assert!(!ctx.is_admin());
    }
}
```

- [ ] **Step 8: Scrivere `lib.rs`**

```rust
//! Tipi ed entità pure di Keeppix. Nessun I/O, nessun SQL, nessuna rete.

pub mod auth;
pub mod error;
pub mod ids;
pub mod user;

pub use auth::{Actor, AuthContext};
pub use error::DomainError;
pub use ids::{GroupId, UserId};
pub use user::{NewUser, SystemRole, User, Username};
```

- [ ] **Step 9: Eseguire i test e verificare che passino**

Run: `cargo test -p keeppix-domain`
Expected: PASS — 8 test.

- [ ] **Step 10: Verificare i lint**

Run: `cargo clippy -p keeppix-domain --all-targets -- -D warnings`
Expected: nessun warning.

- [ ] **Step 11: Commit**

```bash
git add crates/keeppix-domain
git commit -m "feat(domain): add user, id and auth context types"
```

---

