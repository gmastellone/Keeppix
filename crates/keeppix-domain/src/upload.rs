//! Sessioni di upload riprendibili in stile tus (spec §1). Il protocollo è
//! nostro (non un crate tus): pre-check per hash, creazione con verifica di
//! spazio e permessi, chunk con checksum, finalizzazione con verifica
//! end-to-end e risoluzione di collisione sul nome.

use std::path::PathBuf;

use chrono::{DateTime, Utc};

use crate::ids::{AssetId, FolderId, UploadSessionId, UserId};

/// Chi possiede la sessione. Esattamente uno dei due — mai entrambi, mai
/// nessuno — per il vincolo `upload_sessions_one_actor`: un upload
/// appartiene a un utente autenticato oppure a un link condiviso con
/// `allow_upload`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UploadOwner {
    User(UserId),
    ShareLink(uuid::Uuid),
}

/// Una sessione di upload tus in corso o completabile.
#[derive(Debug, Clone, PartialEq)]
pub struct UploadSession {
    pub id: UploadSessionId,
    pub owner: UploadOwner,
    pub target_folder_id: FolderId,
    pub filename: String,
    pub expected_size: i64,
    /// blake3 dichiarato dal client per il file intero. `None` se il client
    /// non lo conosce in anticipo — la verifica end-to-end si salta, ma
    /// quella di decodificabilità resta obbligatoria.
    pub expected_hash: Option<[u8; 32]>,
    pub received_bytes: i64,
    pub temp_path: PathBuf,
    /// `mtime` del file originale sul dispositivo del client, preservato
    /// così una foto senza EXIF non perde la data reale — coerente con
    /// l'invariante già esistente su `assets.mtime` come fallback di
    /// `taken_at_utc`.
    pub client_mtime: Option<DateTime<Utc>>,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

impl UploadSession {
    #[must_use]
    pub fn is_expired_at(&self, now: DateTime<Utc>) -> bool {
        now > self.expires_at
    }
}

/// Checksum blake3 di un singolo chunk (header `Upload-Checksum` del
/// protocollo). Avvolge i 32 byte già decodificati: la codifica esadecimale
/// che arriva sulla rete è un dettaglio del livello HTTP, non del dominio.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkChecksum(pub [u8; 32]);

impl ChunkChecksum {
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    #[must_use]
    pub fn matches(&self, computed: &[u8; 32]) -> bool {
        &self.0 == computed
    }
}

/// Esito della risoluzione di collisione a fine upload (spec §1.4): mai una
/// sovrascrittura silenziosa.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CollisionOutcome {
    /// Nessuna collisione: l'asset è stato creato con il nome originale.
    Created,
    /// Stesso nome, stesso hash: il file caricato è un duplicato di uno già
    /// presente nella cartella. Nessun secondo file, nessun nuovo asset —
    /// `existing_asset_id` è quello già in libreria.
    SkippedDuplicate { existing_asset_id: AssetId },
    /// Stesso nome, hash diverso: salvato con un suffisso numerico per non
    /// sovrascrivere il file esistente.
    RenamedTo(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn sample_session(expires_at: DateTime<Utc>) -> UploadSession {
        UploadSession {
            id: UploadSessionId::new(),
            owner: UploadOwner::User(UserId::new()),
            target_folder_id: FolderId::new(),
            filename: "foto.jpg".to_owned(),
            expected_size: 10,
            expected_hash: None,
            received_bytes: 0,
            temp_path: PathBuf::from("/tmp/x"),
            client_mtime: None,
            expires_at,
            created_at: Utc::now(),
        }
    }

    #[test]
    fn a_session_past_its_expiry_is_expired() {
        let session = sample_session(Utc::now() - Duration::seconds(1));
        assert!(session.is_expired_at(Utc::now()));
    }

    #[test]
    fn a_session_before_its_expiry_is_not_expired() {
        let session = sample_session(Utc::now() + Duration::days(7));
        assert!(!session.is_expired_at(Utc::now()));
    }

    #[test]
    fn a_chunk_checksum_matches_only_the_same_bytes() {
        let checksum = ChunkChecksum([1_u8; 32]);
        assert!(checksum.matches(&[1_u8; 32]));
        assert!(!checksum.matches(&[2_u8; 32]));
    }
}
