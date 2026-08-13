# Keeppix Fase 0 — Piano di implementazione

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Costruire lo scheletro eseguibile di Keeppix: un binario Rust che si avvia, applica le migrazioni su Postgres, serve un frontend Vue incorporato, permette la creazione del primo amministratore e il login con sessione, distribuito come immagine Docker distroless multi-arch con CI verde.

**Architecture:** Workspace Cargo a 7 crate con confini netti (`domain` senza I/O, `db` unico posto con SQL, `api` senza SQL). Server Axum con configurazione a precedenza env → toml → default. Autenticazione a sessione con cookie `__Host-`, Argon2id, refresh token con rotazione e rilevamento del riuso. Frontend Vue 3 + Tailwind v4 + Reka UI compilato e incorporato nel binario con `rust-embed`. Test di integrazione contro un Postgres reale via testcontainers.

**Tech Stack:** Rust 1.85+ (edition 2024) · Axum 0.8 · sqlx 0.8 (Postgres) · Argon2id (`argon2`) · `utoipa` (OpenAPI 3.1) · `figment` (config) · `tracing` · Vue 3 + TypeScript + Vite + Tailwind CSS v4 + Reka UI + vue-i18n · PostgreSQL 17 + PostGIS 3.5 · Docker distroless `cc-debian12`

**Spec:** [`docs/superpowers/specs/2026-08-13-keeppix-design.md`](../specs/2026-08-13-keeppix-design.md)

## Global Constraints

Requisiti di progetto validi per **ogni** task. Valori copiati dallo spec.

- **Rust edition 2024**, toolchain minima **1.85.0**, fissata in `rust-toolchain.toml`. La macchina di sviluppo ha 1.82: va aggiornata con `rustup update stable` prima del Task 1.
- **Nessun SQL fuori da `keeppix-db`.** Gli handler HTTP non scrivono query. Violazione = task rifiutato in review.
- **`keeppix-media` non conosce il database; `keeppix-db` non conosce le immagini.** In Fase 0 i due crate sono quasi vuoti, ma i confini vanno stabiliti subito.
- **Ogni repository che legge dati di un utente richiede un `AuthContext`** come primo parametro. Non deve esistere un metodo che non lo prenda.
- **Nessun segreto predefinito.** La chiave di sessione è generata al primo avvio e persistita. L'unica variabile d'ambiente obbligatoria è `DATABASE_URL`.
- **Precedenza configurazione:** variabili d'ambiente → `config.toml` → default.
- **Errori API in formato RFC 9457** `application/problem+json` con campo `type` stabile e prefissato `keeppix/` (es. `keeppix/invalid-credentials`). Il backend **non traduce**: le stringhe utente vivono nel frontend.
- **Cookie di sessione:** prefisso `__Host-`, `HttpOnly`, `Secure`, `SameSite=Lax`.
- **Argon2id** con parametri OWASP: `m = 19456 KiB`, `t = 2`, `p = 1`.
- **Frontend:** budget bundle iniziale **150 KB gzip**, verificato in CI. Nessuna lingua predefinita: rilevata da `navigator.language`, modificabile in impostazioni. Italiano e inglese completi.
- **i18n:** `vue-i18n` con formato ICU MessageFormat; date e numeri con l'API `Intl` nativa; nessuna stringa utente hard-coded nei componenti.
- **Immagine Docker:** `gcr.io/distroless/cc-debian12`, non-root, root filesystem read-only, `no-new-privileges`, capability azzerate. glibc, **non musl**.
- **Commit convenzionali** (`feat:`, `fix:`, `chore:`, `test:`, `docs:`, `ci:`) in inglese.
- **Ogni task termina con un commit** e con test verdi.

---

## Struttura dei file

Mappa completa di ciò che esiste a fine Fase 0. Ogni file ha una responsabilità sola.

```
Keeppix/
├── rust-toolchain.toml              toolchain fissata a 1.85.0
├── Cargo.toml                       workspace, dipendenze condivise
├── rustfmt.toml · clippy.toml       stile e lint
├── .env.example                     variabili documentate
├── compose.yaml                     keeppix + db (profilo "bundled")
├── Dockerfile                       multi-stage → distroless
│
├── crates/
│   ├── keeppix-domain/
│   │   ├── src/lib.rs               riesporta i moduli
│   │   ├── src/ids.rs               UserId, GroupId — newtype su Uuid
│   │   ├── src/user.rs              User, NewUser, SystemRole
│   │   ├── src/auth.rs              AuthContext, Actor
│   │   └── src/error.rs             DomainError
│   │
│   ├── keeppix-db/
│   │   ├── src/lib.rs               Db (pool), connect(), migrate()
│   │   ├── src/error.rs             DbError
│   │   ├── src/users.rs             UserRepo
│   │   ├── src/sessions.rs          SessionRepo (refresh + reuse detection)
│   │   ├── src/settings.rs          SettingsRepo (segreti persistiti)
│   │   ├── migrations/0001_users.sql
│   │   ├── migrations/0002_sessions.sql
│   │   ├── migrations/0003_settings.sql
│   │   └── tests/                   test di integrazione (testcontainers)
│   │
│   ├── keeppix-media/src/lib.rs     vuoto in Fase 0, confine stabilito
│   ├── keeppix-jobs/src/lib.rs      vuoto in Fase 0
│   ├── keeppix-dav/src/lib.rs       vuoto in Fase 0
│   │
│   ├── keeppix-api/
│   │   ├── src/lib.rs               router() -> Router
│   │   ├── src/state.rs             AppState
│   │   ├── src/problem.rs           errore RFC 9457
│   │   ├── src/extract.rs           extractor AuthContext
│   │   ├── src/openapi.rs           documento utoipa
│   │   └── src/routes/
│   │       ├── mod.rs
│   │       ├── health.rs            GET /health
│   │       ├── setup.rs             GET/POST /api/v1/setup
│   │       └── auth.rs              login, refresh, logout, me
│   │
│   └── keeppix-server/
│       ├── src/main.rs              CLI: serve | healthcheck | migrate
│       ├── src/config.rs            figment: env → toml → default
│       ├── src/secrets.rs           generazione e persistenza chiave sessione
│       ├── src/telemetry.rs         tracing JSON
│       └── src/embed.rs             rust-embed + fallback SPA
│
└── frontend/
    ├── package.json · vite.config.ts · tsconfig.json
    ├── index.html
    └── src/
        ├── main.ts                  bootstrap, router, i18n
        ├── api/client.ts            fetch tipizzato, gestione problem+json
        ├── i18n/index.ts · it.json · en.json
        ├── stores/session.ts        Pinia: utente corrente
        ├── router.ts                rotte + guardia auth
        ├── style.css                Tailwind v4
        ├── components/ui/           Button.vue, TextField.vue, Alert.vue
        └── views/
            ├── SetupView.vue        creazione primo admin
            ├── LoginView.vue
            └── HomeView.vue         placeholder autenticato
```

**Ordine di dipendenza dei task:** 1 → 2 → 3 → 4 → 5 → 6 → 7, poi 8 (frontend) può procedere in parallelo a 6-7, infine 9 (embed) → 10 (Docker) → 11 (CI completa).

---

## Task 1: Workspace e toolchain

**Files:**
- Create: `rust-toolchain.toml`, `Cargo.toml`, `rustfmt.toml`, `clippy.toml`, `.gitignore`
- Create: `crates/keeppix-domain/Cargo.toml`, `crates/keeppix-domain/src/lib.rs`
- Create: `crates/keeppix-db/Cargo.toml`, `crates/keeppix-db/src/lib.rs`
- Create: `crates/keeppix-media/Cargo.toml`, `crates/keeppix-media/src/lib.rs`
- Create: `crates/keeppix-jobs/Cargo.toml`, `crates/keeppix-jobs/src/lib.rs`
- Create: `crates/keeppix-dav/Cargo.toml`, `crates/keeppix-dav/src/lib.rs`
- Create: `crates/keeppix-api/Cargo.toml`, `crates/keeppix-api/src/lib.rs`
- Create: `crates/keeppix-server/Cargo.toml`, `crates/keeppix-server/src/main.rs`

**Interfaces:**
- Consumes: nulla.
- Produces: il workspace `keeppix` con 7 membri; il binario si chiama `keeppix` e vive in `keeppix-server`.

- [ ] **Step 1: Aggiornare la toolchain**

```bash
rustup update stable && rustc --version
```

Atteso: `1.85.0` o superiore. Se il comando non esiste, installare rustup da https://rustup.rs.

- [ ] **Step 2: Creare `rust-toolchain.toml`**

```toml
[toolchain]
channel = "1.85.0"
components = ["rustfmt", "clippy"]
```

- [ ] **Step 3: Creare il `Cargo.toml` del workspace**

```toml
[workspace]
resolver = "3"
members = ["crates/*"]

[workspace.package]
version = "0.1.0"
edition = "2024"
rust-version = "1.85"
license = "AGPL-3.0-or-later"

[workspace.dependencies]
anyhow = "1"
thiserror = "2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["full"] }
tracing = "0.1"
uuid = { version = "1", features = ["v7", "serde"] }
chrono = { version = "0.4", features = ["serde"] }

[profile.release]
lto = true
codegen-units = 1
strip = true
```

- [ ] **Step 4: Creare i 7 crate**

```bash
cd Keeppix
for c in domain db media jobs dav api; do
  mkdir -p crates/keeppix-$c/src && touch crates/keeppix-$c/src/lib.rs
done
mkdir -p crates/keeppix-server/src
```

- [ ] **Step 5: Scrivere i `Cargo.toml` dei crate libreria**

Per ognuno di `domain`, `media`, `jobs`, `dav` (sostituire `NAME`):

```toml
[package]
name = "keeppix-NAME"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[dependencies]
thiserror.workspace = true
```

Per `keeppix-db` e `keeppix-api` le dipendenze arrivano nei task successivi: per ora identici ai precedenti più `keeppix-domain = { path = "../keeppix-domain" }`.

- [ ] **Step 6: Scrivere il `Cargo.toml` del binario**

```toml
[package]
name = "keeppix-server"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[[bin]]
name = "keeppix"
path = "src/main.rs"

[dependencies]
keeppix-domain = { path = "../keeppix-domain" }
keeppix-db = { path = "../keeppix-db" }
keeppix-api = { path = "../keeppix-api" }
anyhow.workspace = true
tokio.workspace = true
tracing.workspace = true
```

- [ ] **Step 7: Scrivere un `main.rs` minimo**

```rust
fn main() {
    println!("keeppix {}", env!("CARGO_PKG_VERSION"));
}
```

- [ ] **Step 8: Configurare stile e lint**

`rustfmt.toml`:

```toml
edition = "2024"
max_width = 100
```

`clippy.toml`:

```toml
avoid-breaking-exported-api = false
```

Aggiungere in fondo al `Cargo.toml` del workspace:

```toml
[workspace.lints.clippy]
all = { level = "warn", priority = -1 }
pedantic = { level = "warn", priority = -1 }
unwrap_used = "warn"
expect_used = "warn"
```

E in **ogni** `Cargo.toml` di crate:

```toml
[lints]
workspace = true
```

- [ ] **Step 9: Creare `.gitignore`**

```gitignore
/target
/data
/pgdata
node_modules
frontend/dist
.env
*.kpxb
```

- [ ] **Step 10: Verificare che tutto compili**

Run: `cargo build --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --check`
Expected: build OK, nessun warning clippy, formattazione conforme.

- [ ] **Step 11: Verificare l'esecuzione del binario**

Run: `cargo run --bin keeppix`
Expected: stampa `keeppix 0.1.0`

- [ ] **Step 12: Commit**

```bash
git add -A
git commit -m "chore: scaffold cargo workspace with seven crates"
```

---

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

## Task 3: Hashing delle password

Sta in `keeppix-domain` perché è calcolo puro: nessun I/O, nessuna rete, testabile senza database. Tenerlo fuori da `keeppix-api` evita che i dettagli crittografici finiscano negli handler.

**Files:**
- Create: `crates/keeppix-domain/src/password.rs`
- Modify: `crates/keeppix-domain/src/lib.rs`
- Modify: `crates/keeppix-domain/src/error.rs`
- Modify: `crates/keeppix-domain/Cargo.toml`

**Interfaces:**
- Consumes: `DomainError` dal Task 2.
- Produces:
  - `Password::parse(&str) -> Result<Password, DomainError>` — minimo 10 caratteri, massimo 1024.
  - `PasswordHash(String)` con `as_str(&self) -> &str` e `from_stored(String) -> Self`.
  - `hash_password(&Password) -> Result<PasswordHash, DomainError>` — Argon2id, `m=19456, t=2, p=1`.
  - `verify_password(&Password, &PasswordHash) -> bool` — falso su hash malformato, mai panico.
  - `DomainError::PasswordHashing(String)` aggiunto all'enum.

- [ ] **Step 1: Aggiungere le dipendenze**

```bash
cargo add argon2 --features std -p keeppix-domain
cargo add rand_core --features getrandom -p keeppix-domain
```

- [ ] **Step 2: Scrivere i test che falliscono**

`crates/keeppix-domain/src/password.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_rejects_short_input() {
        assert!(Password::parse("corta").is_err());
    }

    #[test]
    fn password_accepts_ten_characters() {
        assert!(Password::parse("abcdefghij").is_ok());
    }

    #[test]
    fn hash_is_verifiable() {
        let pw = Password::parse("correct horse battery staple").unwrap();
        let hash = hash_password(&pw).unwrap();
        assert!(verify_password(&pw, &hash));
    }

    #[test]
    fn hash_rejects_wrong_password() {
        let pw = Password::parse("correct horse battery staple").unwrap();
        let other = Password::parse("incorrect horse battery").unwrap();
        let hash = hash_password(&pw).unwrap();
        assert!(!verify_password(&other, &hash));
    }

    #[test]
    fn same_password_produces_different_hashes() {
        let pw = Password::parse("correct horse battery staple").unwrap();
        assert_ne!(
            hash_password(&pw).unwrap().as_str(),
            hash_password(&pw).unwrap().as_str(),
            "il salt deve essere casuale"
        );
    }

    #[test]
    fn malformed_hash_returns_false_without_panicking() {
        let pw = Password::parse("correct horse battery staple").unwrap();
        let broken = PasswordHash::from_stored("non-un-hash".to_owned());
        assert!(!verify_password(&pw, &broken));
    }

    #[test]
    fn hash_is_argon2id_with_owasp_parameters() {
        let pw = Password::parse("correct horse battery staple").unwrap();
        let hash = hash_password(&pw).unwrap();
        assert!(hash.as_str().starts_with("$argon2id$"));
        assert!(hash.as_str().contains("m=19456,t=2,p=1"));
    }
}
```

- [ ] **Step 3: Eseguire e verificare il fallimento**

Run: `cargo test -p keeppix-domain password`
Expected: FAIL — `cannot find type Password in this scope`.

- [ ] **Step 4: Aggiungere la variante di errore**

In `crates/keeppix-domain/src/error.rs`, dentro l'enum:

```rust
    #[error("password hashing failed: {0}")]
    PasswordHashing(String),
```

- [ ] **Step 5: Implementare `password.rs` (sopra il blocco di test)**

```rust
use argon2::password_hash::{PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng};
use argon2::{Algorithm, Argon2, Params, Version};

use crate::error::DomainError;

const PASSWORD_MIN: usize = 10;
const PASSWORD_MAX: usize = 1024;

// Parametri OWASP: 19 MiB di memoria, 2 iterazioni, parallelismo 1.
const ARGON_M_COST: u32 = 19_456;
const ARGON_T_COST: u32 = 2;
const ARGON_P_COST: u32 = 1;

/// Password in chiaro, viva solo il tempo necessario a produrne l'hash.
#[derive(Clone)]
pub struct Password(String);

impl Password {
    /// # Errors
    /// `DomainError::InvalidPassword` se fuori dai limiti di lunghezza.
    pub fn parse(raw: &str) -> Result<Self, DomainError> {
        let len = raw.chars().count();
        if len < PASSWORD_MIN {
            return Err(DomainError::InvalidPassword(format!(
                "must be at least {PASSWORD_MIN} characters"
            )));
        }
        if len > PASSWORD_MAX {
            return Err(DomainError::InvalidPassword(format!(
                "must be at most {PASSWORD_MAX} characters"
            )));
        }
        Ok(Self(raw.to_owned()))
    }

    fn expose(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

// Impedisce che una password finisca nei log per distrazione.
impl std::fmt::Debug for Password {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Password(***)")
    }
}

/// Hash PHC-encoded, pronto per la persistenza.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PasswordHash(String);

impl PasswordHash {
    #[must_use]
    pub const fn from_stored(value: String) -> Self {
        Self(value)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn argon2() -> Result<Argon2<'static>, DomainError> {
    let params = Params::new(ARGON_M_COST, ARGON_T_COST, ARGON_P_COST, None)
        .map_err(|e| DomainError::PasswordHashing(e.to_string()))?;
    Ok(Argon2::new(Algorithm::Argon2id, Version::V0x13, params))
}

/// # Errors
/// `DomainError::PasswordHashing` se i parametri o il generatore di sale falliscono.
pub fn hash_password(password: &Password) -> Result<PasswordHash, DomainError> {
    let salt = SaltString::generate(&mut OsRng);
    let hash = argon2()?
        .hash_password(password.expose(), &salt)
        .map_err(|e| DomainError::PasswordHashing(e.to_string()))?;
    Ok(PasswordHash(hash.to_string()))
}

/// Restituisce `false` — mai un errore — se l'hash memorizzato è illeggibile,
/// così un record corrotto nega l'accesso invece di far esplodere il login.
#[must_use]
pub fn verify_password(password: &Password, hash: &PasswordHash) -> bool {
    let Ok(parsed) = argon2::PasswordHash::new(hash.as_str()) else {
        return false;
    };
    let Ok(hasher) = argon2() else {
        return false;
    };
    hasher.verify_password(password.expose(), &parsed).is_ok()
}
```

- [ ] **Step 6: Esportare da `lib.rs`**

Aggiungere:

```rust
pub mod password;
pub use password::{Password, PasswordHash, hash_password, verify_password};
```

- [ ] **Step 7: Eseguire i test**

Run: `cargo test -p keeppix-domain`
Expected: PASS — 15 test. L'hashing richiede ~100 ms per chiamata: è normale.

- [ ] **Step 8: Verificare i lint**

Run: `cargo clippy -p keeppix-domain --all-targets -- -D warnings`
Expected: nessun warning.

- [ ] **Step 9: Commit**

```bash
git add crates/keeppix-domain
git commit -m "feat(domain): add argon2id password hashing"
```

---

## Task 4: Connessione al database e migrazioni

**Files:**
- Create: `crates/keeppix-db/src/error.rs`
- Create: `crates/keeppix-db/migrations/0001_users.sql`
- Create: `crates/keeppix-db/migrations/0002_sessions.sql`
- Create: `crates/keeppix-db/migrations/0003_settings.sql`
- Create: `crates/keeppix-db/tests/harness/mod.rs`
- Create: `crates/keeppix-db/tests/migrations.rs`
- Modify: `crates/keeppix-db/src/lib.rs`
- Modify: `crates/keeppix-db/Cargo.toml`

**Interfaces:**
- Consumes: nulla dai task precedenti.
- Produces:
  - `Db` — wrapper clonabile su `PgPool`, con `Db::connect(url: &str, max_connections: u32) -> Result<Db, DbError>`, `Db::migrate(&self) -> Result<(), DbError>`, `Db::pool(&self) -> &PgPool`, `Db::ping(&self) -> Result<(), DbError>`.
  - `DbError::{Connection(sqlx::Error), Migration(String), NotFound, Conflict(String)}`.
  - `tests/harness::TestDb` con `TestDb::start().await -> TestDb` (Postgres in container, migrazioni applicate) e `TestDb::db(&self) -> &Db`.

