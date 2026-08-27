use argon2::password_hash::{PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng};
use argon2::{Algorithm, Argon2, Params, Version};
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::error::DomainError;

const PASSWORD_MIN: usize = 10;
const PASSWORD_MAX: usize = 1024;

// OWASP parameters: 19 MiB of memory, 2 iterations, parallelism 1.
const ARGON_M_COST: u32 = 19_456;
const ARGON_T_COST: u32 = 2;
const ARGON_P_COST: u32 = 1;

/// Plaintext password, alive only as long as needed to produce its hash.
///
/// `ZeroizeOnDrop` clears the owned buffer. Prefer [`Password::parse_owned`]
/// when the plaintext already lives in a `String` (e.g. after serde): that
/// moves the allocation into this type so Drop clears the only heap copy we
/// control. [`Password::parse`] copies from a borrowed slice and cannot wipe
/// the caller's source.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct Password(String);

impl Password {
    /// # Errors
    /// `DomainError::InvalidPassword` if outside the length limits.
    pub fn parse(raw: &str) -> Result<Self, DomainError> {
        Self::validate_len(raw)?;
        Ok(Self(raw.to_owned()))
    }

    /// Takes ownership of a plaintext buffer so Drop can zeroize it.
    ///
    /// On validation failure the buffer is cleared before the error returns.
    ///
    /// # Errors
    /// `DomainError::InvalidPassword` if outside the length limits.
    pub fn parse_owned(mut raw: String) -> Result<Self, DomainError> {
        if let Err(err) = Self::validate_len(&raw) {
            raw.zeroize();
            return Err(err);
        }
        Ok(Self(raw))
    }

    fn validate_len(raw: &str) -> Result<(), DomainError> {
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
        Ok(())
    }

    fn expose(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

// Prevents a password from ending up in logs by accident.
impl std::fmt::Debug for Password {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Password(***)")
    }
}

/// PHC-encoded hash, ready for persistence.
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
/// `DomainError::PasswordHashing` if the parameters or salt generator fail.
pub fn hash_password(password: &Password) -> Result<PasswordHash, DomainError> {
    let salt = SaltString::generate(&mut OsRng);
    let hash = argon2()?
        .hash_password(password.expose(), &salt)
        .map_err(|e| DomainError::PasswordHashing(e.to_string()))?;
    Ok(PasswordHash(hash.to_string()))
}

/// Returns `false` — never an error — if the stored hash is unreadable, so
/// a corrupted record denies access instead of blowing up the login.
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
    #[allow(clippy::unwrap_used)]
    fn hash_is_verifiable() {
        let pw = Password::parse("correct horse battery staple").unwrap();
        let hash = hash_password(&pw).unwrap();
        assert!(verify_password(&pw, &hash));
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn hash_rejects_wrong_password() {
        let pw = Password::parse("correct horse battery staple").unwrap();
        let other = Password::parse("incorrect horse battery").unwrap();
        let hash = hash_password(&pw).unwrap();
        assert!(!verify_password(&other, &hash));
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn same_password_produces_different_hashes() {
        let pw = Password::parse("correct horse battery staple").unwrap();
        assert_ne!(
            hash_password(&pw).unwrap().as_str(),
            hash_password(&pw).unwrap().as_str(),
            "the salt must be random"
        );
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn malformed_hash_returns_false_without_panicking() {
        let pw = Password::parse("correct horse battery staple").unwrap();
        let broken = PasswordHash::from_stored("non-un-hash".to_owned());
        assert!(!verify_password(&pw, &broken));
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn hash_is_argon2id_with_owasp_parameters() {
        let pw = Password::parse("correct horse battery staple").unwrap();
        let hash = hash_password(&pw).unwrap();
        assert!(hash.as_str().starts_with("$argon2id$"));
        assert!(hash.as_str().contains("m=19456,t=2,p=1"));
    }

    /// After the owned serde → `Password` path drops, the heap buffer that
    /// held the plaintext must no longer contain the canary. We also assert
    /// `Zeroize` clears the buffer while the allocation is still owned —
    /// post-`Drop` pages are often rewritten by the allocator, so an
    /// all-zeros check after free is not a reliable oracle.
    #[test]
    #[allow(clippy::unwrap_used, clippy::undocumented_unsafe_blocks)]
    fn drop_zeroizes_buffer_after_serde_owned_path() {
        use std::mem::ManuallyDrop;

        use serde::Deserialize;

        #[derive(Deserialize)]
        struct Body {
            password: String,
        }

        const CANARY: &str = "canary-zeroize-after-serde";
        let json = format!(r#"{{"password":"{CANARY}"}}"#);
        let body: Body = serde_json::from_str(&json).unwrap();
        let mut pw = Password::parse_owned(body.password).unwrap();
        let ptr = pw.0.as_ptr();
        let len = pw.0.len();
        assert_eq!(len, CANARY.len());

        // Still owned: Zeroize must scrub the capacity before len goes to 0.
        pw.zeroize();
        unsafe {
            let bytes = std::slice::from_raw_parts(ptr, len);
            assert!(
                bytes.iter().all(|&b| b == 0),
                "Zeroize left plaintext in the owned buffer"
            );
        }

        // Re-build via the same serde-owned path and ensure Drop erases the canary
        // at that address (allocator may scribble non-zero metadata afterward).
        let body: Body = serde_json::from_str(&json).unwrap();
        let pw = Password::parse_owned(body.password).unwrap();
        let mut held = ManuallyDrop::new(pw);
        let ptr = held.0.as_ptr();
        let len = held.0.len();
        unsafe {
            std::ptr::drop_in_place(&raw mut *held);
            let bytes = std::slice::from_raw_parts(ptr, len);
            assert_ne!(
                bytes,
                CANARY.as_bytes(),
                "plaintext canary survived Drop after the serde-owned path"
            );
        }
    }

    #[test]
    #[allow(clippy::undocumented_unsafe_blocks)]
    fn parse_owned_zeroizes_rejected_input() {
        use std::mem::ManuallyDrop;

        let raw = String::from("short");
        let mut held = ManuallyDrop::new(raw);
        let ptr = held.as_ptr();
        let len = held.len();
        let owned = unsafe { ManuallyDrop::take(&mut held) };
        assert!(Password::parse_owned(owned).is_err());
        unsafe {
            let bytes = std::slice::from_raw_parts(ptr, len);
            assert_ne!(
                bytes, b"short",
                "rejected password buffer still held the plaintext"
            );
            // Prefer zeros when the page was not yet reused.
            if bytes.iter().any(|&b| b != 0) {
                // Allocator scribble is acceptable; the original bytes must be gone.
                assert!(!bytes.windows(5).any(|w| w == b"short"));
            }
        }
    }
}
