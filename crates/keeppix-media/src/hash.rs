use std::fs::File;
use std::path::Path;

/// Hash streaming. Non carica il file in RAM.
///
/// # Errors
/// I/O di lettura.
pub fn hash_file(path: &Path) -> std::io::Result<[u8; 32]> {
    let mut hasher = blake3::Hasher::new();
    hasher.update_reader(File::open(path)?)?;
    Ok(*hasher.finalize().as_bytes())
}
