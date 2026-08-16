//! Coda e worker di ingestione. Unisce `keeppix-db` e `keeppix-media`.

pub mod derive;
pub mod discover;
pub mod dispatch;
pub mod error;
pub mod hash;
pub mod metadata;
pub mod moves;
pub mod pool;
pub mod profile;
pub mod raw;
pub mod watch;
pub mod xmp;

pub use dispatch::{DEFAULT_RAM_HINT, IngestHandler, JobHandler, ram_hint_for_image};
pub use error::JobError;
pub use pool::{RamGate, WorkerPool};
pub use profile::{ActivityTracker, EnergyProfile, default_night_window, worker_count};