- [ ] **Step 1: Aggiungere le dipendenze**

```bash
cargo add sqlx --no-default-features \
  --features runtime-tokio,tls-rustls-ring,postgres,uuid,chrono,macros,migrate -p keeppix-db
cargo add keeppix-domain --path crates/keeppix-domain -p keeppix-db
cargo add tokio serde uuid chrono thiserror tracing -p keeppix-db
cargo add --dev testcontainers testcontainers-modules --features postgres -p keeppix-db
cargo add --dev tokio --features macros,rt-multi-thread -p keeppix-db
```

- [ ] **Step 2: Scrivere `error.rs`**

```rust
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
}

impl From<sqlx::migrate::MigrateError> for DbError {
    fn from(e: sqlx::migrate::MigrateError) -> Self {
        Self::Migration(e.to_string())
    }
}
```

- [ ] **Step 3: Scrivere la migrazione `0001_users.sql`**

```sql
-- Estensioni richieste dallo schema completo. PostGIS arriva in Fase 4 ma
-- l'immagine è già postgis/postgis, quindi la si abilita subito per evitare
-- una migrazione che richieda privilegi elevati più avanti.
CREATE EXTENSION IF NOT EXISTS pg_trgm;

CREATE TABLE users (
    id            uuid        PRIMARY KEY,
    username      text        NOT NULL,
    email         text,
    display_name  text        NOT NULL,
    password_hash text        NOT NULL,
    role          text        NOT NULL CHECK (role IN ('admin', 'user')),
    locale        text,
    totp_secret_enc bytea,
    created_at    timestamptz NOT NULL DEFAULT now(),
    updated_at    timestamptz NOT NULL DEFAULT now(),
    disabled_at   timestamptz
);

-- Unicità case-insensitive: gli username sono già normalizzati in minuscolo
-- dal dominio, questo indice impedisce che un bug futuro crei duplicati.
CREATE UNIQUE INDEX users_username_key ON users (lower(username));
CREATE UNIQUE INDEX users_email_key ON users (lower(email)) WHERE email IS NOT NULL;

CREATE TABLE groups (
    id         uuid        PRIMARY KEY,
    name       text        NOT NULL,
    created_by uuid        REFERENCES users (id) ON DELETE SET NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX groups_name_key ON groups (lower(name));

CREATE TABLE group_members (
    group_id uuid        NOT NULL REFERENCES groups (id) ON DELETE CASCADE,
    user_id  uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    added_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (group_id, user_id)
);

CREATE INDEX group_members_user_idx ON group_members (user_id);
```

- [ ] **Step 4: Scrivere la migrazione `0002_sessions.sql`**

```sql
-- Una "famiglia" è la catena di refresh token nata da un singolo login.
-- Il riuso di un token già consumato indica furto: si revoca l'intera famiglia.
CREATE TABLE sessions (
    id                uuid        PRIMARY KEY,
    family_id         uuid        NOT NULL,
    user_id           uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    refresh_token_hash bytea      NOT NULL,
    parent_id         uuid        REFERENCES sessions (id) ON DELETE SET NULL,
    user_agent        text,
    ip                inet,
    created_at        timestamptz NOT NULL DEFAULT now(),
    expires_at        timestamptz NOT NULL,
    consumed_at       timestamptz,
    revoked_at        timestamptz
);

CREATE UNIQUE INDEX sessions_refresh_hash_key ON sessions (refresh_token_hash);
CREATE INDEX sessions_family_idx ON sessions (family_id);
CREATE INDEX sessions_user_idx ON sessions (user_id);
CREATE INDEX sessions_expiry_idx ON sessions (expires_at) WHERE revoked_at IS NULL;
```

- [ ] **Step 5: Scrivere la migrazione `0003_settings.sql`**

```sql
-- Impostazioni di sistema e segreti generati al primo avvio.
-- `value` è jsonb per non dover migrare lo schema a ogni nuova chiave.
CREATE TABLE system_settings (
    key        text        PRIMARY KEY,
    value      jsonb       NOT NULL,
    updated_at timestamptz NOT NULL DEFAULT now()
);
```

- [ ] **Step 6: Scrivere `lib.rs`**

```rust
//! Accesso al database. È l'unico crate del workspace che contiene SQL.

pub mod error;

pub use error::DbError;

use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

#[derive(Clone, Debug)]
pub struct Db {
    pool: PgPool,
}

impl Db {
    /// # Errors
    /// `DbError::Connection` se il pool non riesce a raggiungere il database.
    pub async fn connect(url: &str, max_connections: u32) -> Result<Self, DbError> {
        let pool = PgPoolOptions::new()
            .max_connections(max_connections)
            .acquire_timeout(std::time::Duration::from_secs(10))
            .connect(url)
            .await?;
        Ok(Self { pool })
    }

    /// Applica tutte le migrazioni non ancora eseguite.
    ///
    /// # Errors
    /// `DbError::Migration` se una migrazione fallisce o è stata modificata
    /// dopo essere stata applicata.
    pub async fn migrate(&self) -> Result<(), DbError> {
        MIGRATOR.run(&self.pool).await?;
        Ok(())
    }

    #[must_use]
    pub const fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// # Errors
    /// `DbError::Connection` se il database non risponde.
    pub async fn ping(&self) -> Result<(), DbError> {
        sqlx::query_scalar::<_, i32>("SELECT 1")
            .fetch_one(&self.pool)
            .await?;
        Ok(())
    }
}
```

- [ ] **Step 7: Scrivere il test harness**

`crates/keeppix-db/tests/harness/mod.rs`:

```rust
//! Postgres reale in container per i test di integrazione.
//! Ogni `TestDb` è isolato: container proprio, database vuoto, migrazioni applicate.

use keeppix_db::Db;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::{ContainerAsync, ImageExt};

pub struct TestDb {
    // Tenuto vivo: alla deallocazione il container viene fermato.
    _container: ContainerAsync<Postgres>,
    db: Db,
}

impl TestDb {
    /// # Panics
    /// Se Docker non è disponibile o le migrazioni falliscono: in un test è
    /// il comportamento voluto.
    pub async fn start() -> Self {
        let container = Postgres::default()
            .with_tag("17-3.5")
            .with_name("postgis/postgis")
            .start()
            .await
            .expect("avvio del container Postgres");

        let port = container
            .get_host_port_ipv4(5432)
            .await
            .expect("porta mappata");
        let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");

        let db = Db::connect(&url, 5).await.expect("connessione");
        db.migrate().await.expect("migrazioni");

        Self { _container: container, db }
    }

    #[must_use]
    pub const fn db(&self) -> &Db {
        &self.db
    }
}
```

- [ ] **Step 8: Scrivere il test delle migrazioni**

`crates/keeppix-db/tests/migrations.rs`:

```rust
mod harness;

use harness::TestDb;

#[tokio::test]
async fn migrations_apply_to_an_empty_database() {
    let test = TestDb::start().await;
    test.db().ping().await.expect("il database risponde");
}

#[tokio::test]
async fn migrations_are_idempotent() {
    let test = TestDb::start().await;
    // Rieseguire il migratore su un database già migrato non deve fallire.
    test.db().migrate().await.expect("seconda esecuzione");
}

#[tokio::test]
async fn expected_tables_exist() {
    let test = TestDb::start().await;

    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT table_name FROM information_schema.tables \
         WHERE table_schema = 'public' ORDER BY table_name",
    )
    .fetch_all(test.db().pool())
    .await
    .expect("elenco tabelle");

    for expected in ["users", "groups", "group_members", "sessions", "system_settings"] {
        assert!(tables.contains(&expected.to_owned()), "manca la tabella {expected}");
    }
}

#[tokio::test]
async fn usernames_are_unique_case_insensitively() {
    let test = TestDb::start().await;
    let pool = test.db().pool();

    let insert = "INSERT INTO users (id, username, display_name, password_hash, role) \
                  VALUES ($1, $2, 'X', 'hash', 'user')";

    sqlx::query(insert)
        .bind(uuid::Uuid::now_v7())
        .bind("giovanni")
        .execute(pool)
        .await
        .expect("primo inserimento");

    let second = sqlx::query(insert)
        .bind(uuid::Uuid::now_v7())
        .bind("GIOVANNI")
        .execute(pool)
        .await;

    assert!(second.is_err(), "l'indice unico deve rifiutare il duplicato");
}
```

- [ ] **Step 9: Aggiungere `uuid` alle dev-dependencies**

```bash
cargo add --dev uuid --features v7 -p keeppix-db
```

- [ ] **Step 10: Eseguire i test e verificare che passino**

Assicurarsi che Docker sia in esecuzione, poi:

Run: `cargo test -p keeppix-db`
Expected: PASS — 4 test. Il primo avvio scarica l'immagine `postgis/postgis:17-3.5` (~400 MB) e richiede qualche minuto.

- [ ] **Step 11: Generare la cache offline di sqlx**

Le query verificate a compile-time richiedono un database raggiungibile in fase di build, oppure una cache committata. Si usa la cache, così CI e Docker build non hanno bisogno di Postgres.

```bash
cargo install sqlx-cli --no-default-features --features rustls,postgres
docker run -d --name keeppix-sqlx -e POSTGRES_PASSWORD=postgres -p 55432:5432 postgis/postgis:17-3.5
sleep 5
export DATABASE_URL=postgres://postgres:postgres@127.0.0.1:55432/postgres
cargo sqlx migrate run --source crates/keeppix-db/migrations
cargo sqlx prepare --workspace -- --all-targets
docker rm -f keeppix-sqlx
```

Aggiungere a `.gitignore` **nulla**: la cartella `.sqlx/` va committata.

- [ ] **Step 12: Verificare che la build funzioni offline**

Run: `SQLX_OFFLINE=true cargo build --workspace`
Expected: build OK senza `DATABASE_URL`.

- [ ] **Step 13: Commit**

```bash
git add crates/keeppix-db .sqlx
git commit -m "feat(db): add connection pool, migrations and test harness"
```

---

## Task 5: Repository degli utenti

**Files:**
- Create: `crates/keeppix-db/src/users.rs`
- Create: `crates/keeppix-db/tests/users.rs`
- Modify: `crates/keeppix-db/src/lib.rs`

**Interfaces:**
- Consumes: `Db`, `DbError` (Task 4); `User`, `NewUser`, `Username`, `SystemRole`, `UserId`, `PasswordHash`, `AuthContext` (Task 2-3).
- Produces `UserRepo` con:
  - `UserRepo::new(db: &Db) -> UserRepo`
  - `count(&self) -> Result<i64, DbError>` — senza `AuthContext`: serve a decidere se l'istanza è vergine, prima che esista un utente.
  - `create_bootstrap_admin(&self, new: NewUser) -> Result<User, DbError>` — fallisce con `DbError::Conflict` se esistono già utenti. Unica scrittura senza `AuthContext`, e il nome lo dichiara.
  - `create(&self, ctx: &AuthContext, new: NewUser) -> Result<User, DbError>` — richiede admin.
  - `find_by_username(&self, username: &Username) -> Result<Option<(User, PasswordHash)>, DbError>` — senza `AuthContext` perché è il login stesso; restituisce l'hash per la verifica.
  - `find_by_id(&self, ctx: &AuthContext, id: UserId) -> Result<User, DbError>` — un utente non-admin può leggere solo sé stesso.

- [ ] **Step 1: Scrivere i test che falliscono**

`crates/keeppix-db/tests/users.rs`:

```rust
mod harness;

use harness::TestDb;
use keeppix_db::UserRepo;
use keeppix_domain::{AuthContext, NewUser, SystemRole, UserId, Username, hash_password};
use keeppix_domain::Password;

fn new_user(username: &str, role: SystemRole) -> NewUser {
    let password = Password::parse("correct horse battery staple").unwrap();
    NewUser {
        username: Username::parse(username).unwrap(),
        email: None,
        display_name: username.to_owned(),
        password_hash: hash_password(&password).unwrap().as_str().to_owned(),
        role,
    }
}

#[tokio::test]
async fn fresh_instance_has_no_users() {
    let test = TestDb::start().await;
    let repo = UserRepo::new(test.db());
    assert_eq!(repo.count().await.unwrap(), 0);
}

#[tokio::test]
async fn bootstrap_admin_can_be_created_once() {
    let test = TestDb::start().await;
    let repo = UserRepo::new(test.db());

    let admin = repo.create_bootstrap_admin(new_user("giovanni", SystemRole::Admin)).await.unwrap();
    assert_eq!(admin.username.as_str(), "giovanni");
    assert!(admin.role.is_admin());
    assert_eq!(repo.count().await.unwrap(), 1);

    let second = repo.create_bootstrap_admin(new_user("mario", SystemRole::Admin)).await;
    assert!(second.is_err(), "il bootstrap deve essere possibile una sola volta");
}

#[tokio::test]
async fn login_lookup_returns_user_and_hash() {
    let test = TestDb::start().await;
    let repo = UserRepo::new(test.db());
    repo.create_bootstrap_admin(new_user("giovanni", SystemRole::Admin)).await.unwrap();

    let found = repo
        .find_by_username(&Username::parse("GIOVANNI").unwrap())
        .await
        .unwrap();

    let (user, hash) = found.expect("l'utente esiste, la ricerca è case-insensitive");
    assert_eq!(user.username.as_str(), "giovanni");
    assert!(hash.as_str().starts_with("$argon2id$"));
}

#[tokio::test]
async fn login_lookup_returns_none_for_unknown_user() {
    let test = TestDb::start().await;
    let repo = UserRepo::new(test.db());
    let found = repo.find_by_username(&Username::parse("nessuno").unwrap()).await.unwrap();
    assert!(found.is_none());
}

#[tokio::test]
async fn only_admins_can_create_users() {
    let test = TestDb::start().await;
    let repo = UserRepo::new(test.db());
    let admin = repo.create_bootstrap_admin(new_user("giovanni", SystemRole::Admin)).await.unwrap();

    let admin_ctx = AuthContext::user(admin.id, SystemRole::Admin);
    let created = repo.create(&admin_ctx, new_user("mario", SystemRole::User)).await.unwrap();

    let user_ctx = AuthContext::user(created.id, SystemRole::User);
    let denied = repo.create(&user_ctx, new_user("luigi", SystemRole::User)).await;
    assert!(denied.is_err(), "un utente non-admin non può creare utenti");
}

#[tokio::test]
async fn plain_user_can_only_read_itself() {
    let test = TestDb::start().await;
    let repo = UserRepo::new(test.db());
    let admin = repo.create_bootstrap_admin(new_user("giovanni", SystemRole::Admin)).await.unwrap();
    let admin_ctx = AuthContext::user(admin.id, SystemRole::Admin);
    let mario = repo.create(&admin_ctx, new_user("mario", SystemRole::User)).await.unwrap();

    let mario_ctx = AuthContext::user(mario.id, SystemRole::User);
    assert!(repo.find_by_id(&mario_ctx, mario.id).await.is_ok());
    assert!(repo.find_by_id(&mario_ctx, admin.id).await.is_err());
    assert!(repo.find_by_id(&admin_ctx, mario.id).await.is_ok());
}

#[tokio::test]
async fn duplicate_username_is_a_conflict() {
    let test = TestDb::start().await;
    let repo = UserRepo::new(test.db());
    let admin = repo.create_bootstrap_admin(new_user("giovanni", SystemRole::Admin)).await.unwrap();
    let ctx = AuthContext::user(admin.id, SystemRole::Admin);

    let dup = repo.create(&ctx, new_user("giovanni", SystemRole::User)).await;
    assert!(matches!(dup, Err(keeppix_db::DbError::Conflict(_))));
}

#[tokio::test]
async fn unknown_id_is_not_found() {
    let test = TestDb::start().await;
    let repo = UserRepo::new(test.db());
    let admin = repo.create_bootstrap_admin(new_user("giovanni", SystemRole::Admin)).await.unwrap();
    let ctx = AuthContext::user(admin.id, SystemRole::Admin);

    let missing = repo.find_by_id(&ctx, UserId::new()).await;
    assert!(matches!(missing, Err(keeppix_db::DbError::NotFound)));
}
```

- [ ] **Step 2: Eseguire e verificare il fallimento**

Run: `cargo test -p keeppix-db --test users`
Expected: FAIL — `unresolved import keeppix_db::UserRepo`.

- [ ] **Step 3: Aggiungere `DbError::Forbidden`**

In `crates/keeppix-db/src/error.rs`, dentro l'enum:

```rust
    #[error("forbidden")]
    Forbidden,
```

- [ ] **Step 4: Implementare `users.rs`**

