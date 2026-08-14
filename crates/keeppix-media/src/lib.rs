//! Elaborazione dei file. Nessun database, nessuna rete, nessuno stato.

pub mod kind;
pub mod walk;

pub use kind::detect_kind;
pub use walk::{WalkedFile, is_stable, iter_entries, restat_if_stable};
