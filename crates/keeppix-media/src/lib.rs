//! Elaborazione dei file. Nessun database, nessuna rete, nessuno stato.

pub mod derive;
pub mod exif;
pub mod hash;
pub mod kind;
pub mod probe;
pub mod raw;
pub mod sandbox;
pub mod video;
pub mod walk;
pub mod xmp;

pub use derive::{
    DEFAULT_FULL_CACHE_BYTES, DEFAULT_WEBP_QUALITY, DeriveError, DeriveResult, derivative_paths,
    derive_from_bytes, derive_from_rgb, derive_jpeg, enforce_full_cache_cap,
    ensure_full_from_bytes, full_derivative_path, set_webp_quality,
};
pub use exif::read_exif;
pub use hash::hash_file;
pub use kind::detect_kind;
pub use probe::{Capabilities, probe};
pub use raw::{
    PreviewSource, RawError, RawPreview, dcraw_emu_available, demosaic_half,
    extract_embedded_preview,
};
pub use walk::{
    Freshness, SETTLED_AFTER, WalkedFile, freshness, is_stable, iter_entries, restat_if_stable,
};

pub use xmp::{SidecarData, XmpError, read_sidecar, write_sidecar};