```rust
use keeppix_domain::{
    AuthContext, NewUser, PasswordHash, SystemRole, User, UserId, Username,
};
use sqlx::Row;

use crate::{Db, DbError};

pub struct UserRepo<'a> {
    db: &'a Db,
}

/// Riga grezza della tabella `users`, convertita in `User` dal dominio.
struct UserRow {
    id: uuid::Uuid,
    username: String,
    email: Option<String>,
    display_name: String,
    role: String,
    locale: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
    disabled_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl UserRow {
    fn into_domain(self) -> Result<User, DbError> {
        let username = Username::parse(&self.username)
            .map_err(|e| DbError::Migration(format!("stored username is invalid: {e}")))?;
        let role = match self.role.as_str() {
            "admin" => SystemRole::Admin,
            "user" => SystemRole::User,
            other => return Err(DbError::Migration(format!("unknown role: {other}"))),
        };
        Ok(User {
            id: UserId::from_uuid(self.id),
            username,
            email: self.email,
            display_name: self.display_name,
            role,
            locale: self.locale,
            created_at: self.created_at,
            disabled_at: self.disabled_at,
        })
    }
}

const fn role_str(role: SystemRole) -> &'static str {
    match role {
        SystemRole::Admin => "admin",
        SystemRole::User => "user",
    }
}

/// Traduce la violazione dell'indice unico in un conflitto leggibile.
fn map_unique_violation(err: sqlx::Error) -> DbError {
    if let sqlx::Error::Database(ref db_err) = err
        && db_err.code().as_deref() == Some("23505")
    {
        return DbError::Conflict("username or email already in use".to_owned());
    }
    DbError::Connection(err)
}

impl<'a> UserRepo<'a> {
    #[must_use]
    pub const fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// Numero totale di utenti. Non richiede `AuthContext` perché serve a
    /// stabilire se l'istanza è ancora vergine, cioè prima che un contesto
    /// possa esistere.
    ///
    /// # Errors
    /// `DbError::Connection` se la query fallisce.
    pub async fn count(&self) -> Result<i64, DbError> {
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM users")
            .fetch_one(self.db.pool())
            .await?;
        Ok(n)
    }

    /// Crea il primo amministratore. Unica scrittura priva di `AuthContext`,
    /// permessa solo finché la tabella è vuota.
    ///
    /// # Errors
    /// `DbError::Conflict` se esistono già utenti.
    pub async fn create_bootstrap_admin(&self, new: NewUser) -> Result<User, DbError> {
        let mut tx = self.db.pool().begin().await?;

        // Blocca la tabella per la durata della transazione: due richieste di
        // setup concorrenti non possono creare due amministratori.
        sqlx::query("LOCK TABLE users IN EXCLUSIVE MODE")
            .execute(&mut *tx)
            .await?;

        let existing: i64 = sqlx::query_scalar("SELECT count(*) FROM users")
            .fetch_one(&mut *tx)
            .await?;
        if existing > 0 {
            return Err(DbError::Conflict("instance is already initialised".to_owned()));
        }

        let row = insert_user(&mut tx, &new).await?;
        tx.commit().await?;
        row.into_domain()
    }

    /// # Errors
    /// `DbError::Forbidden` se il chiamante non è admin; `DbError::Conflict`
    /// se username o email sono già in uso.
    pub async fn create(&self, ctx: &AuthContext, new: NewUser) -> Result<User, DbError> {
        if !ctx.is_admin() {
            return Err(DbError::Forbidden);
        }
        let mut tx = self.db.pool().begin().await?;
        let row = insert_user(&mut tx, &new).await?;
        tx.commit().await?;
        row.into_domain()
    }

    /// Ricerca per il login: restituisce anche l'hash della password.
    /// Non richiede `AuthContext` perché è il passo che lo produce.
    ///
    /// # Errors
    /// `DbError::Connection` se la query fallisce.
    pub async fn find_by_username(
        &self,
        username: &Username,
    ) -> Result<Option<(User, PasswordHash)>, DbError> {
        let row = sqlx::query(
            "SELECT id, username, email, display_name, password_hash, role, locale, \
                    created_at, disabled_at \
               FROM users WHERE lower(username) = lower($1)",
        )
        .bind(username.as_str())
        .fetch_optional(self.db.pool())
        .await?;

        let Some(row) = row else { return Ok(None) };

        let hash = PasswordHash::from_stored(row.try_get("password_hash")?);
        let user = UserRow {
            id: row.try_get("id")?,
            username: row.try_get("username")?,
            email: row.try_get("email")?,
            display_name: row.try_get("display_name")?,
            role: row.try_get("role")?,
            locale: row.try_get("locale")?,
            created_at: row.try_get("created_at")?,
            disabled_at: row.try_get("disabled_at")?,
        }
        .into_domain()?;

        Ok(Some((user, hash)))
    }

    /// # Errors
    /// `DbError::Forbidden` se un utente non-admin chiede un id diverso dal
    /// proprio; `DbError::NotFound` se l'utente non esiste.
    pub async fn find_by_id(&self, ctx: &AuthContext, id: UserId) -> Result<User, DbError> {
        if !ctx.is_admin() && ctx.user_id() != Some(id) {
            return Err(DbError::Forbidden);
        }

        let row = sqlx::query(
            "SELECT id, username, email, display_name, role, locale, created_at, disabled_at \
               FROM users WHERE id = $1",
        )
        .bind(id.as_uuid())
        .fetch_optional(self.db.pool())
        .await?
        .ok_or(DbError::NotFound)?;

        UserRow {
            id: row.try_get("id")?,
            username: row.try_get("username")?,
            email: row.try_get("email")?,
            display_name: row.try_get("display_name")?,
            role: row.try_get("role")?,
            locale: row.try_get("locale")?,
            created_at: row.try_get("created_at")?,
            disabled_at: row.try_get("disabled_at")?,
        }
        .into_domain()
    }
}

async fn insert_user(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    new: &NewUser,
) -> Result<UserRow, DbError> {
    let row = sqlx::query(
        "INSERT INTO users (id, username, email, display_name, password_hash, role) \
         VALUES ($1, $2, $3, $4, $5, $6) \
         RETURNING id, username, email, display_name, role, locale, created_at, disabled_at",
    )
    .bind(UserId::new().as_uuid())
    .bind(new.username.as_str())
    .bind(new.email.as_deref())
    .bind(&new.display_name)
    .bind(&new.password_hash)
    .bind(role_str(new.role))
    .fetch_one(&mut **tx)
    .await
    .map_err(map_unique_violation)?;

    Ok(UserRow {
        id: row.try_get("id")?,
        username: row.try_get("username")?,
        email: row.try_get("email")?,
        display_name: row.try_get("display_name")?,
        role: row.try_get("role")?,
        locale: row.try_get("locale")?,
        created_at: row.try_get("created_at")?,
        disabled_at: row.try_get("disabled_at")?,
    })
}
```

- [ ] **Step 5: Esportare da `lib.rs`**

```rust
pub mod users;
pub use users::UserRepo;
```

- [ ] **Step 6: Eseguire i test**

Run: `cargo test -p keeppix-db --test users`
Expected: PASS — 8 test.

- [ ] **Step 7: Verificare che l'intera suite sia ancora verde e senza warning**

Run: `cargo test -p keeppix-db && cargo clippy -p keeppix-db --all-targets -- -D warnings`
Expected: 12 test verdi, nessun warning.

- [ ] **Step 8: Commit**

```bash
git add crates/keeppix-db
git commit -m "feat(db): add user repository with auth-context enforcement"
```

---

## Task 6: Token di sessione e segreti persistiti

**Files:**
- Create: `crates/keeppix-domain/src/token.rs`
- Create: `crates/keeppix-db/src/settings.rs`
- Create: `crates/keeppix-db/tests/settings.rs`
- Modify: `crates/keeppix-domain/src/lib.rs`, `crates/keeppix-domain/Cargo.toml`
- Modify: `crates/keeppix-db/src/lib.rs`

**Interfaces:**
- Consumes: `Db`, `DbError` (Task 4).
- Produces:
  - `SessionToken` con `generate() -> SessionToken`, `from_string(String) -> SessionToken`, `as_str(&self) -> &str`, `digest(&self) -> [u8; 32]` (SHA-256). `Debug` non rivela il valore.
  - `SettingsRepo::new(db: &Db)` con `get_or_create_secret(&self, key: &str) -> Result<[u8; 32], DbError>` — genera al primo accesso, poi restituisce sempre lo stesso valore.

Il token è opaco e casuale, non un JWT: la validazione passa dal database, quindi la revoca è immediata. Nel database vive solo il **digest**: un dump non permette di impersonare nessuno.

- [ ] **Step 1: Aggiungere le dipendenze**

```bash
cargo add sha2 -p keeppix-domain
cargo add rand --features std,std_rng -p keeppix-domain
cargo add base64 -p keeppix-domain
cargo add serde_json -p keeppix-db
```

- [ ] **Step 2: Scrivere i test che falliscono**

`crates/keeppix-domain/src/token.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_tokens_are_unique() {
        assert_ne!(SessionToken::generate().as_str(), SessionToken::generate().as_str());
    }

    #[test]
    fn token_carries_at_least_256_bits() {
        // 32 byte in base64url senza padding = 43 caratteri.
        assert_eq!(SessionToken::generate().as_str().len(), 43);
    }

    #[test]
    fn digest_is_stable_for_the_same_token() {
        let t = SessionToken::generate();
        let copy = SessionToken::from_string(t.as_str().to_owned());
        assert_eq!(t.digest(), copy.digest());
    }

    #[test]
    fn digest_differs_between_tokens() {
        assert_ne!(SessionToken::generate().digest(), SessionToken::generate().digest());
    }

    #[test]
    fn debug_does_not_leak_the_secret() {
        let t = SessionToken::generate();
        let rendered = format!("{t:?}");
        assert!(!rendered.contains(t.as_str()), "il token non deve finire nei log");
    }
}
```

- [ ] **Step 3: Eseguire e verificare il fallimento**

Run: `cargo test -p keeppix-domain token`
Expected: FAIL — `cannot find type SessionToken in this scope`.

- [ ] **Step 4: Implementare `token.rs` (sopra il blocco di test)**

```rust
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rand::RngCore;
use sha2::{Digest, Sha256};

const TOKEN_BYTES: usize = 32;

/// Token opaco di sessione. Il valore in chiaro esiste solo nel cookie del
/// client; il database conserva soltanto `digest()`.
#[derive(Clone, PartialEq, Eq)]
pub struct SessionToken(String);

impl SessionToken {
    #[must_use]
    pub fn generate() -> Self {
        let mut bytes = [0u8; TOKEN_BYTES];
        rand::rng().fill_bytes(&mut bytes);
        Self(URL_SAFE_NO_PAD.encode(bytes))
    }

    #[must_use]
    pub const fn from_string(value: String) -> Self {
        Self(value)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// SHA-256 del token. È ciò che finisce in `sessions.refresh_token_hash`.
    #[must_use]
    pub fn digest(&self) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(self.0.as_bytes());
        hasher.finalize().into()
    }
}

impl std::fmt::Debug for SessionToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SessionToken(***)")
    }
}
```

- [ ] **Step 5: Esportare da `lib.rs` del dominio**

```rust
pub mod token;
pub use token::SessionToken;
```

- [ ] **Step 6: Eseguire i test del dominio**

Run: `cargo test -p keeppix-domain`
Expected: PASS — 20 test.

- [ ] **Step 7: Scrivere i test dei segreti**

`crates/keeppix-db/tests/settings.rs`:

```rust
mod harness;

use harness::TestDb;
use keeppix_db::SettingsRepo;

#[tokio::test]
async fn secret_is_generated_once_and_then_stable() {
    let test = TestDb::start().await;
    let repo = SettingsRepo::new(test.db());

    let first = repo.get_or_create_secret("session_key").await.unwrap();
    let second = repo.get_or_create_secret("session_key").await.unwrap();

    assert_eq!(first, second, "il segreto non deve cambiare fra due letture");
    assert_ne!(first, [0u8; 32], "il segreto non deve essere nullo");
}

#[tokio::test]
async fn different_keys_get_different_secrets() {
    let test = TestDb::start().await;
    let repo = SettingsRepo::new(test.db());

    let session = repo.get_or_create_secret("session_key").await.unwrap();
    let totp = repo.get_or_create_secret("totp_key").await.unwrap();

    assert_ne!(session, totp);
}

#[tokio::test]
async fn concurrent_generation_yields_a_single_secret() {
    let test = TestDb::start().await;
    let repo_a = SettingsRepo::new(test.db());
    let repo_b = SettingsRepo::new(test.db());

    let (a, b) = tokio::join!(
        repo_a.get_or_create_secret("session_key"),
        repo_b.get_or_create_secret("session_key"),
    );

    assert_eq!(a.unwrap(), b.unwrap(), "due avvii concorrenti non devono divergere");
}
```

- [ ] **Step 8: Eseguire e verificare il fallimento**

Run: `cargo test -p keeppix-db --test settings`
Expected: FAIL — `unresolved import keeppix_db::SettingsRepo`.

- [ ] **Step 9: Implementare `settings.rs`**

```rust
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use rand::RngCore;
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
    /// `DbError::Connection` se la query fallisce; `DbError::Migration` se il
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
            .map_err(|e| DbError::Migration(format!("stored secret is not base64: {e}")))?;

        bytes
            .try_into()
            .map_err(|_| DbError::Migration("stored secret is not 32 bytes".to_owned()))
    }
}
```

- [ ] **Step 10: Aggiungere le dipendenze mancanti a `keeppix-db` ed esportare**

```bash
cargo add base64 rand --features std,std_rng -p keeppix-db
```

In `crates/keeppix-db/src/lib.rs`:

```rust
pub mod settings;
pub use settings::SettingsRepo;
```

- [ ] **Step 11: Eseguire i test**

Run: `cargo test -p keeppix-db --test settings`
Expected: PASS — 3 test.

- [ ] **Step 12: Commit**

```bash
git add crates/keeppix-domain crates/keeppix-db
git commit -m "feat: add opaque session tokens and persisted server secrets"
```

---

## Task 7: Repository delle sessioni con rotazione e rilevamento del riuso

**Files:**
- Create: `crates/keeppix-db/src/sessions.rs`
- Create: `crates/keeppix-db/tests/sessions.rs`
- Modify: `crates/keeppix-db/src/lib.rs`

**Interfaces:**
- Consumes: `Db`, `DbError`, `UserRepo` (Task 4-5); `SessionToken` (Task 6).
- Produces `SessionRepo` con:
  - `SessionRepo::new(db: &Db) -> SessionRepo`
  - `create(&self, user_id: UserId, ttl: Duration, user_agent: Option<&str>) -> Result<SessionToken, DbError>` — nuova famiglia, token restituito in chiaro una sola volta.
  - `authenticate(&self, token: &SessionToken) -> Result<AuthContext, DbError>` — `DbError::NotFound` se assente, scaduto, consumato o revocato.
  - `rotate(&self, token: &SessionToken, ttl: Duration) -> Result<SessionToken, DbError>` — consuma il vecchio, emette il nuovo nella stessa famiglia. Se il token era **già consumato**, revoca l'intera famiglia e restituisce `DbError::Forbidden`.
  - `revoke(&self, token: &SessionToken) -> Result<(), DbError>` — logout della sola sessione.
  - `purge_expired(&self) -> Result<u64, DbError>` — manutenzione.

- [ ] **Step 1: Scrivere i test che falliscono**

`crates/keeppix-db/tests/sessions.rs`:

```rust
mod harness;

use std::time::Duration;

use harness::TestDb;
use keeppix_db::{DbError, SessionRepo, UserRepo};
use keeppix_domain::{NewUser, Password, SessionToken, SystemRole, Username, hash_password};

const TTL: Duration = Duration::from_secs(3600);

async fn seed_admin(test: &TestDb) -> keeppix_domain::UserId {
    let password = Password::parse("correct horse battery staple").unwrap();
    let repo = UserRepo::new(test.db());
    repo.create_bootstrap_admin(NewUser {
        username: Username::parse("giovanni").unwrap(),
        email: None,
        display_name: "Giovanni".to_owned(),
        password_hash: hash_password(&password).unwrap().as_str().to_owned(),
        role: SystemRole::Admin,
    })
    .await
    .unwrap()
    .id
}

#[tokio::test]
async fn a_fresh_token_authenticates() {
    let test = TestDb::start().await;
    let user_id = seed_admin(&test).await;
    let repo = SessionRepo::new(test.db());

    let token = repo.create(user_id, TTL, Some("test")).await.unwrap();
    let ctx = repo.authenticate(&token).await.unwrap();

    assert_eq!(ctx.user_id(), Some(user_id));
    assert!(ctx.is_admin());
}

#[tokio::test]
async fn an_unknown_token_is_rejected() {
    let test = TestDb::start().await;
    seed_admin(&test).await;
    let repo = SessionRepo::new(test.db());

    let result = repo.authenticate(&SessionToken::generate()).await;
    assert!(matches!(result, Err(DbError::NotFound)));
}

#[tokio::test]
async fn rotation_issues_a_new_token_and_retires_the_old_one() {
    let test = TestDb::start().await;
    let user_id = seed_admin(&test).await;
    let repo = SessionRepo::new(test.db());

    let first = repo.create(user_id, TTL, None).await.unwrap();
    let second = repo.rotate(&first, TTL).await.unwrap();

    assert_ne!(first.as_str(), second.as_str());
    assert!(repo.authenticate(&second).await.is_ok(), "il nuovo token vale");
    assert!(matches!(repo.authenticate(&first).await, Err(DbError::NotFound)),
            "il vecchio token non vale più");
}

#[tokio::test]
async fn reusing_a_consumed_token_kills_the_whole_family() {
    let test = TestDb::start().await;
    let user_id = seed_admin(&test).await;
    let repo = SessionRepo::new(test.db());

    let first = repo.create(user_id, TTL, None).await.unwrap();
    let second = repo.rotate(&first, TTL).await.unwrap();

    // Un attaccante ripresenta il token già consumato: è furto in corso.
    let replay = repo.rotate(&first, TTL).await;
    assert!(matches!(replay, Err(DbError::Forbidden)));

    // Anche il token legittimo viene invalidato: il legittimo proprietario
    // dovrà rifare il login, ma l'attaccante non ha accesso.
    assert!(matches!(repo.authenticate(&second).await, Err(DbError::NotFound)));
}

#[tokio::test]
async fn revoking_logs_out_only_that_session() {
    let test = TestDb::start().await;
    let user_id = seed_admin(&test).await;
    let repo = SessionRepo::new(test.db());

    let phone = repo.create(user_id, TTL, Some("phone")).await.unwrap();
    let laptop = repo.create(user_id, TTL, Some("laptop")).await.unwrap();

    repo.revoke(&phone).await.unwrap();

    assert!(matches!(repo.authenticate(&phone).await, Err(DbError::NotFound)));
    assert!(repo.authenticate(&laptop).await.is_ok(), "l'altro dispositivo resta connesso");
}

#[tokio::test]
async fn an_expired_token_is_rejected() {
    let test = TestDb::start().await;
    let user_id = seed_admin(&test).await;
    let repo = SessionRepo::new(test.db());

    let token = repo.create(user_id, Duration::from_secs(0), None).await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;

    assert!(matches!(repo.authenticate(&token).await, Err(DbError::NotFound)));
}

#[tokio::test]
async fn purge_removes_expired_sessions_only() {
    let test = TestDb::start().await;
    let user_id = seed_admin(&test).await;
    let repo = SessionRepo::new(test.db());

    let _dead = repo.create(user_id, Duration::from_secs(0), None).await.unwrap();
    let alive = repo.create(user_id, TTL, None).await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;

    assert_eq!(repo.purge_expired().await.unwrap(), 1);
    assert!(repo.authenticate(&alive).await.is_ok());
}

#[tokio::test]
async fn a_disabled_user_cannot_authenticate() {
    let test = TestDb::start().await;
    let user_id = seed_admin(&test).await;
    let repo = SessionRepo::new(test.db());
    let token = repo.create(user_id, TTL, None).await.unwrap();

    sqlx::query("UPDATE users SET disabled_at = now() WHERE id = $1")
        .bind(user_id.as_uuid())
        .execute(test.db().pool())
        .await
        .unwrap();

    assert!(matches!(repo.authenticate(&token).await, Err(DbError::NotFound)));
}
```

- [ ] **Step 2: Eseguire e verificare il fallimento**

Run: `cargo test -p keeppix-db --test sessions`
Expected: FAIL — `unresolved import keeppix_db::SessionRepo`.

- [ ] **Step 3: Implementare `sessions.rs`**

