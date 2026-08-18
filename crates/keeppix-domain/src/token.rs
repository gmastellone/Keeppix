use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rand::Rng;
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

/// Token opaco per link pubblici. Stesso schema di `SessionToken`.
#[derive(Clone, PartialEq, Eq)]
pub struct ShareToken(String);

impl ShareToken {
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

    #[must_use]
    pub fn digest(&self) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(self.0.as_bytes());
        hasher.finalize().into()
    }
}

impl std::fmt::Debug for ShareToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ShareToken(***)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_tokens_are_unique() {
        assert_ne!(
            SessionToken::generate().as_str(),
            SessionToken::generate().as_str()
        );
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
        assert_ne!(
            SessionToken::generate().digest(),
            SessionToken::generate().digest()
        );
    }

    #[test]
    fn debug_does_not_leak_the_secret() {
        let t = SessionToken::generate();
        let rendered = format!("{t:?}");
        assert!(
            !rendered.contains(t.as_str()),
            "il token non deve finire nei log"
        );
    }
}
