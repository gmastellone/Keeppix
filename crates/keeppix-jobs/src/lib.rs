//! Ingestion queue and worker. Ties together `keeppix-db` and `keeppix-media`.

pub mod backup;
pub mod cleanup_trash;
pub mod derive;
pub mod detect_faces;
pub mod discover;
pub mod dispatch;
pub mod embed;
pub mod error;
pub mod geotag;
pub mod hash;
pub mod maintenance;
pub mod metadata;
pub mod moves;
pub mod pool;
pub mod profile;
pub mod raw;
pub mod regions;
pub mod rename_batch;
pub mod retry_derives;
pub mod tmp_cleanup;
pub mod transcode;
pub mod watch;
pub mod xmp;

use std::time::Duration;

pub use dispatch::{DEFAULT_RAM_HINT, IngestHandler, JobHandler, ram_hint_for_image};
pub use error::JobError;
pub use keeppix_media::{
    DEFAULT_FULL_CACHE_BYTES, DEFAULT_WEBP_METHOD, DEFAULT_WEBP_QUALITY, set_webp_method,
    set_webp_quality,
};
pub use pool::{RamGate, WorkerPool};
pub use profile::{
    ActivityTracker, AnalysisLevel, DEFAULT_ANALYSIS_IDLE_MS, EnergyProfile, default_night_window,
    max_claimable_priority, worker_count,
};

/// Wait before reconsidering a file that still looks like it's arriving.
/// Used by `main.rs` and by tests: if the two diverged, a bug could live
/// only in the shipped code and go unnoticed — that has happened before.
pub const PRODUCTION_STABILITY_WAIT: Duration = Duration::from_secs(5);

/// A file with an `mtime` older than this is not still arriving: a single
/// `stat`, no waiting.
pub const PRODUCTION_SETTLED_AFTER: Duration = Duration::from_secs(60);

/// How many files to write to `assets` before going back to watching the
/// disk. Keeps RAM constant and lets photos appear in the timeline during
/// the scan.
pub const PRODUCTION_BATCH_SIZE: usize = 500;