```rust
use std::time::Duration;

use keeppix_domain::{AuthContext, SessionToken, SystemRole, UserId};
use sqlx::Row;
use uuid::Uuid;

use crate::{Db, DbError};

pub struct SessionRepo<'a> {
    db: &'a Db,
}

impl<'a> SessionRepo<'a> {
    #[must_use]
    pub const fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// Apre una nuova famiglia di sessione. Il token in chiaro è restituito
    /// una sola volta: nel database resta solo il digest.
    ///
    /// # Errors
    /// `DbError::Connection` se l'inserimento fallisce.
    pub async fn create(
        &self,
        user_id: UserId,
        ttl: Duration,
        user_agent: Option<&str>,
    ) -> Result<SessionToken, DbError> {
        let token = SessionToken::generate();
        let id = Uuid::now_v7();

        sqlx::query(
            "INSERT INTO sessions \
                 (id, family_id, user_id, refresh_token_hash, user_agent, expires_at) \
             VALUES ($1, $1, $2, $3, $4, now() + $5::interval)",
        )
        .bind(id)
        .bind(user_id.as_uuid())
        .bind(token.digest().as_slice())
        .bind(user_agent)
        .bind(interval(ttl))
        .execute(self.db.pool())
        .await?;

        Ok(token)
    }

    /// # Errors
    /// `DbError::NotFound` se il token è sconosciuto, scaduto, consumato,
    /// revocato, oppure se l'utente è disabilitato.
    pub async fn authenticate(&self, token: &SessionToken) -> Result<AuthContext, DbError> {
        let row = sqlx::query(
            "SELECT u.id AS user_id, u.role \
               FROM sessions s JOIN users u ON u.id = s.user_id \
              WHERE s.refresh_token_hash = $1 \
                AND s.consumed_at IS NULL \
                AND s.revoked_at IS NULL \
                AND s.expires_at > now() \
                AND u.disabled_at IS NULL",
        )
        .bind(token.digest().as_slice())
        .fetch_optional(self.db.pool())
        .await?
        .ok_or(DbError::NotFound)?;

        let user_id: Uuid = row.try_get("user_id")?;
        let role: String = row.try_get("role")?;
        let role = match role.as_str() {
            "admin" => SystemRole::Admin,
            _ => SystemRole::User,
        };

        Ok(AuthContext::user(UserId::from_uuid(user_id), role))
    }

    /// Ruota il token. Se quello presentato risulta **già consumato**, l'unica
    /// spiegazione è che una copia sia in mano a qualcun altro: si revoca
    /// l'intera famiglia e si costringe a un nuovo login.
    ///
    /// # Errors
    /// `DbError::Forbidden` in caso di riuso rilevato; `DbError::NotFound` se
    /// il token non esiste o è scaduto.
    pub async fn rotate(
        &self,
        token: &SessionToken,
        ttl: Duration,
    ) -> Result<SessionToken, DbError> {
        let mut tx = self.db.pool().begin().await?;

        let row = sqlx::query(
            "SELECT id, family_id, user_id, consumed_at, revoked_at, expires_at \
               FROM sessions WHERE refresh_token_hash = $1 FOR UPDATE",
        )
        .bind(token.digest().as_slice())
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(DbError::NotFound)?;

        let family_id: Uuid = row.try_get("family_id")?;
        let consumed: Option<chrono::DateTime<chrono::Utc>> = row.try_get("consumed_at")?;
        let revoked: Option<chrono::DateTime<chrono::Utc>> = row.try_get("revoked_at")?;

        if consumed.is_some() {
            sqlx::query(
                "UPDATE sessions SET revoked_at = now() \
                  WHERE family_id = $1 AND revoked_at IS NULL",
            )
            .bind(family_id)
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            return Err(DbError::Forbidden);
        }

        if revoked.is_some() {
            return Err(DbError::NotFound);
        }

        let expires_at: chrono::DateTime<chrono::Utc> = row.try_get("expires_at")?;
        if expires_at <= chrono::Utc::now() {
            return Err(DbError::NotFound);
        }

        let parent_id: Uuid = row.try_get("id")?;
        let user_id: Uuid = row.try_get("user_id")?;

        sqlx::query("UPDATE sessions SET consumed_at = now() WHERE id = $1")
            .bind(parent_id)
            .execute(&mut *tx)
            .await?;

        let next = SessionToken::generate();
        sqlx::query(
            "INSERT INTO sessions \
                 (id, family_id, user_id, refresh_token_hash, parent_id, expires_at) \
             VALUES ($1, $2, $3, $4, $5, now() + $6::interval)",
        )
        .bind(Uuid::now_v7())
        .bind(family_id)
        .bind(user_id)
        .bind(next.digest().as_slice())
        .bind(parent_id)
        .bind(interval(ttl))
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(next)
    }

    /// # Errors
    /// `DbError::Connection` se l'aggiornamento fallisce.
    pub async fn revoke(&self, token: &SessionToken) -> Result<(), DbError> {
        sqlx::query(
            "UPDATE sessions SET revoked_at = now() \
              WHERE refresh_token_hash = $1 AND revoked_at IS NULL",
        )
        .bind(token.digest().as_slice())
        .execute(self.db.pool())
        .await?;
        Ok(())
    }

    /// # Errors
    /// `DbError::Connection` se la cancellazione fallisce.
    pub async fn purge_expired(&self) -> Result<u64, DbError> {
        let result = sqlx::query("DELETE FROM sessions WHERE expires_at <= now()")
            .execute(self.db.pool())
            .await?;
        Ok(result.rows_affected())
    }
}

/// Postgres non accetta un `Duration` di Rust: si passa un intervallo in secondi.
fn interval(ttl: Duration) -> String {
    format!("{} seconds", ttl.as_secs())
}
```

- [ ] **Step 4: Esportare da `lib.rs`**

```rust
pub mod sessions;
pub use sessions::SessionRepo;
```

- [ ] **Step 5: Eseguire i test**

Run: `cargo test -p keeppix-db --test sessions`
Expected: PASS — 8 test.

- [ ] **Step 6: Verificare l'intera suite e i lint**

Run: `cargo test -p keeppix-db && cargo clippy -p keeppix-db --all-targets -- -D warnings`
Expected: 23 test verdi, nessun warning.

- [ ] **Step 7: Commit**

```bash
git add crates/keeppix-db
git commit -m "feat(db): add session repository with rotation and reuse detection"
```

---

## Task 8: Configurazione, telemetria e CLI del server

**Files:**
- Create: `crates/keeppix-server/src/config.rs`
- Create: `crates/keeppix-server/src/telemetry.rs`
- Create: `crates/keeppix-server/tests/config.rs`
- Create: `.env.example`
- Modify: `crates/keeppix-server/src/main.rs`, `crates/keeppix-server/Cargo.toml`

**Interfaces:**
- Consumes: `Db` (Task 4).
- Produces:
  - `Config { database_url: String, bind: SocketAddr, data_dir: PathBuf, db_max_connections: u32, session_ttl_secs: u64, log_format: LogFormat, allowed_origins: Vec<String> }` con `Config::load(config_path: Option<&Path>) -> Result<Config, anyhow::Error>`.
  - `LogFormat::{Json, Pretty}`.
  - `telemetry::init(format: LogFormat)`.
  - CLI: `keeppix serve` (default), `keeppix migrate`, `keeppix healthcheck`.

Precedenza: variabili d'ambiente con prefisso `KEEPPIX_` → `config.toml` → default. `DATABASE_URL` è accettata anche senza prefisso, perché è la convenzione che tutti si aspettano.

- [ ] **Step 1: Aggiungere le dipendenze**

```bash
cargo add figment --features toml,env -p keeppix-server
cargo add clap --features derive,env -p keeppix-server
cargo add tracing-subscriber --features json,env-filter -p keeppix-server
cargo add axum tower-http --features fs,trace,compression-br,set-header,cors -p keeppix-server
cargo add serde anyhow keeppix-api --path crates/keeppix-api -p keeppix-server
cargo add --dev tempfile -p keeppix-server
```

- [ ] **Step 2: Scrivere i test che falliscono**

`crates/keeppix-server/tests/config.rs`:

```rust
use std::io::Write as _;

use keeppix_server::config::{Config, LogFormat};

/// I test manipolano variabili d'ambiente di processo: vanno eseguiti in serie.
/// `cargo test -- --test-threads=1` è imposto dallo script di verifica.
fn clear_env() {
    for key in ["DATABASE_URL", "KEEPPIX_BIND", "KEEPPIX_DATA_DIR", "KEEPPIX_LOG_FORMAT"] {
        unsafe { std::env::remove_var(key) };
    }
}

#[test]
fn database_url_is_required() {
    clear_env();
    assert!(Config::load(None).is_err(), "senza DATABASE_URL il caricamento fallisce");
}

#[test]
fn defaults_are_applied() {
    clear_env();
    unsafe { std::env::set_var("DATABASE_URL", "postgres://localhost/keeppix") };

    let cfg = Config::load(None).unwrap();
    assert_eq!(cfg.bind.port(), 5673);
    assert_eq!(cfg.data_dir, std::path::PathBuf::from("/data"));
    assert_eq!(cfg.session_ttl_secs, 60 * 60 * 24 * 30);
    assert!(matches!(cfg.log_format, LogFormat::Json));
}

#[test]
fn environment_overrides_the_file() {
    clear_env();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let mut file = std::fs::File::create(&path).unwrap();
    writeln!(file, "database_url = \"postgres://from-file/keeppix\"").unwrap();
    writeln!(file, "bind = \"0.0.0.0:1111\"").unwrap();

    unsafe { std::env::set_var("KEEPPIX_BIND", "0.0.0.0:2222") };

    let cfg = Config::load(Some(&path)).unwrap();
    assert_eq!(cfg.bind.port(), 2222, "l'ambiente vince sul file");
    assert_eq!(cfg.database_url, "postgres://from-file/keeppix", "il file vince sul default");
}

#[test]
fn bare_database_url_is_accepted() {
    clear_env();
    unsafe { std::env::set_var("DATABASE_URL", "postgres://bare/keeppix") };
    assert_eq!(Config::load(None).unwrap().database_url, "postgres://bare/keeppix");
}
```

- [ ] **Step 3: Eseguire e verificare il fallimento**

Run: `cargo test -p keeppix-server --test config -- --test-threads=1`
Expected: FAIL — `unresolved import keeppix_server::config`.

- [ ] **Step 4: Trasformare il binario in libreria + binario**

In `crates/keeppix-server/Cargo.toml`, prima di `[[bin]]`:

```toml
[lib]
name = "keeppix_server"
path = "src/lib.rs"
```

Creare `crates/keeppix-server/src/lib.rs`:

```rust
pub mod config;
pub mod telemetry;
```

- [ ] **Step 5: Implementare `config.rs`**

```rust
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use figment::Figment;
use figment::providers::{Env, Format as _, Serialized, Toml};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    Json,
    Pretty,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Unica impostazione obbligatoria.
    pub database_url: String,
    pub bind: SocketAddr,
    pub data_dir: PathBuf,
    pub db_max_connections: u32,
    pub session_ttl_secs: u64,
    pub log_format: LogFormat,
    /// Origini ammesse per CORS e per la verifica dell'`Origin` sul WebSocket.
    pub allowed_origins: Vec<String>,
}

/// Valori usati quando né l'ambiente né il file dicono nulla.
#[derive(Debug, Serialize)]
struct Defaults {
    bind: SocketAddr,
    data_dir: PathBuf,
    db_max_connections: u32,
    session_ttl_secs: u64,
    log_format: LogFormat,
    allowed_origins: Vec<String>,
}

impl Default for Defaults {
    fn default() -> Self {
        Self {
            bind: "0.0.0.0:5673".parse().expect("literal socket address"),
            data_dir: PathBuf::from("/data"),
            db_max_connections: 10,
            session_ttl_secs: 60 * 60 * 24 * 30,
            log_format: LogFormat::Json,
            allowed_origins: Vec::new(),
        }
    }
}

impl Config {
    /// Precedenza: variabili d'ambiente → file toml → default.
    ///
    /// # Errors
    /// Se `DATABASE_URL` manca o se un valore non è del tipo atteso.
    pub fn load(config_path: Option<&Path>) -> Result<Self, anyhow::Error> {
        let mut figment = Figment::from(Serialized::defaults(Defaults::default()));

        if let Some(path) = config_path
            && path.exists()
        {
            figment = figment.merge(Toml::file(path));
        }

        let figment = figment
            .merge(Env::prefixed("KEEPPIX_"))
            // `DATABASE_URL` senza prefisso: è la convenzione attesa da chiunque.
            .merge(Env::raw().only(&["DATABASE_URL"]));

        figment.extract().map_err(|e| {
            if e.to_string().contains("database_url") {
                anyhow::anyhow!("DATABASE_URL is required (es. postgres://user:pw@host/keeppix)")
            } else {
                anyhow::Error::new(e)
            }
        })
    }
}
```

- [ ] **Step 6: Implementare `telemetry.rs`**

```rust
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt;

use crate::config::LogFormat;

/// Inizializza il logging. `RUST_LOG` sovrascrive il livello predefinito.
pub fn init(format: LogFormat) {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,sqlx=warn,tower_http=info"));

    match format {
        LogFormat::Json => fmt().json().with_env_filter(filter).init(),
        LogFormat::Pretty => fmt().pretty().with_env_filter(filter).init(),
    }
}
```

- [ ] **Step 7: Eseguire i test della configurazione**

Run: `cargo test -p keeppix-server --test config -- --test-threads=1`
Expected: PASS — 4 test.

- [ ] **Step 8: Scrivere `main.rs` con i tre sottocomandi**

```rust
use std::path::PathBuf;

use anyhow::Context as _;
use clap::{Parser, Subcommand};
use keeppix_db::Db;
use keeppix_server::config::Config;
use keeppix_server::telemetry;

#[derive(Parser)]
#[command(name = "keeppix", version)]
struct Cli {
    /// Percorso del file di configurazione.
    #[arg(long, env = "KEEPPIX_CONFIG", default_value = "/data/config.toml")]
    config: PathBuf,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Avvia il server (comportamento predefinito).
    Serve,
    /// Applica le migrazioni ed esce.
    Migrate,
    /// Verifica che il server locale risponda. Usato da HEALTHCHECK in Docker,
    /// dove non esistono né shell né curl.
    Healthcheck,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    if matches!(cli.command, Some(Command::Healthcheck)) {
        return healthcheck().await;
    }

    let config = Config::load(Some(&cli.config))?;
    telemetry::init(config.log_format);

    let db = Db::connect(&config.database_url, config.db_max_connections)
        .await
        .context("connessione al database")?;
    db.migrate().await.context("applicazione delle migrazioni")?;

    match cli.command {
        Some(Command::Migrate) => {
            tracing::info!("migrations applied");
            Ok(())
        }
        _ => serve(config, db).await,
    }
}

async fn serve(config: Config, db: Db) -> anyhow::Result<()> {
    let listener = tokio::net::TcpListener::bind(config.bind).await?;
    tracing::info!(addr = %config.bind, "keeppix listening");

    let app = keeppix_api::router(keeppix_api::AppState::new(db, config.session_ttl_secs));

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

/// Chiusura garbata su SIGTERM (Docker) e Ctrl-C (sviluppo).
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c().await.ok();
    };

    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut sig) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            sig.recv().await;
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
    tracing::info!("shutting down");
}

async fn healthcheck() -> anyhow::Result<()> {
    let port = std::env::var("KEEPPIX_BIND")
        .ok()
        .and_then(|b| b.rsplit(':').next().and_then(|p| p.parse::<u16>().ok()))
        .unwrap_or(5673);

    let stream = tokio::net::TcpStream::connect(("127.0.0.1", port)).await?;
    drop(stream);
    Ok(())
}
```

- [ ] **Step 9: Scrivere `.env.example`**

```bash
# Unica variabile obbligatoria.
DATABASE_URL=postgres://keeppix:changeme@localhost:5432/keeppix

# Opzionali: mostrati con i valori predefiniti.
# KEEPPIX_BIND=0.0.0.0:5673
# KEEPPIX_DATA_DIR=/data
# KEEPPIX_DB_MAX_CONNECTIONS=10
# KEEPPIX_SESSION_TTL_SECS=2592000
# KEEPPIX_LOG_FORMAT=json
# KEEPPIX_ALLOWED_ORIGINS=["https://foto.example.com"]
# RUST_LOG=info,sqlx=warn
```

- [ ] **Step 10: Verificare compilazione e lint**

Il codice non compilerà finché `keeppix_api::router` e `AppState` non esistono (Task 9). Verificare solo il crate config:

Run: `cargo test -p keeppix-server --test config -- --test-threads=1 && cargo fmt --check`
Expected: PASS.

- [ ] **Step 11: Commit**

```bash
git add crates/keeppix-server .env.example
git commit -m "feat(server): add layered config, telemetry and cli subcommands"
```

---

## Task 9: Stato applicativo, errori RFC 9457 ed extractor di autenticazione

**Files:**
- Create: `crates/keeppix-api/src/state.rs`
- Create: `crates/keeppix-api/src/problem.rs`
- Create: `crates/keeppix-api/src/extract.rs`
- Create: `crates/keeppix-api/src/routes/mod.rs`
- Create: `crates/keeppix-api/src/routes/health.rs`
- Create: `crates/keeppix-api/tests/health.rs`
- Modify: `crates/keeppix-api/src/lib.rs`, `crates/keeppix-api/Cargo.toml`

**Interfaces:**
- Consumes: `Db`, `DbError`, `SessionRepo` (Task 4, 7); `AuthContext`, `SessionToken` (Task 2, 6).
- Produces:
  - `AppState { db: Db, session_ttl: Duration }` clonabile, con `AppState::new(db: Db, session_ttl_secs: u64) -> AppState`.
  - `Problem` con `Problem::new(status: StatusCode, type_slug: &str, title: &str) -> Problem`, `with_detail(self, detail: impl Into<String>) -> Problem`, e `impl IntoResponse` che emette `application/problem+json`. `impl From<DbError> for Problem`.
  - `Auth(pub AuthContext)` — extractor Axum che legge il cookie `__Host-kpx_session`; risponde `401 keeppix/unauthenticated` se assente o non valido.
  - `AdminAuth(pub AuthContext)` — come sopra ma `403 keeppix/forbidden` se non admin.
  - `SESSION_COOKIE: &str = "__Host-kpx_session"`.
  - `router(state: AppState) -> axum::Router` con `GET /health` e gli header di sicurezza applicati.

- [ ] **Step 1: Aggiungere le dipendenze**

```bash
cargo add axum --features macros -p keeppix-api
cargo add axum-extra --features cookie -p keeppix-api
cargo add tower-http --features set-header,trace,compression-br,cors -p keeppix-api
cargo add tower keeppix-db --path crates/keeppix-db -p keeppix-api
cargo add serde serde_json tokio tracing http -p keeppix-api
cargo add --dev tower --features util -p keeppix-api
cargo add --dev http-body-util -p keeppix-api
```

- [ ] **Step 2: Scrivere i test che falliscono**

`crates/keeppix-api/tests/health.rs`:

```rust
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use tower::ServiceExt as _;

/// `/health` non tocca il database, quindi il test non ha bisogno di Postgres.
fn app() -> axum::Router {
    keeppix_api::router_without_state()
}

#[tokio::test]
async fn health_returns_ok() {
    let response = app()
        .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn security_headers_are_present() {
    let response = app()
        .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();

    let headers = response.headers();
    assert_eq!(headers.get("x-content-type-options").unwrap(), "nosniff");
    assert_eq!(headers.get("referrer-policy").unwrap(), "no-referrer");
    assert!(headers.get("content-security-policy").is_some());
    assert_eq!(
        headers.get("permissions-policy").unwrap(),
        "camera=(), microphone=(), geolocation=()"
    );
}

#[tokio::test]
async fn unknown_api_path_returns_problem_json() {
    let response = app()
        .oneshot(
            Request::builder()
                .uri("/api/v1/does-not-exist")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/problem+json"
    );

    let body = http_body_util::BodyExt::collect(response.into_body())
        .await
        .unwrap()
        .to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["type"], "keeppix/not-found");
    assert_eq!(json["status"], 404);
}
```

- [ ] **Step 3: Eseguire e verificare il fallimento**

Run: `cargo test -p keeppix-api --test health`
Expected: FAIL — `cannot find function router_without_state`.

- [ ] **Step 4: Implementare `problem.rs`**

```rust
use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use keeppix_db::DbError;
use serde::Serialize;

/// Errore in formato RFC 9457. Il campo `type` è un codice stabile su cui i
/// client possono ramificare; `title` è in inglese e serve al debug, non
/// all'utente finale — la traduzione avviene nel frontend.
#[derive(Debug, Serialize)]
pub struct Problem {
    #[serde(rename = "type")]
    pub type_slug: String,
    pub title: String,
    pub status: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(skip)]
    status_code: StatusCode,
}

impl Problem {
    #[must_use]
    pub fn new(status: StatusCode, type_slug: &str, title: &str) -> Self {
        Self {
            type_slug: format!("keeppix/{type_slug}"),
            title: title.to_owned(),
            status: status.as_u16(),
            detail: None,
            status_code: status,
        }
    }

    #[must_use]
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    #[must_use]
    pub fn not_found() -> Self {
        Self::new(StatusCode::NOT_FOUND, "not-found", "Resource not found")
    }

    #[must_use]
    pub fn unauthenticated() -> Self {
        Self::new(StatusCode::UNAUTHORIZED, "unauthenticated", "Authentication required")
    }

    #[must_use]
    pub fn forbidden() -> Self {
        Self::new(StatusCode::FORBIDDEN, "forbidden", "Not allowed")
    }

    #[must_use]
    pub fn internal() -> Self {
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal-error",
            "Unexpected server error",
        )
    }
}

impl IntoResponse for Problem {
    fn into_response(self) -> Response {
        let status = self.status_code;
        let mut response = (status, Json(self)).into_response();
        response.headers_mut().insert(
            axum::http::header::CONTENT_TYPE,
            axum::http::HeaderValue::from_static("application/problem+json"),
        );
        response
    }
}

impl From<DbError> for Problem {
    fn from(err: DbError) -> Self {
        match err {
            DbError::NotFound => Self::not_found(),
            DbError::Forbidden => Self::forbidden(),
            DbError::Conflict(msg) => {
                Self::new(StatusCode::CONFLICT, "conflict", "Conflict").with_detail(msg)
            }
            // I dettagli interni restano nei log, non nella risposta.
            other => {
                tracing::error!(error = %other, "database error");
                Self::internal()
            }
        }
    }
}
```

