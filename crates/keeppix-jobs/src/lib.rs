//! Coda e worker di ingestione. Unisce `keeppix-db` e `keeppix-media`.

pub mod discover;
pub mod dispatch;
pub mod error;
pub mod pool;
pub mod profile;

pub use dispatch::{DEFAULT_RAM_HINT, IngestHandler, JobHandler, ram_hint_for_image};
pub use error::JobError;
pub use pool::{RamGate, WorkerPool};
pub use profile::{ActivityTracker, EnergyProfile, worker_count};
