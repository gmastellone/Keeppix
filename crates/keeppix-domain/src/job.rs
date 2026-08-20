use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::DomainError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobKind {
    DiscoverLibrary,
    ExtractMetadata,
    HashAsset,
    DeriveAsset,
    DeriveRaw,
    WriteSidecar,
    ReapStale,
    CleanupTrash,
    RetryErrorAssets,
    DownloadMapRegion,
    TmpCleanup,
    TranscodeVideo,
    BackupDump,
    RestoreProof,
    PurgeSessions,
    CleanupDoneJobs,
    CleanupTranscodeCache,
    CleanupIdempotency,
    VacuumAnalyze,
    IntegrityScrub,
}

impl JobKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DiscoverLibrary => "discover_library",
            Self::ExtractMetadata => "extract_metadata",
            Self::HashAsset => "hash_asset",
            Self::DeriveAsset => "derive_asset",
            Self::DeriveRaw => "derive_raw",
            Self::WriteSidecar => "write_sidecar",
            Self::ReapStale => "reap_stale",
            Self::CleanupTrash => "cleanup_trash",
            Self::RetryErrorAssets => "retry_error_assets",
            Self::DownloadMapRegion => "download_map_region",
            Self::TmpCleanup => "tmp_cleanup",
            Self::TranscodeVideo => "transcode_video",
            Self::BackupDump => "backup_dump",
            Self::RestoreProof => "restore_proof",
            Self::PurgeSessions => "purge_sessions",
            Self::CleanupDoneJobs => "cleanup_done_jobs",
            Self::CleanupTranscodeCache => "cleanup_transcode_cache",
            Self::CleanupIdempotency => "cleanup_idempotency",
            Self::VacuumAnalyze => "vacuum_analyze",
            Self::IntegrityScrub => "integrity_scrub",
        }
    }

    /// # Errors
    /// `DomainError::InvalidJobKind` se la stringa non è uno dei tipi noti.
    pub fn parse(raw: &str) -> Result<Self, DomainError> {
        match raw {
            "discover_library" => Ok(Self::DiscoverLibrary),
            "extract_metadata" => Ok(Self::ExtractMetadata),
            "hash_asset" => Ok(Self::HashAsset),
            "derive_asset" => Ok(Self::DeriveAsset),
            "derive_raw" => Ok(Self::DeriveRaw),
            "write_sidecar" => Ok(Self::WriteSidecar),
            "reap_stale" => Ok(Self::ReapStale),
            "cleanup_trash" => Ok(Self::CleanupTrash),
            "retry_error_assets" => Ok(Self::RetryErrorAssets),
            "download_map_region" => Ok(Self::DownloadMapRegion),
            "tmp_cleanup" => Ok(Self::TmpCleanup),
            "transcode_video" => Ok(Self::TranscodeVideo),
            "backup_dump" => Ok(Self::BackupDump),
            "restore_proof" => Ok(Self::RestoreProof),
            "purge_sessions" => Ok(Self::PurgeSessions),
            "cleanup_done_jobs" => Ok(Self::CleanupDoneJobs),
            "cleanup_transcode_cache" => Ok(Self::CleanupTranscodeCache),
            "cleanup_idempotency" => Ok(Self::CleanupIdempotency),
            "vacuum_analyze" => Ok(Self::VacuumAnalyze),
            "integrity_scrub" => Ok(Self::IntegrityScrub),
            other => Err(DomainError::InvalidJobKind(other.to_owned())),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Pending,
    Running,
    Done,
    Failed,
}

impl JobStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Done => "done",
            Self::Failed => "failed",
        }
    }

    /// # Errors
    /// `DomainError::InvalidJobStatus` se la stringa non è uno dei quattro stati.
    pub fn parse(raw: &str) -> Result<Self, DomainError> {
        match raw {
            "pending" => Ok(Self::Pending),
            "running" => Ok(Self::Running),
            "done" => Ok(Self::Done),
            "failed" => Ok(Self::Failed),
            other => Err(DomainError::InvalidJobStatus(other.to_owned())),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[repr(i16)]
pub enum JobPriority {
    Interactive = 0,
    High = 1,
    Visible = 2,
    Background = 3,
}

impl JobPriority {
    #[must_use]
    pub const fn as_i16(self) -> i16 {
        self as i16
    }

    /// # Errors
    /// `DomainError::InvalidJobPriority` se il valore non è 0..=3.
    pub fn from_i16(raw: i16) -> Result<Self, DomainError> {
        match raw {
            0 => Ok(Self::Interactive),
            1 => Ok(Self::High),
            2 => Ok(Self::Visible),
            3 => Ok(Self::Background),
            other => Err(DomainError::InvalidJobPriority(other)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Job {
    pub id: i64,
    pub kind: JobKind,
    pub payload: serde_json::Value,
    pub priority: JobPriority,
    pub status: JobStatus,
    pub attempts: i32,
    pub max_attempts: i32,
    pub last_error: Option<String>,
    pub run_after: DateTime<Utc>,
    pub locked_by: Option<uuid::Uuid>,
    pub dedup_key: Option<String>,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn job_kind_round_trips_snake_case() {
        for kind in [
            JobKind::DiscoverLibrary,
            JobKind::ExtractMetadata,
            JobKind::HashAsset,
            JobKind::DeriveAsset,
            JobKind::DeriveRaw,
            JobKind::WriteSidecar,
            JobKind::ReapStale,
            JobKind::CleanupTrash,
            JobKind::RetryErrorAssets,
            JobKind::DownloadMapRegion,
            JobKind::TmpCleanup,
            JobKind::TranscodeVideo,
            JobKind::BackupDump,
            JobKind::RestoreProof,
            JobKind::PurgeSessions,
            JobKind::CleanupDoneJobs,
            JobKind::CleanupTranscodeCache,
            JobKind::CleanupIdempotency,
            JobKind::VacuumAnalyze,
            JobKind::IntegrityScrub,
        ] {
            assert_eq!(JobKind::parse(kind.as_str()).expect("round-trip"), kind);
        }
    }

    #[test]
    fn unknown_kind_is_rejected() {
        assert!(JobKind::parse("index_file").is_err());
    }

    #[test]
    fn map_region_download_kind_round_trips() {
        assert_eq!(
            JobKind::parse("download_map_region").expect("known kind"),
            JobKind::DownloadMapRegion
        );
    }

    #[test]
    fn priority_order_matches_the_energy_profile() {
        assert!(JobPriority::Interactive < JobPriority::Background);
        assert_eq!(
            JobPriority::from_i16(3).expect("3"),
            JobPriority::Background
        );
        assert!(JobPriority::from_i16(4).is_err());
    }
}