- [ ] **Step 5: Implementare `state.rs`**

```rust
use std::time::Duration;

use keeppix_db::Db;

#[derive(Clone)]
pub struct AppState {
    pub db: Db,
    pub session_ttl: Duration,
}

impl AppState {
    #[must_use]
    pub const fn new(db: Db, session_ttl_secs: u64) -> Self {
        Self { db, session_ttl: Duration::from_secs(session_ttl_secs) }
    }
}
```

- [ ] **Step 6: Implementare `extract.rs`**

```rust
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum_extra::extract::CookieJar;
use keeppix_db::SessionRepo;
use keeppix_domain::{AuthContext, SessionToken};

use crate::problem::Problem;
use crate::state::AppState;

pub const SESSION_COOKIE: &str = "__Host-kpx_session";

/// Estrae il contesto di autenticazione dal cookie di sessione.
/// Ogni handler che tratta dati di un utente **deve** prendere questo
/// extractor: è il modo in cui l'`AuthContext` raggiunge i repository.
pub struct Auth(pub AuthContext);

impl FromRequestParts<AppState> for Auth {
    type Rejection = Problem;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let jar = CookieJar::from_headers(&parts.headers);
        let cookie = jar.get(SESSION_COOKIE).ok_or_else(Problem::unauthenticated)?;
        let token = SessionToken::from_string(cookie.value().to_owned());

        let ctx = SessionRepo::new(&state.db)
            .authenticate(&token)
            .await
            .map_err(|_| Problem::unauthenticated())?;

        Ok(Self(ctx))
    }
}

/// Come `Auth`, ma rifiuta chi non è amministratore.
pub struct AdminAuth(pub AuthContext);

impl FromRequestParts<AppState> for AdminAuth {
    type Rejection = Problem;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let Auth(ctx) = Auth::from_request_parts(parts, state).await?;
        if !ctx.is_admin() {
            return Err(Problem::forbidden());
        }
        Ok(Self(ctx))
    }
}
```

- [ ] **Step 7: Implementare `routes/health.rs` e `routes/mod.rs`**

`routes/health.rs`:

```rust
use axum::Json;
use serde::Serialize;

#[derive(Serialize)]
pub struct Health {
    status: &'static str,
    version: &'static str,
}

pub async fn get() -> Json<Health> {
    Json(Health { status: "ok", version: env!("CARGO_PKG_VERSION") })
}
```

`routes/mod.rs`:

```rust
pub mod health;
```

- [ ] **Step 8: Implementare `lib.rs`**

```rust
//! Superficie HTTP di Keeppix. Non contiene SQL: ogni accesso ai dati passa
//! dai repository di `keeppix-db`, che richiedono un `AuthContext`.

pub mod extract;
pub mod problem;
pub mod routes;
pub mod state;

pub use extract::{AdminAuth, Auth, SESSION_COOKIE};
pub use problem::Problem;
pub use state::AppState;

use axum::Router;
use axum::http::HeaderValue;
use axum::routing::get;
use tower_http::compression::CompressionLayer;
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::trace::TraceLayer;

/// Content Security Policy restrittiva. `style-src` ammette `unsafe-inline`
/// perché Vue inietta stili scoped a runtime; gli script no.
const CSP: &str = "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; \
                   img-src 'self' data: blob:; connect-src 'self'; frame-ancestors 'none'; \
                   base-uri 'none'; form-action 'self'";

/// Router con stato, montato dal binario.
#[must_use]
pub fn router(state: AppState) -> Router {
    base_router().with_state(state)
}

/// Router senza stato, per i test che non toccano il database.
#[must_use]
pub fn router_without_state() -> Router {
    base_router_stateless()
}

fn common_layers<S: Clone + Send + Sync + 'static>(router: Router<S>) -> Router<S> {
    router
        .layer(SetResponseHeaderLayer::overriding(
            axum::http::header::HeaderName::from_static("x-content-type-options"),
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            axum::http::header::HeaderName::from_static("referrer-policy"),
            HeaderValue::from_static("no-referrer"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            axum::http::header::HeaderName::from_static("content-security-policy"),
            HeaderValue::from_static(CSP),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            axum::http::header::HeaderName::from_static("permissions-policy"),
            HeaderValue::from_static("camera=(), microphone=(), geolocation=()"),
        ))
        .layer(CompressionLayer::new().br(true).gzip(true))
        .layer(TraceLayer::new_for_http())
        .fallback(not_found)
}

fn base_router() -> Router<AppState> {
    common_layers(Router::new().route("/health", get(routes::health::get)))
}

fn base_router_stateless() -> Router {
    common_layers(Router::new().route("/health", get(routes::health::get)))
}

async fn not_found() -> Problem {
    Problem::not_found()
}
```

- [ ] **Step 9: Eseguire i test**

Run: `cargo test -p keeppix-api --test health`
Expected: PASS — 3 test.

- [ ] **Step 10: Verificare che il server compili e si avvii**

```bash
docker run -d --name keeppix-dev -e POSTGRES_PASSWORD=postgres -p 55432:5432 postgis/postgis:17-3.5
sleep 5
DATABASE_URL=postgres://postgres:postgres@127.0.0.1:55432/postgres \
KEEPPIX_LOG_FORMAT=pretty KEEPPIX_DATA_DIR=./data \
cargo run --bin keeppix -- --config ./nonexistent.toml serve
```

In un altro terminale: `curl -i http://127.0.0.1:5673/health`
Expected: `200 OK`, corpo `{"status":"ok","version":"0.1.0"}`, header di sicurezza presenti.

- [ ] **Step 11: Commit**

```bash
git add crates/keeppix-api crates/keeppix-server
git commit -m "feat(api): add app state, rfc9457 problems and auth extractors"
```

---

## Task 10: Setup iniziale e autenticazione

**Files:**
- Create: `crates/keeppix-api/src/routes/setup.rs`
- Create: `crates/keeppix-api/src/routes/auth.rs`
- Create: `crates/keeppix-api/src/cookie.rs`
- Create: `crates/keeppix-api/tests/harness/mod.rs`
- Create: `crates/keeppix-api/tests/auth.rs`
- Modify: `crates/keeppix-api/src/lib.rs`, `crates/keeppix-api/src/routes/mod.rs`, `crates/keeppix-api/Cargo.toml`

**Interfaces:**
- Consumes: `AppState`, `Problem`, `Auth`, `SESSION_COOKIE` (Task 9); `UserRepo`, `SessionRepo` (Task 5, 7); `Password`, `Username`, `hash_password`, `verify_password` (Task 2-3).
- Produces gli endpoint:
  - `GET /api/v1/setup/status` → `{ "initialised": bool }`. Pubblico.
  - `POST /api/v1/setup` con `{ username, display_name, email?, password }` → `201` + cookie di sessione + `{ user }`. `409 keeppix/already-initialised` se esistono già utenti.
  - `POST /api/v1/auth/login` con `{ username, password }` → `200` + cookie + `{ user }`. `401 keeppix/invalid-credentials`.
  - `POST /api/v1/auth/refresh` → `204` + cookie ruotato. `401` se riuso rilevato.
  - `POST /api/v1/auth/logout` → `204` + cookie cancellato.
  - `GET /api/v1/auth/me` → `{ user }`. Richiede `Auth`.
- Produce inoltre `cookie::session_cookie(token, ttl) -> Cookie` e `cookie::clearing_cookie() -> Cookie`.

- [ ] **Step 1: Aggiungere le dipendenze**

```bash
cargo add keeppix-domain --path crates/keeppix-domain -p keeppix-api
cargo add --dev testcontainers-modules --features postgres -p keeppix-api
cargo add --dev testcontainers tokio --features macros,rt-multi-thread -p keeppix-api
cargo add --dev reqwest --no-default-features --features json,rustls-tls,cookies -p keeppix-api
```

- [ ] **Step 2: Scrivere l'harness HTTP**

`crates/keeppix-api/tests/harness/mod.rs`:

```rust
//! Server reale su porta effimera con Postgres reale in container.
//! I test parlano HTTP come un browser, cookie inclusi: è l'unico modo di
//! verificare davvero il comportamento dei cookie di sessione.

use keeppix_db::Db;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::{ContainerAsync, ImageExt};

pub struct TestServer {
    _container: ContainerAsync<Postgres>,
    pub base_url: String,
    pub client: reqwest::Client,
}

impl TestServer {
    /// # Panics
    /// Se Docker non è disponibile o il server non si avvia.
    pub async fn start() -> Self {
        let container = Postgres::default()
            .with_tag("17-3.5")
            .with_name("postgis/postgis")
            .start()
            .await
            .expect("container Postgres");
        let port = container.get_host_port_ipv4(5432).await.expect("porta");
        let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");

        let db = Db::connect(&url, 5).await.expect("connessione");
        db.migrate().await.expect("migrazioni");

        let state = keeppix_api::AppState::new(db, 3600);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("indirizzo");

        tokio::spawn(async move {
            axum::serve(listener, keeppix_api::router(state)).await.ok();
        });

        let client = reqwest::Client::builder()
            .cookie_store(true)
            .build()
            .expect("client http");

        Self { _container: container, base_url: format!("http://{addr}"), client }
    }

    pub fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base_url)
    }
}
```

> **Nota sui cookie nei test:** il cookie usa il prefisso `__Host-`, che richiede `Secure` e quindi HTTPS. In test si parla in chiaro su `127.0.0.1`, dove `reqwest` non memorizzerebbe un cookie `Secure`. Per questo `session_cookie` emette l'attributo `Secure` solo quando la richiesta non arriva da localhost — vedi Step 4.

- [ ] **Step 3: Scrivere i test che falliscono**

`crates/keeppix-api/tests/auth.rs`:

```rust
mod harness;

use harness::TestServer;
use serde_json::json;

#[tokio::test]
async fn a_fresh_instance_reports_not_initialised() {
    let server = TestServer::start().await;
    let body: serde_json::Value = server
        .client
        .get(server.url("/api/v1/setup/status"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    assert_eq!(body["initialised"], false);
}

#[tokio::test]
async fn setup_creates_the_first_admin_and_logs_in() {
    let server = TestServer::start().await;

    let response = server
        .client
        .post(server.url("/api/v1/setup"))
        .json(&json!({
            "username": "giovanni",
            "display_name": "Giovanni",
            "password": "correct horse battery staple"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 201);
    let cookie = response
        .headers()
        .get("set-cookie")
        .expect("il setup deve autenticare subito")
        .to_str()
        .unwrap()
        .to_owned();
    assert!(cookie.contains("__Host-kpx_session="));
    assert!(cookie.contains("HttpOnly"));
    assert!(cookie.contains("SameSite=Lax"));

    let me: serde_json::Value = server
        .client
        .get(server.url("/api/v1/auth/me"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(me["user"]["username"], "giovanni");
    assert_eq!(me["user"]["role"], "admin");
}

#[tokio::test]
async fn setup_can_only_run_once() {
    let server = TestServer::start().await;
    let payload = json!({
        "username": "giovanni",
        "display_name": "Giovanni",
        "password": "correct horse battery staple"
    });

    server.client.post(server.url("/api/v1/setup")).json(&payload).send().await.unwrap();

    let second = server
        .client
        .post(server.url("/api/v1/setup"))
        .json(&json!({
            "username": "mario",
            "display_name": "Mario",
            "password": "correct horse battery staple"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(second.status(), 409);
    let body: serde_json::Value = second.json().await.unwrap();
    assert_eq!(body["type"], "keeppix/already-initialised");
}

#[tokio::test]
async fn setup_rejects_a_weak_password() {
    let server = TestServer::start().await;
    let response = server
        .client
        .post(server.url("/api/v1/setup"))
        .json(&json!({ "username": "giovanni", "display_name": "G", "password": "corta" }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 422);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["type"], "keeppix/invalid-password");
}

#[tokio::test]
async fn login_succeeds_with_correct_credentials() {
    let server = TestServer::start().await;
    setup(&server).await;

    let response = server
        .client
        .post(server.url("/api/v1/auth/login"))
        .json(&json!({ "username": "GIOVANNI", "password": "correct horse battery staple" }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200, "lo username è case-insensitive");
}

#[tokio::test]
async fn login_fails_with_wrong_password() {
    let server = TestServer::start().await;
    setup(&server).await;

    let response = server
        .client
        .post(server.url("/api/v1/auth/login"))
        .json(&json!({ "username": "giovanni", "password": "password sbagliata" }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 401);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["type"], "keeppix/invalid-credentials");
}

#[tokio::test]
async fn login_fails_identically_for_unknown_user() {
    let server = TestServer::start().await;
    setup(&server).await;

    let response = server
        .client
        .post(server.url("/api/v1/auth/login"))
        .json(&json!({ "username": "nessuno", "password": "correct horse battery staple" }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 401);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(
        body["type"], "keeppix/invalid-credentials",
        "utente inesistente e password errata devono essere indistinguibili"
    );
}

#[tokio::test]
async fn me_requires_authentication() {
    let server = TestServer::start().await;
    setup(&server).await;

    // Client nuovo, senza cookie.
    let anonymous = reqwest::Client::new();
    let response = anonymous.get(server.url("/api/v1/auth/me")).send().await.unwrap();

    assert_eq!(response.status(), 401);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["type"], "keeppix/unauthenticated");
}

#[tokio::test]
async fn refresh_rotates_the_session_cookie() {
    let server = TestServer::start().await;

    let setup_response = server
        .client
        .post(server.url("/api/v1/setup"))
        .json(&json!({
            "username": "giovanni",
            "display_name": "Giovanni",
            "password": "correct horse battery staple"
        }))
        .send()
        .await
        .unwrap();
    let before = session_value_from(&setup_response);

    let refresh = server.client.post(server.url("/api/v1/auth/refresh")).send().await.unwrap();
    assert_eq!(refresh.status(), 204);
    let after = session_value_from(&refresh);

    assert_ne!(before, after, "il cookie deve cambiare a ogni refresh");

    // Il nuovo cookie continua a valere.
    let me = server.client.get(server.url("/api/v1/auth/me")).send().await.unwrap();
    assert_eq!(me.status(), 200);
}

#[tokio::test]
async fn logout_invalidates_the_session() {
    let server = TestServer::start().await;
    setup(&server).await;

    let response = server.client.post(server.url("/api/v1/auth/logout")).send().await.unwrap();
    assert_eq!(response.status(), 204);

    let me = server.client.get(server.url("/api/v1/auth/me")).send().await.unwrap();
    assert_eq!(me.status(), 401);
}

async fn setup(server: &TestServer) {
    server
        .client
        .post(server.url("/api/v1/setup"))
        .json(&json!({
            "username": "giovanni",
            "display_name": "Giovanni",
            "password": "correct horse battery staple"
        }))
        .send()
        .await
        .unwrap();
}

/// Estrae il valore del cookie di sessione dall'header `set-cookie` di una
/// risposta. Il cookie store di `reqwest` non è ispezionabile, quindi si legge
/// direttamente ciò che il server ha emesso.
fn session_value_from(response: &reqwest::Response) -> String {
    response
        .headers()
        .get("set-cookie")
        .expect("set-cookie presente")
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .trim_start_matches("__Host-kpx_session=")
        .to_owned()
}
```

- [ ] **Step 4: Eseguire e verificare il fallimento**

Run: `cargo test -p keeppix-api --test auth`
Expected: FAIL — tutti i test rispondono 404, gli endpoint non esistono.

- [ ] **Step 5: Implementare `cookie.rs`**

```rust
use std::time::Duration;

use axum_extra::extract::cookie::{Cookie, SameSite};
use keeppix_domain::SessionToken;

use crate::extract::SESSION_COOKIE;

/// Cookie di sessione. `__Host-` impone `Secure` e `Path=/`, e vieta `Domain`:
/// il cookie non può essere piazzato da un sottodominio compromesso.
///
/// `secure` è parametrico perché in test si parla in chiaro su 127.0.0.1, dove
/// un client conforme scarterebbe un cookie `Secure`.
#[must_use]
pub fn session_cookie(token: &SessionToken, ttl: Duration, secure: bool) -> Cookie<'static> {
    let mut cookie = Cookie::new(SESSION_COOKIE, token.as_str().to_owned());
    cookie.set_http_only(true);
    cookie.set_secure(secure);
    cookie.set_same_site(SameSite::Lax);
    cookie.set_path("/");
    cookie.set_max_age(Some(
        time::Duration::try_from(ttl).unwrap_or(time::Duration::days(30)),
    ));
    cookie
}

#[must_use]
pub fn clearing_cookie() -> Cookie<'static> {
    let mut cookie = Cookie::new(SESSION_COOKIE, "");
    cookie.set_http_only(true);
    cookie.set_path("/");
    cookie.set_max_age(Some(time::Duration::ZERO));
    cookie
}

/// Vero quando la richiesta non arriva da localhost: in produzione si sta
/// dietro HTTPS, quindi il cookie deve essere `Secure`.
#[must_use]
pub fn should_be_secure(host: Option<&str>) -> bool {
    !matches!(host, Some(h) if h.starts_with("127.0.0.1") || h.starts_with("localhost"))
}
```

Aggiungere la dipendenza: `cargo add time -p keeppix-api`

- [ ] **Step 6: Implementare `routes/setup.rs`**

```rust
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::{Json, response::IntoResponse};
use axum_extra::extract::CookieJar;
use keeppix_db::{SessionRepo, UserRepo};
use keeppix_domain::{NewUser, Password, SystemRole, Username, hash_password};
use serde::{Deserialize, Serialize};

use crate::cookie::{session_cookie, should_be_secure};
use crate::problem::Problem;
use crate::routes::auth::UserView;
use crate::state::AppState;

#[derive(Serialize)]
pub struct SetupStatus {
    initialised: bool,
}

/// # Errors
/// `Problem` se il conteggio degli utenti fallisce.
pub async fn status(State(state): State<AppState>) -> Result<Json<SetupStatus>, Problem> {
    let count = UserRepo::new(&state.db).count().await?;
    Ok(Json(SetupStatus { initialised: count > 0 }))
}

#[derive(Deserialize)]
pub struct SetupRequest {
    username: String,
    display_name: String,
    email: Option<String>,
    password: String,
}

#[derive(Serialize)]
pub struct SetupResponse {
    user: UserView,
}

/// Crea il primo amministratore e apre subito una sessione.
///
/// # Errors
/// `409 already-initialised` se l'istanza è già configurata;
/// `422 invalid-username` / `422 invalid-password` sui dati non validi.
pub async fn create(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Json(req): Json<SetupRequest>,
) -> Result<impl IntoResponse, Problem> {
    let username = Username::parse(&req.username).map_err(|e| {
        Problem::new(StatusCode::UNPROCESSABLE_ENTITY, "invalid-username", "Invalid username")
            .with_detail(e.to_string())
    })?;
    let password = Password::parse(&req.password).map_err(|e| {
        Problem::new(StatusCode::UNPROCESSABLE_ENTITY, "invalid-password", "Invalid password")
            .with_detail(e.to_string())
    })?;
    let hash = hash_password(&password).map_err(|_| Problem::internal())?;

    let users = UserRepo::new(&state.db);
    let user = users
        .create_bootstrap_admin(NewUser {
            username,
            email: req.email,
            display_name: req.display_name,
            password_hash: hash.as_str().to_owned(),
            role: SystemRole::Admin,
        })
        .await
        .map_err(|e| match e {
            keeppix_db::DbError::Conflict(_) => Problem::new(
                StatusCode::CONFLICT,
                "already-initialised",
                "Instance is already initialised",
            ),
            other => Problem::from(other),
        })?;

    let token = SessionRepo::new(&state.db)
        .create(user.id, state.session_ttl, user_agent(&headers))
        .await?;

    let secure = should_be_secure(host(&headers));
    let jar = jar.add(session_cookie(&token, state.session_ttl, secure));

    Ok((
        StatusCode::CREATED,
        jar,
        Json(SetupResponse { user: UserView::from(&user) }),
    ))
}

pub(crate) fn user_agent(headers: &HeaderMap) -> Option<&str> {
    headers.get(header::USER_AGENT).and_then(|v| v.to_str().ok())
}

pub(crate) fn host(headers: &HeaderMap) -> Option<&str> {
    headers.get(header::HOST).and_then(|v| v.to_str().ok())
}
```

