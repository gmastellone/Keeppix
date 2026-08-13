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

