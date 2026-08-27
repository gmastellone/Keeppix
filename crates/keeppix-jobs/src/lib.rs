//! Coda e worker di ingestione. Unisce `keeppix-db` e `keeppix-media`.

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

/// Attesa prima di riconsiderare un file che sembra ancora in arrivo.
/// Usata da `main.rs` e dai test: se divergessero, un difetto potrebbe
/// vivere solo nel codice spedito — è già successo (Fase 2R / DIFETTO 1).
pub const PRODUCTION_STABILITY_WAIT: Duration = Duration::from_secs(5);

/// Un file con `mtime` più vecchio di questo non sta arrivando: un solo
/// `stat`, nessuna attesa.
pub const PRODUCTION_SETTLED_AFTER: Duration = Duration::from_secs(60);

/// Quanti file scrivere in `assets` prima di tornare a guardare il disco.
/// Tiene la RAM costante e fa comparire le foto in timeline durante lo scan.
pub const PRODUCTION_BATCH_SIZE: usize = 500;
