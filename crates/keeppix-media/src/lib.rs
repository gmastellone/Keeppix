//! Elaborazione dei file. Nessun database, nessuna rete, nessuno stato.

pub mod exif;
pub mod kind;
pub mod walk;

pub use exif::read_exif;
pub use kind::detect_kind;
pub use walk::{WalkedFile, is_stable, iter_entries, restat_if_stable};