- [ ] **Step 7: Implementare `routes/auth.rs`**

```rust
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::{Json, response::IntoResponse};
use axum_extra::extract::CookieJar;
use keeppix_db::{SessionRepo, UserRepo};
use keeppix_domain::{
    AuthContext, Password, SessionToken, SystemRole, User, Username, verify_password,
};
use serde::{Deserialize, Serialize};

use crate::cookie::{clearing_cookie, session_cookie, should_be_secure};
use crate::extract::{Auth, SESSION_COOKIE};
use crate::problem::Problem;
use crate::routes::setup::{host, user_agent};
use crate::state::AppState;

/// Rappresentazione pubblica dell'utente. Non contiene l'hash della password
/// né il segreto TOTP: quei campi non lasciano mai il database.
#[derive(Serialize)]
pub struct UserView {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub email: Option<String>,
    pub role: &'static str,
    pub locale: Option<String>,
}

impl From<&User> for UserView {
    fn from(u: &User) -> Self {
        Self {
            id: u.id.to_string(),
            username: u.username.as_str().to_owned(),
            display_name: u.display_name.clone(),
            email: u.email.clone(),
            role: match u.role {
                SystemRole::Admin => "admin",
                SystemRole::User => "user",
            },
            locale: u.locale.clone(),
        }
    }
}

#[derive(Deserialize)]
pub struct LoginRequest {
    username: String,
    password: String,
}

#[derive(Serialize)]
pub struct LoginResponse {
    user: UserView,
}

/// # Errors
/// `401 invalid-credentials` per utente inesistente, password errata o account
/// disabilitato: le tre situazioni sono indistinguibili dall'esterno.
pub async fn login(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Json(req): Json<LoginRequest>,
) -> Result<impl IntoResponse, Problem> {
    let invalid = || {
        Problem::new(StatusCode::UNAUTHORIZED, "invalid-credentials", "Invalid credentials")
    };

    let username = Username::parse(&req.username).map_err(|_| invalid())?;
    let password = Password::parse(&req.password).map_err(|_| invalid())?;

    let found = UserRepo::new(&state.db).find_by_username(&username).await?;
    let Some((user, hash)) = found else {
        // Verifica fittizia per non far trapelare l'esistenza dell'utente
        // dal tempo di risposta.
        let _ = verify_password(&password, &dummy_hash());
        return Err(invalid());
    };

    if !verify_password(&password, &hash) || !user.is_active() {
        return Err(invalid());
    }

    let token = SessionRepo::new(&state.db)
        .create(user.id, state.session_ttl, user_agent(&headers))
        .await?;

    let secure = should_be_secure(host(&headers));
    let jar = jar.add(session_cookie(&token, state.session_ttl, secure));

    Ok((StatusCode::OK, jar, Json(LoginResponse { user: UserView::from(&user) })))
}

/// Hash costante usato solo per pareggiare i tempi di risposta.
fn dummy_hash() -> keeppix_domain::PasswordHash {
    keeppix_domain::PasswordHash::from_stored(
        "$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHR2YWx1ZQ$\
         0000000000000000000000000000000000000000000"
            .to_owned(),
    )
}

/// # Errors
/// `401 unauthenticated` se il cookie manca, è scaduto o è stato riusato dopo
/// il consumo — in quest'ultimo caso l'intera famiglia è già stata revocata.
pub async fn refresh(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
) -> Result<impl IntoResponse, Problem> {
    let cookie = jar.get(SESSION_COOKIE).ok_or_else(Problem::unauthenticated)?;
    let token = SessionToken::from_string(cookie.value().to_owned());

    let next = SessionRepo::new(&state.db)
        .rotate(&token, state.session_ttl)
        .await
        .map_err(|_| Problem::unauthenticated())?;

    let secure = should_be_secure(host(&headers));
    let jar = jar.add(session_cookie(&next, state.session_ttl, secure));

    Ok((StatusCode::NO_CONTENT, jar))
}

/// Sempre `204`, anche senza cookie: uscire deve funzionare comunque.
pub async fn logout(State(state): State<AppState>, jar: CookieJar) -> impl IntoResponse {
    if let Some(cookie) = jar.get(SESSION_COOKIE) {
        let token = SessionToken::from_string(cookie.value().to_owned());
        if let Err(e) = SessionRepo::new(&state.db).revoke(&token).await {
            tracing::warn!(error = %e, "revoca sessione fallita");
        }
    }
    (StatusCode::NO_CONTENT, jar.add(clearing_cookie()))
}

#[derive(Serialize)]
pub struct MeResponse {
    user: UserView,
}

/// # Errors
/// `401` se non autenticato, `404` se l'utente è stato nel frattempo rimosso.
pub async fn me(
    State(state): State<AppState>,
    Auth(ctx): Auth,
) -> Result<Json<MeResponse>, Problem> {
    let id = ctx.user_id().ok_or_else(Problem::unauthenticated)?;
    let user = UserRepo::new(&state.db).find_by_id(&ctx, id).await?;
    Ok(Json(MeResponse { user: UserView::from(&user) }))
}

/// Riesportato per gli handler che devono costruire un contesto a mano.
pub type Ctx = AuthContext;
```

- [ ] **Step 8: Montare le rotte**

`routes/mod.rs`:

```rust
pub mod auth;
pub mod health;
pub mod setup;
```

In `lib.rs`, sostituire `base_router` e `base_router_stateless`:

```rust
fn api_routes() -> Router<AppState> {
    Router::new()
        .route("/setup/status", get(routes::setup::status))
        .route("/setup", axum::routing::post(routes::setup::create))
        .route("/auth/login", axum::routing::post(routes::auth::login))
        .route("/auth/refresh", axum::routing::post(routes::auth::refresh))
        .route("/auth/logout", axum::routing::post(routes::auth::logout))
        .route("/auth/me", get(routes::auth::me))
}

fn base_router() -> Router<AppState> {
    common_layers(
        Router::new()
            .route("/health", get(routes::health::get))
            .nest("/api/v1", api_routes()),
    )
}
```

E aggiungere `pub mod cookie;` in cima a `lib.rs`.

`base_router_stateless` resta com'è: serve solo ai test di `/health`.

- [ ] **Step 9: Eseguire i test**

Run: `cargo test -p keeppix-api`
Expected: PASS — 13 test (3 di health, 10 di auth).

- [ ] **Step 10: Verificare i lint su tutto il workspace**

Run: `cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --check`
Expected: nessun warning.

- [ ] **Step 11: Commit**

```bash
git add crates/keeppix-api
git commit -m "feat(api): add first-run setup and session authentication"
```

---

## Task 11: Specifica OpenAPI

La specifica è ciò da cui verrà generato il client mobile: nasce dal codice, così non può divergere.

**Files:**
- Create: `crates/keeppix-api/src/openapi.rs`
- Create: `crates/keeppix-api/tests/openapi.rs`
- Modify: `crates/keeppix-api/src/routes/setup.rs`, `crates/keeppix-api/src/routes/auth.rs`, `crates/keeppix-api/src/lib.rs`

**Interfaces:**
- Consumes: gli handler del Task 10.
- Produces: `openapi::ApiDoc` con `ApiDoc::openapi() -> utoipa::openapi::OpenApi`, servita da `GET /api/openapi.json`.

- [ ] **Step 1: Aggiungere le dipendenze**

```bash
cargo add utoipa --features axum_extras,chrono,uuid -p keeppix-api
```

- [ ] **Step 2: Scrivere il test che fallisce**

`crates/keeppix-api/tests/openapi.rs`:

```rust
use axum::body::Body;
use axum::http::Request;
use tower::ServiceExt as _;

#[tokio::test]
async fn openapi_document_is_served_and_complete() {
    let response = keeppix_api::router_without_state()
        .oneshot(Request::builder().uri("/api/openapi.json").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let body = http_body_util::BodyExt::collect(response.into_body())
        .await
        .unwrap()
        .to_bytes();
    let doc: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(doc["openapi"].as_str().unwrap(), "3.1.0");
    assert_eq!(doc["info"]["title"], "Keeppix API");

    for path in [
        "/api/v1/setup/status",
        "/api/v1/setup",
        "/api/v1/auth/login",
        "/api/v1/auth/refresh",
        "/api/v1/auth/logout",
        "/api/v1/auth/me",
    ] {
        assert!(doc["paths"][path].is_object(), "manca il percorso {path}");
    }

    assert!(doc["components"]["schemas"]["UserView"].is_object());
}
```

- [ ] **Step 3: Eseguire e verificare il fallimento**

Run: `cargo test -p keeppix-api --test openapi`
Expected: FAIL — 404 su `/api/openapi.json`.

- [ ] **Step 4: Annotare i tipi e gli handler**

In `routes/auth.rs`, aggiungere `ToSchema` alle strutture pubbliche:

```rust
#[derive(Serialize, utoipa::ToSchema)]
pub struct UserView { /* campi invariati */ }

#[derive(Deserialize, utoipa::ToSchema)]
pub struct LoginRequest { /* invariato */ }

#[derive(Serialize, utoipa::ToSchema)]
pub struct LoginResponse { /* invariato */ }

#[derive(Serialize, utoipa::ToSchema)]
pub struct MeResponse { /* invariato */ }
```

E annotare gli handler, per esempio `login`:

```rust
#[utoipa::path(
    post,
    path = "/api/v1/auth/login",
    tag = "auth",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Sessione aperta", body = LoginResponse),
        (status = 401, description = "Credenziali non valide")
    )
)]
pub async fn login(/* invariato */) { /* invariato */ }
```

Ripetere per `refresh` (204/401), `logout` (204), `me` (200 `MeResponse` / 401), `setup::status` (200 `SetupStatus`), `setup::create` (201 `SetupResponse` / 409 / 422). Aggiungere `ToSchema` anche a `SetupStatus`, `SetupRequest`, `SetupResponse`.

- [ ] **Step 5: Implementare `openapi.rs`**

```rust
use utoipa::OpenApi;

use crate::routes::{auth, setup};

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Keeppix API",
        version = env!("CARGO_PKG_VERSION"),
        description = "API di Keeppix. Contratto congelato: solo aggiunte entro /api/v1."
    ),
    paths(
        setup::status,
        setup::create,
        auth::login,
        auth::refresh,
        auth::logout,
        auth::me,
    ),
    components(schemas(
        auth::UserView,
        auth::LoginRequest,
        auth::LoginResponse,
        auth::MeResponse,
        setup::SetupStatus,
        setup::SetupRequest,
        setup::SetupResponse,
    )),
    tags(
        (name = "setup", description = "Configurazione iniziale dell'istanza"),
        (name = "auth", description = "Autenticazione e sessioni")
    )
)]
pub struct ApiDoc;

pub async fn serve() -> axum::Json<utoipa::openapi::OpenApi> {
    axum::Json(ApiDoc::openapi())
}
```

- [ ] **Step 6: Montare la rotta in `lib.rs`**

Aggiungere `pub mod openapi;` e, in **entrambi** `base_router` e `base_router_stateless`, la rotta:

```rust
.route("/api/openapi.json", get(openapi::serve))
```

- [ ] **Step 7: Eseguire i test**

Run: `cargo test -p keeppix-api --test openapi`
Expected: PASS.

- [ ] **Step 8: Congelare la specifica per il controllo di compatibilità**

```bash
mkdir -p docs/api
cargo run --bin keeppix -- --help >/dev/null 2>&1 || true
cargo test -p keeppix-api --test openapi -- --nocapture >/dev/null
```

Aggiungere un piccolo test che scrive il file quando manca e lo confronta quando esiste, in `crates/keeppix-api/tests/openapi.rs`:

```rust
/// Blocca la specifica su disco. Se cambia, il test fallisce e mostra il diff:
/// aggiornare `docs/api/openapi.json` è una scelta consapevole, non un effetto
/// collaterale di un refactoring.
#[test]
fn openapi_snapshot_matches_the_committed_file() {
    let generated = serde_json::to_string_pretty(
        &<keeppix_api::openapi::ApiDoc as utoipa::OpenApi>::openapi(),
    )
    .unwrap();

    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/api/openapi.json");

    if !path.exists() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &generated).unwrap();
        return;
    }

    let committed = std::fs::read_to_string(&path).unwrap();
    assert_eq!(
        committed.trim(),
        generated.trim(),
        "la specifica è cambiata: rigenerare con `rm docs/api/openapi.json && cargo test`"
    );
}
```

- [ ] **Step 9: Generare e verificare il file**

Run: `cargo test -p keeppix-api --test openapi && cargo test -p keeppix-api --test openapi`
Expected: la prima esecuzione crea `docs/api/openapi.json`, la seconda lo trova identico.

- [ ] **Step 10: Commit**

```bash
git add crates/keeppix-api docs/api
git commit -m "feat(api): generate and freeze the openapi 3.1 document"
```

---

## Task 12: Frontend

**Files:**
- Create: `frontend/package.json`, `frontend/vite.config.ts`, `frontend/tsconfig.json`, `frontend/index.html`
- Create: `frontend/src/main.ts`, `frontend/src/App.vue`, `frontend/src/style.css`, `frontend/src/router.ts`
- Create: `frontend/src/api/client.ts`, `frontend/src/api/auth.ts`
- Create: `frontend/src/i18n/index.ts`, `frontend/src/i18n/it.json`, `frontend/src/i18n/en.json`
- Create: `frontend/src/stores/session.ts`
- Create: `frontend/src/components/ui/Button.vue`, `TextField.vue`, `Alert.vue`
- Create: `frontend/src/views/SetupView.vue`, `LoginView.vue`, `HomeView.vue`
- Create: `frontend/src/api/client.spec.ts`, `frontend/src/i18n/i18n.spec.ts`

**Interfaces:**
- Consumes: gli endpoint del Task 10.
- Produces:
  - `apiFetch<T>(path: string, init?: RequestInit): Promise<T>` — lancia `ApiProblem { type, title, status, detail? }` sugli errori.
  - `useSessionStore()` (Pinia) con `user`, `initialised`, `bootstrap()`, `login(username, password)`, `setup(payload)`, `logout()`.
  - Rotte: `/setup`, `/login`, `/` (protetta).

- [ ] **Step 1: Creare il progetto e installare le dipendenze**

```bash
cd Keeppix
npm create vite@latest frontend -- --template vue-ts
cd frontend
npm install
npm install vue-router pinia vue-i18n@11 @intlify/core-base reka-ui
npm install -D tailwindcss @tailwindcss/vite vitest @vue/test-utils jsdom \
  eslint eslint-plugin-vue @vue/eslint-config-typescript vue-tsc
```

- [ ] **Step 2: Configurare Vite**

`frontend/vite.config.ts`:

```ts
import { fileURLToPath, URL } from 'node:url'

import tailwindcss from '@tailwindcss/vite'
import vue from '@vitejs/plugin-vue'
// `vitest/config` invece di `vite`: è ciò che rende tipizzata la chiave `test`.
import { defineConfig } from 'vitest/config'

export default defineConfig({
  plugins: [vue(), tailwindcss()],
  resolve: {
    alias: { '@': fileURLToPath(new URL('./src', import.meta.url)) }
  },
  build: {
    // Il budget di 150 KB gzip è verificato in CI; qui si avvisa prima.
    chunkSizeWarningLimit: 400
  },
  server: {
    // In sviluppo il frontend gira su 5173 e inoltra le API al backend.
    proxy: {
      '/api': { target: 'http://127.0.0.1:5673', changeOrigin: true },
      '/health': { target: 'http://127.0.0.1:5673', changeOrigin: true }
    }
  },
  test: {
    environment: 'jsdom'
  }
})
```

- [ ] **Step 3: Configurare Tailwind v4**

`frontend/src/style.css`:

```css
@import "tailwindcss";

@theme {
  --color-surface: oklch(99% 0 0);
  --color-surface-elevated: oklch(100% 0 0);
  --color-content: oklch(20% 0 0);
  --color-content-muted: oklch(50% 0 0);
  --color-accent: oklch(58% 0.19 258);
  --color-danger: oklch(55% 0.20 25);
  --color-border: oklch(90% 0 0);
}

@media (prefers-color-scheme: dark) {
  @theme {
    --color-surface: oklch(17% 0 0);
    --color-surface-elevated: oklch(22% 0 0);
    --color-content: oklch(95% 0 0);
    --color-content-muted: oklch(65% 0 0);
    --color-border: oklch(30% 0 0);
  }
}

html, body, #app { height: 100%; }
body { background: var(--color-surface); color: var(--color-content); }

/* Rispetta chi ha ridotto le animazioni a livello di sistema. */
@media (prefers-reduced-motion: reduce) {
  *, *::before, *::after {
    animation-duration: 0.01ms !important;
    transition-duration: 0.01ms !important;
  }
}
```

- [ ] **Step 4: Scrivere i test che falliscono**

`frontend/src/api/client.spec.ts`:

```ts
import { afterEach, describe, expect, it, vi } from 'vitest'

import { ApiProblem, apiFetch } from './client'

afterEach(() => vi.unstubAllGlobals())

function mockResponse(status: number, body: unknown, contentType: string) {
  vi.stubGlobal(
    'fetch',
    vi.fn(async () =>
      new Response(JSON.stringify(body), {
        status,
        headers: { 'content-type': contentType }
      })
    )
  )
}

describe('apiFetch', () => {
  it('restituisce il corpo su risposta positiva', async () => {
    mockResponse(200, { user: { username: 'giovanni' } }, 'application/json')
    await expect(apiFetch('/api/v1/auth/me')).resolves.toEqual({
      user: { username: 'giovanni' }
    })
  })

  it('lancia ApiProblem con il codice stabile', async () => {
    mockResponse(
      401,
      { type: 'keeppix/invalid-credentials', title: 'Invalid credentials', status: 401 },
      'application/problem+json'
    )

    await expect(apiFetch('/api/v1/auth/login')).rejects.toMatchObject({
      type: 'keeppix/invalid-credentials',
      status: 401
    })
  })

  it('lancia ApiProblem generico se il corpo non è problem+json', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => new Response('boom', { status: 502 })))

    const error = await apiFetch('/api/v1/auth/me').catch((e: unknown) => e)
    expect(error).toBeInstanceOf(ApiProblem)
    expect((error as ApiProblem).status).toBe(502)
  })

  it('restituisce null su 204', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => new Response(null, { status: 204 })))
    await expect(apiFetch('/api/v1/auth/refresh')).resolves.toBeNull()
  })
})
```

`frontend/src/i18n/i18n.spec.ts`:

