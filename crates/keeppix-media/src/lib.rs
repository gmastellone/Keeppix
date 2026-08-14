//! Elaborazione dei file. Nessun database, nessuna rete, nessuno stato.

pub mod exif;
pub mod hash;
pub mod kind;
pub mod walk;

pub use exif::read_exif;
pub use hash::hash_file;
pub use kind::detect_kind;
pub use walk::{WalkedFile, is_stable, iter_entries, restat_if_stable};
