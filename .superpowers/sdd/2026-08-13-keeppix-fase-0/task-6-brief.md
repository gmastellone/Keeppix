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