```ts
import { describe, expect, it } from 'vitest'

import en from './en.json'
import it from './it.json'

/// Appiattisce un oggetto annidato in un elenco di chiavi puntate.
function keys(obj: Record<string, unknown>, prefix = ''): string[] {
  return Object.entries(obj).flatMap(([k, v]) =>
    typeof v === 'object' && v !== null
      ? keys(v as Record<string, unknown>, `${prefix}${k}.`)
      : [`${prefix}${k}`]
  )
}

describe('traduzioni', () => {
  it('italiano e inglese hanno le stesse chiavi', () => {
    const itKeys = keys(it).sort()
    const enKeys = keys(en).sort()
    expect(itKeys).toEqual(enKeys)
  })

  it('nessuna traduzione è vuota', () => {
    for (const [locale, messages] of [['it', it], ['en', en]] as const) {
      for (const key of keys(messages)) {
        const value = key.split('.').reduce<unknown>(
          (acc, part) => (acc as Record<string, unknown>)[part],
          messages
        )
        expect(value, `${locale}.${key}`).not.toBe('')
      }
    }
  })
})
```

- [ ] **Step 5: Eseguire e verificare il fallimento**

Run: `cd frontend && npx vitest run`
Expected: FAIL — `Cannot find module './client'`.

- [ ] **Step 6: Implementare `src/api/client.ts`**

```ts
/** Errore RFC 9457. `type` è il codice stabile su cui ramificare. */
export class ApiProblem extends Error {
  constructor(
    readonly type: string,
    readonly title: string,
    readonly status: number,
    readonly detail?: string
  ) {
    super(`${type}: ${title}`)
    this.name = 'ApiProblem'
  }
}

/**
 * Chiamata JSON verso l'API. Invia sempre i cookie e l'header custom che
 * il backend richiede sulle mutazioni: un form HTML esterno non può produrlo,
 * quindi copre la protezione CSRF insieme a SameSite=Lax.
 */
export async function apiFetch<T = unknown>(
  path: string,
  init: RequestInit = {}
): Promise<T> {
  const response = await fetch(path, {
    ...init,
    credentials: 'same-origin',
    headers: {
      'content-type': 'application/json',
      'x-keeppix-client': 'web',
      ...(init.headers ?? {})
    }
  })

  if (response.status === 204) {
    return null as T
  }

  if (!response.ok) {
    const contentType = response.headers.get('content-type') ?? ''
    if (contentType.includes('application/problem+json')) {
      const problem = await response.json()
      throw new ApiProblem(problem.type, problem.title, problem.status, problem.detail)
    }
    throw new ApiProblem('keeppix/unexpected', response.statusText, response.status)
  }

  return (await response.json()) as T
}
```

- [ ] **Step 7: Implementare le traduzioni**

`frontend/src/i18n/it.json`:

```json
{
  "app": { "name": "Keeppix" },
  "setup": {
    "title": "Benvenuto in Keeppix",
    "subtitle": "Crea l'account amministratore per iniziare.",
    "displayName": "Nome",
    "username": "Nome utente",
    "email": "Email (facoltativa)",
    "password": "Password",
    "passwordHint": "Almeno 10 caratteri.",
    "submit": "Crea account",
    "errors": {
      "invalidUsername": "Nome utente non valido: usa da 3 a 32 caratteri fra lettere, numeri, punto, trattino e trattino basso.",
      "invalidPassword": "La password deve avere almeno 10 caratteri.",
      "alreadyInitialised": "Questa istanza è già configurata."
    }
  },
  "login": {
    "title": "Accedi",
    "username": "Nome utente",
    "password": "Password",
    "submit": "Accedi",
    "errors": { "invalidCredentials": "Nome utente o password non corretti." }
  },
  "home": { "greeting": "Ciao, {name}", "logout": "Esci" },
  "common": { "loading": "Caricamento…", "unexpectedError": "Si è verificato un errore imprevisto." }
}
```

`frontend/src/i18n/en.json`:

```json
{
  "app": { "name": "Keeppix" },
  "setup": {
    "title": "Welcome to Keeppix",
    "subtitle": "Create the administrator account to get started.",
    "displayName": "Name",
    "username": "Username",
    "email": "Email (optional)",
    "password": "Password",
    "passwordHint": "At least 10 characters.",
    "submit": "Create account",
    "errors": {
      "invalidUsername": "Invalid username: use 3 to 32 characters from letters, digits, dot, hyphen and underscore.",
      "invalidPassword": "The password must be at least 10 characters long.",
      "alreadyInitialised": "This instance is already set up."
    }
  },
  "login": {
    "title": "Sign in",
    "username": "Username",
    "password": "Password",
    "submit": "Sign in",
    "errors": { "invalidCredentials": "Incorrect username or password." }
  },
  "home": { "greeting": "Hello, {name}", "logout": "Sign out" },
  "common": { "loading": "Loading…", "unexpectedError": "An unexpected error occurred." }
}
```

`frontend/src/i18n/index.ts`:

```ts
import { createI18n } from 'vue-i18n'

import en from './en.json'
import it from './it.json'

const SUPPORTED = ['it', 'en'] as const
export type Locale = (typeof SUPPORTED)[number]

const STORAGE_KEY = 'keeppix.locale'

/** Nessuna lingua predefinita: si rileva, poi vince la scelta esplicita. */
export function detectLocale(): Locale {
  const stored = localStorage.getItem(STORAGE_KEY)
  if (stored && SUPPORTED.includes(stored as Locale)) {
    return stored as Locale
  }
  const preferred = navigator.languages ?? [navigator.language]
  for (const tag of preferred) {
    const base = tag.split('-')[0]
    if (SUPPORTED.includes(base as Locale)) {
      return base as Locale
    }
  }
  return 'en'
}

export function setLocale(locale: Locale): void {
  localStorage.setItem(STORAGE_KEY, locale)
  i18n.global.locale.value = locale
  document.documentElement.lang = locale
}

export const i18n = createI18n({
  legacy: false,
  locale: detectLocale(),
  fallbackLocale: 'en',
  messages: { it, en }
})
```

- [ ] **Step 8: Eseguire i test del frontend**

Run: `cd frontend && npx vitest run`
Expected: PASS — 6 test.

- [ ] **Step 9: Implementare lo store di sessione**

`frontend/src/stores/session.ts`:

```ts
import { defineStore } from 'pinia'
import { ref } from 'vue'

import { ApiProblem, apiFetch } from '@/api/client'

export interface User {
  id: string
  username: string
  display_name: string
  email: string | null
  role: 'admin' | 'user'
  locale: string | null
}

export const useSessionStore = defineStore('session', () => {
  const user = ref<User | null>(null)
  const initialised = ref<boolean | null>(null)
  const ready = ref(false)

  /** Determina lo stato dell'istanza e ripristina la sessione se presente. */
  async function bootstrap(): Promise<void> {
    const status = await apiFetch<{ initialised: boolean }>('/api/v1/setup/status')
    initialised.value = status.initialised

    if (status.initialised) {
      try {
        const me = await apiFetch<{ user: User }>('/api/v1/auth/me')
        user.value = me.user
      } catch (error) {
        // 401 è normale: nessuna sessione attiva.
        if (!(error instanceof ApiProblem) || error.status !== 401) throw error
        user.value = null
      }
    }
    ready.value = true
  }

  async function login(username: string, password: string): Promise<void> {
    const result = await apiFetch<{ user: User }>('/api/v1/auth/login', {
      method: 'POST',
      body: JSON.stringify({ username, password })
    })
    user.value = result.user
  }

  async function setup(payload: {
    username: string
    display_name: string
    email?: string
    password: string
  }): Promise<void> {
    const result = await apiFetch<{ user: User }>('/api/v1/setup', {
      method: 'POST',
      body: JSON.stringify(payload)
    })
    user.value = result.user
    initialised.value = true
  }

  async function logout(): Promise<void> {
    await apiFetch('/api/v1/auth/logout', { method: 'POST' })
    user.value = null
  }

  return { user, initialised, ready, bootstrap, login, setup, logout }
})
```

- [ ] **Step 10: Implementare i componenti UI**

`frontend/src/components/ui/Button.vue`:

```vue
<script setup lang="ts">
defineProps<{ type?: 'button' | 'submit'; disabled?: boolean; loading?: boolean }>()
</script>

<template>
  <button
    :type="type ?? 'button'"
    :disabled="disabled || loading"
    class="w-full rounded-lg bg-accent px-4 py-2.5 font-medium text-white
           transition-opacity hover:opacity-90 focus-visible:outline-2
           focus-visible:outline-offset-2 focus-visible:outline-accent
           disabled:opacity-50"
  >
    <slot />
  </button>
</template>
```

`frontend/src/components/ui/TextField.vue`:

```vue
<script setup lang="ts">
import { useId } from 'vue'

defineProps<{ label: string; type?: string; hint?: string; autocomplete?: string; required?: boolean }>()
const model = defineModel<string>({ required: true })
const id = useId()
</script>

<template>
  <div class="flex flex-col gap-1.5">
    <label :for="id" class="text-sm font-medium text-content">{{ label }}</label>
    <input
      :id="id"
      v-model="model"
      :type="type ?? 'text'"
      :autocomplete="autocomplete"
      :required="required"
      :aria-describedby="hint ? `${id}-hint` : undefined"
      class="rounded-lg border border-border bg-surface-elevated px-3 py-2.5
             text-content focus-visible:outline-2 focus-visible:outline-accent"
    />
    <p v-if="hint" :id="`${id}-hint`" class="text-xs text-content-muted">{{ hint }}</p>
  </div>
</template>
```

`frontend/src/components/ui/Alert.vue`:

```vue
<script setup lang="ts">
defineProps<{ message: string }>()
</script>

<template>
  <p role="alert" class="rounded-lg bg-danger/10 px-3 py-2 text-sm text-danger">
    {{ message }}
  </p>
</template>
```

- [ ] **Step 11: Implementare le viste**

`frontend/src/views/LoginView.vue`:

```vue
<script setup lang="ts">
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'

import { ApiProblem } from '@/api/client'
import Alert from '@/components/ui/Alert.vue'
import Button from '@/components/ui/Button.vue'
import TextField from '@/components/ui/TextField.vue'
import { useSessionStore } from '@/stores/session'

const { t } = useI18n()
const router = useRouter()
const session = useSessionStore()

const username = ref('')
const password = ref('')
const error = ref('')
const loading = ref(false)

async function submit() {
  error.value = ''
  loading.value = true
  try {
    await session.login(username.value, password.value)
    await router.push('/')
  } catch (e) {
    error.value =
      e instanceof ApiProblem && e.type === 'keeppix/invalid-credentials'
        ? t('login.errors.invalidCredentials')
        : t('common.unexpectedError')
  } finally {
    loading.value = false
  }
}
</script>

<template>
  <main class="mx-auto flex min-h-full w-full max-w-sm flex-col justify-center gap-6 p-6">
    <h1 class="text-2xl font-semibold">{{ t('login.title') }}</h1>
    <form class="flex flex-col gap-4" @submit.prevent="submit">
      <TextField v-model="username" :label="t('login.username')" autocomplete="username" required />
      <TextField
        v-model="password"
        :label="t('login.password')"
        type="password"
        autocomplete="current-password"
        required
      />
      <Alert v-if="error" :message="error" />
      <Button type="submit" :loading="loading">{{ t('login.submit') }}</Button>
    </form>
  </main>
</template>
```

`frontend/src/views/SetupView.vue`:

```vue
<script setup lang="ts">
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'

import { ApiProblem } from '@/api/client'
import Alert from '@/components/ui/Alert.vue'
import Button from '@/components/ui/Button.vue'
import TextField from '@/components/ui/TextField.vue'
import { useSessionStore } from '@/stores/session'

const { t } = useI18n()
const router = useRouter()
const session = useSessionStore()

const displayName = ref('')
const username = ref('')
const email = ref('')
const password = ref('')
const error = ref('')
const loading = ref(false)

/** Il backend restituisce codici stabili: la traduzione avviene qui. */
function messageFor(e: unknown): string {
  if (!(e instanceof ApiProblem)) return t('common.unexpectedError')
  const known: Record<string, string> = {
    'keeppix/invalid-username': t('setup.errors.invalidUsername'),
    'keeppix/invalid-password': t('setup.errors.invalidPassword'),
    'keeppix/already-initialised': t('setup.errors.alreadyInitialised')
  }
  return known[e.type] ?? t('common.unexpectedError')
}

async function submit() {
  error.value = ''
  loading.value = true
  try {
    await session.setup({
      username: username.value,
      display_name: displayName.value,
      email: email.value || undefined,
      password: password.value
    })
    await router.push('/')
  } catch (e) {
    error.value = messageFor(e)
  } finally {
    loading.value = false
  }
}
</script>

<template>
  <main class="mx-auto flex min-h-full w-full max-w-sm flex-col justify-center gap-6 p-6">
    <header class="flex flex-col gap-1">
      <h1 class="text-2xl font-semibold">{{ t('setup.title') }}</h1>
      <p class="text-sm text-content-muted">{{ t('setup.subtitle') }}</p>
    </header>

    <form class="flex flex-col gap-4" @submit.prevent="submit">
      <TextField v-model="displayName" :label="t('setup.displayName')" autocomplete="name" required />
      <TextField v-model="username" :label="t('setup.username')" autocomplete="username" required />
      <TextField v-model="email" :label="t('setup.email')" type="email" autocomplete="email" />
      <TextField
        v-model="password"
        :label="t('setup.password')"
        :hint="t('setup.passwordHint')"
        type="password"
        autocomplete="new-password"
        required
      />
      <Alert v-if="error" :message="error" />
      <Button type="submit" :loading="loading">{{ t('setup.submit') }}</Button>
    </form>
  </main>
</template>
```

`frontend/src/views/HomeView.vue`:

```vue
<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'

import Button from '@/components/ui/Button.vue'
import { useSessionStore } from '@/stores/session'

const { t } = useI18n()
const router = useRouter()
const session = useSessionStore()

async function signOut() {
  await session.logout()
  await router.push('/login')
}
</script>

<template>
  <main class="mx-auto flex min-h-full w-full max-w-2xl flex-col gap-6 p-6">
    <h1 class="text-2xl font-semibold">
      {{ t('home.greeting', { name: session.user?.display_name ?? '' }) }}
    </h1>
    <div class="max-w-xs">
      <Button @click="signOut">{{ t('home.logout') }}</Button>
    </div>
  </main>
</template>
```

- [ ] **Step 12: Implementare router e bootstrap**

`frontend/src/router.ts`:

```ts
import { createRouter, createWebHistory } from 'vue-router'

import { useSessionStore } from '@/stores/session'

export const router = createRouter({
  history: createWebHistory(),
  routes: [
    { path: '/', component: () => import('@/views/HomeView.vue'), meta: { auth: true } },
    { path: '/login', component: () => import('@/views/LoginView.vue') },
    { path: '/setup', component: () => import('@/views/SetupView.vue') },
    { path: '/:pathMatch(.*)*', redirect: '/' }
  ]
})

router.beforeEach(async (to) => {
  const session = useSessionStore()
  if (!session.ready) {
    await session.bootstrap()
  }

  // Istanza vergine: qualsiasi percorso porta al setup.
  if (session.initialised === false) {
    return to.path === '/setup' ? true : '/setup'
  }
  if (to.path === '/setup') {
    return '/'
  }
  if (to.meta.auth && !session.user) {
    return '/login'
  }
  if (to.path === '/login' && session.user) {
    return '/'
  }
  return true
})
```

`frontend/src/App.vue`:

```vue
<template>
  <RouterView />
</template>
```

`frontend/src/main.ts`:

```ts
import { createPinia } from 'pinia'
import { createApp } from 'vue'

import App from './App.vue'
import { i18n, detectLocale } from './i18n'
import { router } from './router'
import './style.css'

document.documentElement.lang = detectLocale()

createApp(App).use(createPinia()).use(i18n).use(router).mount('#app')
```

- [ ] **Step 13: Verificare tipi, lint, test e build**

Run: `cd frontend && npx vue-tsc --noEmit && npx vitest run && npm run build`
Expected: nessun errore di tipo, 6 test verdi, build completata.

- [ ] **Step 14: Verificare il budget di bundle**

```bash
cd frontend && find dist/assets -name '*.js' -exec gzip -c {} \; | wc -c
```

Expected: sotto **153600** byte. Se sforato, spostare le viste in import dinamici (già fatto nel router) e verificare che Reka UI non sia importato interamente.

- [ ] **Step 15: Provare il flusso completo a mano**

Con il backend in esecuzione (Task 9, Step 10) e `npm run dev` nel frontend, aprire `http://127.0.0.1:5173`:
1. Si viene rediretti a `/setup`.
2. Creare l'admin → si arriva a `/` con il saluto.
3. Ricaricare → si resta autenticati.
4. Uscire → si torna a `/login`.
5. Rientrare con le credenziali corrette.

- [ ] **Step 16: Commit**

```bash
git add frontend
git commit -m "feat(frontend): add vue app with setup, login and i18n"
```

---

## Task 13: Frontend incorporato nel binario

**Files:**
- Create: `crates/keeppix-server/src/embed.rs`
- Create: `crates/keeppix-server/tests/embed.rs`
- Modify: `crates/keeppix-server/src/lib.rs`, `crates/keeppix-server/src/main.rs`, `crates/keeppix-server/Cargo.toml`

**Interfaces:**
- Consumes: `router(state)` (Task 9-11); `frontend/dist` (Task 12).
- Produces: `embed::spa_fallback() -> axum::routing::MethodRouter` e `embed::mount(router: Router<AppState>) -> Router<AppState>`.

