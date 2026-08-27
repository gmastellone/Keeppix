//! Resumable upload sessions, tus-style. The protocol is our own (not a tus
//! crate): a hash pre-check, creation with space and permission checks,
//! chunks with checksums, and finalization with end-to-end verification and
//! name-collision resolution.

use std::path::PathBuf;

use chrono::{DateTime, Utc};

use crate::ids::{AssetId, FolderId, UploadSessionId, UserId};

/// Who owns the session. Exactly one of the two — never both, never
/// neither — per the `upload_sessions_one_actor` constraint: an upload
/// belongs either to an authenticated user or to a share link with
/// `allow_upload`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UploadOwner {
    User(UserId),
    ShareLink(uuid::Uuid),
}

/// A tus upload session that's in progress or completable.
#[derive(Debug, Clone, PartialEq)]
pub struct UploadSession {
    pub id: UploadSessionId,
    pub owner: UploadOwner,
    pub target_folder_id: FolderId,
    pub filename: String,
    pub expected_size: i64,
    /// blake3 declared by the client for the whole file. `None` if the
    /// client doesn't know it in advance — the end-to-end check is skipped,
    /// but the decodability check remains mandatory.
    pub expected_hash: Option<[u8; 32]>,
    pub received_bytes: i64,
    pub temp_path: PathBuf,
    /// `mtime` of the original file on the client's device, preserved so a
    /// photo without EXIF doesn't lose its real date — consistent with the
    /// existing invariant on `assets.mtime` as a fallback for
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

/// blake3 checksum of a single chunk (the protocol's `Upload-Checksum`
/// header). Wraps the already-decoded 32 bytes: the hex encoding that
/// arrives over the network is an HTTP-layer detail, not a domain one.
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

/// Outcome of collision resolution at the end of an upload: never a silent
/// overwrite.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CollisionOutcome {
    /// No collision: the asset was created with its original name.
    Created,
    /// Same name, same hash: the uploaded file is a duplicate of one
    /// already present in the folder. No second file, no new asset —
    /// `existing_asset_id` is the one already in the library.
    SkippedDuplicate { existing_asset_id: AssetId },
    /// Same name, different hash: saved with a numeric suffix so it doesn't
    /// overwrite the existing file.
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
