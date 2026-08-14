//! Elaborazione dei file. Nessun database, nessuna rete, nessuno stato.

pub mod derive;
pub mod exif;
pub mod hash;
pub mod kind;
pub mod probe;
pub mod sandbox;
pub mod video;
pub mod walk;

pub use derive::{DeriveError, DeriveResult, derivative_paths, derive_jpeg};
pub use exif::read_exif;
pub use hash::hash_file;
pub use kind::detect_kind;
pub use probe::{Capabilities, probe};
pub use walk::{WalkedFile, is_stable, iter_entries, restat_if_stable};