Comportamento: `/assets/*` servito con `Cache-Control: immutable` (i nomi contengono l'hash del contenuto), `index.html` con `no-cache`, e qualunque percorso non-API restituisce `index.html` perché il routing è lato client. I percorsi sotto `/api` non ricadono mai nel fallback: devono restituire `404 problem+json`.

- [ ] **Step 1: Aggiungere le dipendenze**

```bash
cargo add rust-embed --features interpolate-folder-path -p keeppix-server
cargo add mime_guess -p keeppix-server
```

- [ ] **Step 2: Scrivere i test che falliscono**

`crates/keeppix-server/tests/embed.rs`:

```rust
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt as _;

/// Il test gira solo quando il frontend è stato compilato: in CI la build del
/// frontend precede quella del backend.
fn frontend_built() -> bool {
    std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/dist/index.html"))
        .exists()
}

#[tokio::test]
async fn index_is_served_at_root() {
    if !frontend_built() {
        eprintln!("frontend/dist assente: test saltato");
        return;
    }

    let app = keeppix_server::embed::mount_stateless();
    let response = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers().get("cache-control").unwrap(), "no-cache");
}

#[tokio::test]
async fn client_routes_fall_back_to_index() {
    if !frontend_built() {
        return;
    }

    let app = keeppix_server::embed::mount_stateless();
    let response = app
        .oneshot(Request::builder().uri("/login").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK, "il routing è lato client");
}

#[tokio::test]
async fn api_paths_never_fall_back_to_index() {
    if !frontend_built() {
        return;
    }

    let app = keeppix_server::embed::mount_stateless();
    let response = app
        .oneshot(Request::builder().uri("/api/v1/nope").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        response.headers().get("content-type").unwrap(),
        "application/problem+json",
        "un client API non deve mai ricevere HTML"
    );
}
```

- [ ] **Step 3: Eseguire e verificare il fallimento**

Run: `cd frontend && npm run build && cd .. && cargo test -p keeppix-server --test embed`
Expected: FAIL — `cannot find module embed`.

- [ ] **Step 4: Implementare `embed.rs`**

```rust
use axum::body::Body;
use axum::http::{HeaderValue, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use keeppix_api::{AppState, Problem};
use rust_embed::Embed;

#[derive(Embed)]
#[folder = "$CARGO_MANIFEST_DIR/../../frontend/dist"]
struct Assets;

/// Serve un file incorporato oppure `index.html` come fallback SPA.
/// I percorsi API non arrivano qui: sono registrati prima nel router.
async fn serve(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');

    // Difesa in profondità: se un giorno l'ordine delle rotte cambiasse,
    // un client API non deve comunque ricevere HTML.
    if path.starts_with("api/") {
        return Problem::not_found().into_response();
    }

    if let Some(file) = Assets::get(path) {
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        // I nomi dei bundle contengono l'hash del contenuto: sono immutabili.
        let cache = if path.starts_with("assets/") {
            "public, max-age=31536000, immutable"
        } else {
            "no-cache"
        };

        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, HeaderValue::from_str(mime.as_ref()).unwrap_or(
                HeaderValue::from_static("application/octet-stream"),
            ))
            .header(header::CACHE_CONTROL, HeaderValue::from_static(cache))
            .body(Body::from(file.data.into_owned()))
            .unwrap_or_else(|_| Problem::internal().into_response());
    }

    match Assets::get("index.html") {
        Some(index) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, HeaderValue::from_static("text/html; charset=utf-8"))
            .header(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"))
            .body(Body::from(index.data.into_owned()))
            .unwrap_or_else(|_| Problem::internal().into_response()),
        None => Problem::not_found().into_response(),
    }
}

/// Aggiunge il fallback SPA a un router con stato.
#[must_use]
pub fn mount(router: axum::Router<AppState>) -> axum::Router<AppState> {
    router.fallback(get(serve))
}

/// Router minimo per i test: solo API 404 + fallback SPA.
#[must_use]
pub fn mount_stateless() -> axum::Router {
    axum::Router::new().fallback(get(serve))
}
```

- [ ] **Step 5: Esportare e montare**

In `crates/keeppix-server/src/lib.rs` aggiungere `pub mod embed;`.

In `main.rs`, sostituire la costruzione del router:

```rust
    let app = keeppix_server::embed::mount(keeppix_api::router_parts())
        .with_state(keeppix_api::AppState::new(db, config.session_ttl_secs));
```

E in `keeppix-api/src/lib.rs` separare gli strati dal fallback, sostituendo `common_layers`, `base_router` e `base_router_stateless` con:

```rust
/// Strati comuni a tutti i router. Non registra alcun fallback: chi monta
/// decide se rispondere 404 in JSON (API pura) o servire la SPA (binario).
fn common_layers<S: Clone + Send + Sync + 'static>(router: Router<S>) -> Router<S> {
    router
        .layer(SetResponseHeaderLayer::overriding(
            axum::http::header::HeaderName::from_static("x-content-type-options"),
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            axum::http::header::HeaderName::from_static("referrer-policy"),
            HeaderValue::from_static("no-referrer"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            axum::http::header::HeaderName::from_static("content-security-policy"),
            HeaderValue::from_static(CSP),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            axum::http::header::HeaderName::from_static("permissions-policy"),
            HeaderValue::from_static("camera=(), microphone=(), geolocation=()"),
        ))
        .layer(CompressionLayer::new().br(true).gzip(true))
        .layer(TraceLayer::new_for_http())
}

fn all_routes() -> Router<AppState> {
    Router::new()
        .route("/health", get(routes::health::get))
        .route("/api/openapi.json", get(openapi::serve))
        .nest("/api/v1", api_routes())
}

/// Router con tutte le rotte ma **senza** fallback: il binario aggiunge il
/// proprio, che serve il frontend incorporato.
#[must_use]
pub fn router_parts() -> Router<AppState> {
    common_layers(all_routes())
}

/// Router completo con fallback 404 in `problem+json`. Usato dai test API.
#[must_use]
pub fn router(state: AppState) -> Router {
    router_parts().fallback(not_found).with_state(state)
}

/// Router senza stato per i test che non toccano il database.
#[must_use]
pub fn router_without_state() -> Router {
    common_layers(
        Router::new()
            .route("/health", get(routes::health::get))
            .route("/api/openapi.json", get(openapi::serve)),
    )
    .fallback(not_found)
}
```

Verificare che `router_without_state` non richieda più `AppState`: le due rotte che espone (`/health` e `/api/openapi.json`) sono handler senza stato, quindi il tipo `Router` (senza parametro) è corretto.

- [ ] **Step 6: Eseguire i test**

Run: `cargo test -p keeppix-server`
Expected: PASS — 3 test di embed + 4 di config.

- [ ] **Step 7: Verificare a mano il binario completo**

```bash
cd frontend && npm run build && cd ..
DATABASE_URL=postgres://postgres:postgres@127.0.0.1:55432/postgres \
KEEPPIX_LOG_FORMAT=pretty cargo run --release --bin keeppix -- --config ./nonexistent.toml
```

Aprire `http://127.0.0.1:5673`: il frontend deve essere servito dal binario, senza Vite.

- [ ] **Step 8: Commit**

```bash
git add crates/keeppix-server crates/keeppix-api
git commit -m "feat(server): embed the frontend and add spa fallback"
```

---

## Task 14: Immagine Docker e compose

**Files:**
- Create: `Dockerfile`, `.dockerignore`, `compose.yaml`
- Create: `docs/DEPLOY.md`

**Interfaces:**
- Consumes: il binario `keeppix` e `frontend/dist`.
- Produces: immagine `keeppix:dev` avviabile con `docker compose --profile bundled up`.

- [ ] **Step 1: Scrivere `.dockerignore`**

```
target
frontend/node_modules
frontend/dist
data
pgdata
.git
docs
*.md
```

- [ ] **Step 2: Scrivere il `Dockerfile`**

```dockerfile
# syntax=docker/dockerfile:1.9

# ── Frontend ──────────────────────────────────────────────────────────────
FROM node:24-bookworm-slim AS frontend
WORKDIR /app/frontend
COPY frontend/package.json frontend/package-lock.json ./
RUN npm ci
COPY frontend/ ./
RUN npm run build

# ── Backend ───────────────────────────────────────────────────────────────
FROM rust:1.85-bookworm AS backend
WORKDIR /app

# Le query sqlx sono verificate contro la cache committata: nessun database
# è necessario in fase di build.
ENV SQLX_OFFLINE=true

# Strato di dipendenze, invalidato solo dai manifest.
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates/ crates/
COPY .sqlx/ .sqlx/
COPY --from=frontend /app/frontend/dist frontend/dist

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo build --release --bin keeppix && \
    cp target/release/keeppix /usr/local/bin/keeppix

# ── Runtime ───────────────────────────────────────────────────────────────
# distroless: nessuna shell, nessun package manager, ~6 pacchetti da monitorare.
FROM gcr.io/distroless/cc-debian12:nonroot AS runtime

COPY --from=backend /usr/local/bin/keeppix /usr/local/bin/keeppix

USER nonroot:nonroot
WORKDIR /data
EXPOSE 5673

ENV KEEPPIX_BIND=0.0.0.0:5673 \
    KEEPPIX_DATA_DIR=/data \
    KEEPPIX_LOG_FORMAT=json

# Nessun curl disponibile: si usa il sottocomando del binario stesso.
HEALTHCHECK --interval=30s --timeout=5s --start-period=20s --retries=3 \
    CMD ["/usr/local/bin/keeppix", "healthcheck"]

ENTRYPOINT ["/usr/local/bin/keeppix"]
CMD ["serve"]
```

- [ ] **Step 3: Scrivere `compose.yaml`**

```yaml
name: keeppix

services:
  keeppix:
    build: .
    image: keeppix:dev
    restart: unless-stopped
    environment:
      # Con un Postgres esterno, sostituire questo valore e omettere
      # `--profile bundled`: il servizio `db` non verrà avviato.
      DATABASE_URL: postgres://keeppix:${DB_PASSWORD:-changeme}@db/keeppix
      KEEPPIX_ALLOWED_ORIGINS: '[]'
    ports:
      - "5673:5673"
    volumes:
      - ./data:/data
      # Originali in sola lettura: nessun bug può cancellarli.
      # Passare a `rw` solo quando servirà l'upload (Fase 1).
      - ${PHOTOS_PATH:-./photos}:/photos:ro
    read_only: true
    tmpfs:
      - /tmp
    security_opt:
      - no-new-privileges:true
    cap_drop:
      - ALL
    depends_on:
      db:
        condition: service_healthy
        required: false

  db:
    profiles: ["bundled"]
    image: postgis/postgis:17-3.5
    restart: unless-stopped
    environment:
      POSTGRES_USER: keeppix
      POSTGRES_PASSWORD: ${DB_PASSWORD:-changeme}
      POSTGRES_DB: keeppix
    volumes:
      - ./pgdata:/var/lib/postgresql/data
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U keeppix -d keeppix"]
      interval: 5s
      timeout: 3s
      retries: 10
```

- [ ] **Step 4: Costruire l'immagine**

Run: `docker build -t keeppix:dev .`
Expected: build completata. Verificare la dimensione:

```bash
docker images keeppix:dev --format '{{.Size}}'
```

Expected: sotto 100 MB (in Fase 0 non c'è ancora ffmpeg).

- [ ] **Step 5: Verificare che l'immagine non abbia shell**

Run: `docker run --rm --entrypoint /bin/sh keeppix:dev -c 'echo ciao'`
Expected: errore `exec: "/bin/sh": stat /bin/sh: no such file or directory`. È il comportamento voluto.

- [ ] **Step 6: Avviare lo stack completo**

```bash
DB_PASSWORD=devpassword docker compose --profile bundled up -d --build
sleep 15
curl -s http://127.0.0.1:5673/health
curl -s http://127.0.0.1:5673/api/v1/setup/status
```

Expected: `{"status":"ok","version":"0.1.0"}` e `{"initialised":false}`.

- [ ] **Step 7: Verificare l'healthcheck del container**

Run: `docker compose ps --format '{{.Name}} {{.Status}}'`
Expected: il servizio `keeppix` riporta `(healthy)`.

- [ ] **Step 8: Verificare il flusso completo nel browser**

Aprire `http://127.0.0.1:5673`, completare il setup, uscire, rientrare. Poi:

```bash
docker compose down && DB_PASSWORD=devpassword docker compose --profile bundled up -d
```

Expected: l'istanza risulta già configurata (`initialised: true`), i dati sono sopravvissuti al riavvio.

- [ ] **Step 9: Scrivere `docs/DEPLOY.md`**

````markdown
# Installazione

## Requisiti

- Docker 24+ con Compose v2
- PostgreSQL 17 con PostGIS 3.5 (incluso, oppure esterno)
- 2 GB di RAM liberi, architettura `amd64` o `arm64`

## Avvio con tutto incluso

```bash
export DB_PASSWORD=$(openssl rand -base64 24)
docker compose --profile bundled up -d
```

Aprire http://127.0.0.1:5673 e completare la creazione dell'amministratore.

## Avvio con un Postgres già esistente

Il database deve avere l'estensione PostGIS disponibile. Omettere il profilo:

```bash
DATABASE_URL=postgres://utente:password@mio-host:5432/keeppix docker compose up -d
```

Il servizio `db` non verrà avviato.

## Variabili d'ambiente

| Variabile | Predefinito | Descrizione |
|---|---|---|
| `DATABASE_URL` | — | **Obbligatoria.** Stringa di connessione a Postgres |
| `KEEPPIX_BIND` | `0.0.0.0:5673` | Indirizzo e porta di ascolto |
| `KEEPPIX_DATA_DIR` | `/data` | Derivati, mappe, backup, `config.toml` |
| `KEEPPIX_DB_MAX_CONNECTIONS` | `10` | Dimensione del pool |
| `KEEPPIX_SESSION_TTL_SECS` | `2592000` | Durata della sessione (30 giorni) |
| `KEEPPIX_LOG_FORMAT` | `json` | `json` o `pretty` |
| `KEEPPIX_ALLOWED_ORIGINS` | `[]` | Origini ammesse, es. `["https://foto.example.com"]` |
| `RUST_LOG` | `info,sqlx=warn` | Verbosità dei log |

Le stesse chiavi sono impostabili in `/data/config.toml` in minuscolo e senza
prefisso. **L'ambiente vince sempre sul file.**

## Volumi

| Percorso | Modo | Contenuto |
|---|---|---|
| `./data` → `/data` | rw | derivati, mappe, backup, configurazione |
| `$PHOTOS_PATH` → `/photos` | **ro** | i tuoi originali |

In Fase 0 non esiste ancora l'indicizzazione: `/photos` è montato in sola
lettura e nulla lo tocca. Passerà a `rw` in Fase 1, solo per le librerie su cui
abiliterai upload o scrittura dei sidecar.

## Aggiornamento

```bash
docker compose pull && docker compose up -d
```

Le migrazioni del database vengono applicate automaticamente all'avvio, in
transazione. Il tag `:1` segue la versione major: gli aggiornamenti al suo
interno non richiedono interventi manuali.

## Dietro un reverse proxy

Keeppix parla HTTP in chiaro e si aspetta che la terminazione TLS avvenga a
monte. Il cookie di sessione usa il prefisso `__Host-`, che **richiede HTTPS**:
senza TLS l'accesso funziona solo da `localhost`.

```nginx
location / {
    proxy_pass http://127.0.0.1:5673;
    proxy_set_header Host $host;
    proxy_set_header X-Forwarded-Proto $scheme;
    proxy_http_version 1.1;
}
```

## Diagnosi

```bash
docker compose logs -f keeppix
curl -s http://127.0.0.1:5673/health
```

L'immagine è distroless e **non contiene shell**: `docker exec ... sh` non
funziona, ed è voluto. Per ispezionarla, usare il tag `:1-debug`.
````

- [ ] **Step 10: Pulire e committare**

```bash
docker compose down -v
git add Dockerfile .dockerignore compose.yaml docs/DEPLOY.md
git commit -m "feat: add distroless docker image and compose stack"
```

---

## Task 15: Integrazione continua

**Files:**
- Create: `.github/workflows/ci.yml`
- Create: `.github/workflows/release.yml`
- Create: `deny.toml`

**Interfaces:**
- Consumes: tutti i task precedenti.
- Produces: CI che blocca il merge su fmt, clippy, test, tipi frontend, budget bundle, compatibilità OpenAPI, audit delle dipendenze; e una pipeline di release che pubblica l'immagine multi-arch firmata.

- [ ] **Step 1: Scrivere `deny.toml`**

```toml
[advisories]
yanked = "deny"

[licenses]
allow = ["MIT", "Apache-2.0", "Apache-2.0 WITH LLVM-exception", "BSD-2-Clause",
         "BSD-3-Clause", "ISC", "Unicode-3.0", "Zlib", "MPL-2.0", "AGPL-3.0"]

[bans]
multiple-versions = "warn"
```

- [ ] **Step 2: Scrivere `.github/workflows/ci.yml`**

```yaml
name: CI

on:
  push: { branches: [main] }
  pull_request:

env:
  CARGO_TERM_COLOR: always
  SQLX_OFFLINE: "true"

jobs:
  backend:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@1.85.0
        with: { components: rustfmt, clippy }
      - uses: Swatinem/rust-cache@v2

      - name: Formattazione
        run: cargo fmt --all --check

      - name: Lint
        run: cargo clippy --workspace --all-targets -- -D warnings

      # I test di integrazione avviano Postgres via testcontainers: Docker è
      # già disponibile sui runner GitHub.
      - name: Test
        run: cargo test --workspace -- --test-threads=1

      - name: La specifica OpenAPI è aggiornata
        run: git diff --exit-code docs/api/openapi.json

  frontend:
    runs-on: ubuntu-latest
    defaults: { run: { working-directory: frontend } }
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with: { node-version: "24", cache: npm, cache-dependency-path: frontend/package-lock.json }

      - run: npm ci
      - name: Tipi
        run: npx vue-tsc --noEmit
      - name: Test
        run: npx vitest run
      - name: Build
        run: npm run build

      - name: Budget del bundle iniziale (150 KB gzip)
        run: |
          SIZE=$(find dist/assets -name '*.js' -exec gzip -c {} \; | wc -c)
          echo "bundle gzip: $SIZE byte"
          if [ "$SIZE" -gt 153600 ]; then
            echo "::error::bundle oltre il budget di 153600 byte"
            exit 1
          fi

  audit:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: EmbarkStudios/cargo-deny-action@v2
        with: { command: check advisories bans licenses }

  image:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: docker/setup-buildx-action@v3
      - name: Build immagine (senza push)
        uses: docker/build-push-action@v6
        with:
          context: .
          push: false
          cache-from: type=gha
          cache-to: type=gha,mode=max
```

- [ ] **Step 3: Scrivere `.github/workflows/release.yml`**

```yaml
name: Release

on:
  push:
    tags: ["v*"]
  schedule:
    # Ricostruzione settimanale: raccoglie le patch di sicurezza delle immagini
    # di base senza attendere una release.
    - cron: "0 4 * * 1"

permissions:
  contents: read
  packages: write
  id-token: write

jobs:
  publish:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: docker/setup-qemu-action@v3
      - uses: docker/setup-buildx-action@v3
      - uses: docker/login-action@v3
        with:
          registry: ghcr.io
          username: ${{ github.actor }}
          password: ${{ secrets.GITHUB_TOKEN }}

      - id: meta
        uses: docker/metadata-action@v5
        with:
          images: ghcr.io/${{ github.repository }}
          tags: |
            type=semver,pattern={{version}}
            type=semver,pattern={{major}}
            type=raw,value=latest,enable={{is_default_branch}}

      - id: build
        uses: docker/build-push-action@v6
        with:
          context: .
          platforms: linux/amd64,linux/arm64
          push: true
          tags: ${{ steps.meta.outputs.tags }}
          labels: ${{ steps.meta.outputs.labels }}
          sbom: true
          provenance: mode=max

      - uses: sigstore/cosign-installer@v3
      - name: Firma l'immagine
        run: |
          cosign sign --yes \
            ghcr.io/${{ github.repository }}@${{ steps.build.outputs.digest }}
```

- [ ] **Step 4: Verificare i workflow in locale, per quanto possibile**

Run: `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace -- --test-threads=1`
Expected: tutto verde. Poi `cd frontend && npx vue-tsc --noEmit && npx vitest run && npm run build`.

- [ ] **Step 5: Commit e push**

```bash
git add .github deny.toml
git commit -m "ci: add build, test, audit and release pipelines"
git push -u origin main
```

- [ ] **Step 6: Verificare che la CI passi su GitHub**

Aprire la pagina Actions del repository. Tutti e quattro i job (`backend`, `frontend`, `audit`, `image`) devono essere verdi. In caso di fallimento, correggere e ricommittare prima di considerare la Fase 0 conclusa.

---

## Criteri di completamento della Fase 0

La fase è chiusa quando **tutti** questi punti sono verificati:

- [ ] `cargo test --workspace -- --test-threads=1` è verde (≈40 test).
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` non produce warning.
- [ ] `cd frontend && npx vitest run && npx vue-tsc --noEmit` è verde.
- [ ] Il bundle iniziale del frontend è sotto 150 KB gzip.
- [ ] `docker compose --profile bundled up -d` avvia lo stack e l'healthcheck riporta `healthy`.
- [ ] Da browser: setup del primo admin, logout, login, ricarica pagina con sessione persistente.
- [ ] L'immagine non contiene shell (`docker run --entrypoint /bin/sh` fallisce).
- [ ] `docs/api/openapi.json` è committato ed elenca i 6 endpoint.
- [ ] La CI è verde su GitHub.
- [ ] Riavviando lo stack, i dati sopravvivono.

## Cosa NON è in Fase 0

Da non implementare, per quanto tentante: scansione di librerie, asset, miniature, EXIF, mappe, WebDAV, upload, condivisione, gruppi, 2FA, WebSocket, code di job. Ognuno ha la sua fase. L'obiettivo qui è avere fondamenta su cui il resto si appoggia senza dover essere riscritto.

