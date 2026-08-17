use std::fs::Metadata;
use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::{DateTime, Utc};
use walkdir::{DirEntry, WalkDir};

/// Un file con `mtime` più vecchio di questo non sta arrivando: un solo
/// `stat`, nessuna attesa. Allineato a `keeppix_jobs::PRODUCTION_SETTLED_AFTER`.
pub const SETTLED_AFTER: Duration = Duration::from_secs(60);

/// Esito di [`freshness`]: assestato (si può indicizzare) o ancora in
/// arrivo (il chiamante rimanda, senza dormire).
#[derive(Debug)]
pub enum Freshness {
    Settled(Metadata),
    InFlight,
}

/// File visto dal walker: solo `stat`, nessun open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalkedFile {
    pub path: PathBuf,
    pub relative_dir: Vec<String>,
    pub filename: String,
    pub size_bytes: i64,
    pub mtime: DateTime<Utc>,
    pub inode: Option<i64>,
}

/// Due `stat` con `size` o `mtime` diversi: il file sta ancora arrivando.
#[must_use]
pub fn is_stable(first: &Metadata, second: &Metadata) -> bool {
    first.len() == second.len() && first.modified().ok() == second.modified().ok()
}

/// Non dorme mai. Un file fermo da più di `settled_after` è assestato:
/// un solo `stat` e via. Solo i file toccati di recente sono ambigui, e
/// quelli vengono rimandati dal chiamante invece di bloccarlo.
///
/// # Errors
/// I/O dello `stat`.
pub fn freshness(path: &Path, settled_after: Duration) -> std::io::Result<Freshness> {
    let meta = std::fs::metadata(path)?;
    let age = meta
        .modified()
        .ok()
        .and_then(|m| m.elapsed().ok())
        .unwrap_or(Duration::MAX);
    Ok(if age >= settled_after {
        Freshness::Settled(meta)
    } else {
        Freshness::InFlight
    })
}

/// Ristat dopo `wait` **solo se** `wait` è zero (compat test legacy): altrimenti
/// delega a [`freshness`] con `wait` come soglia di età. **Non dorme mai.**
///
/// # Errors
/// I/O dello `stat`.
pub fn restat_if_stable(path: &Path, wait: Duration) -> std::io::Result<Option<Metadata>> {
    match freshness(path, wait)? {
        Freshness::Settled(meta) => Ok(Some(meta)),
        Freshness::InFlight => Ok(None),
    }
}

pub fn iter_entries<'a>(
    root: &'a Path,
    extra_globs: &'a [String],
) -> impl Iterator<Item = WalkedFile> + 'a {
    WalkDir::new(root)
        .into_iter()
        .filter_entry(|e| !is_pruned(e))
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .filter_map(move |e| to_walked(root, &e, extra_globs))
}

fn is_pruned(entry: &DirEntry) -> bool {
    if entry.depth() == 0 {
        return false;
    }
    let name = entry.file_name().to_string_lossy();
    is_excluded_name(&name)
}

fn is_excluded_name(name: &str) -> bool {
    matches!(
        name,
        "@eaDir" | "Thumbs.db" | "#recycle" | "#snapshot" | ".keeppix-trash" | ".keeppix-tmp"
    ) || name.starts_with('.')
        || Path::new(name)
            .extension()
            .is_some_and(is_sidecar_extension)
}

fn is_sidecar_extension(ext: &std::ffi::OsStr) -> bool {
    ext.eq_ignore_ascii_case("xmp")
        || ext.eq_ignore_ascii_case("dop")
        || ext.eq_ignore_ascii_case("pp3")
        || ext.eq_ignore_ascii_case("arp")
        || ext.eq_ignore_ascii_case("thm")
        || ext.eq_ignore_ascii_case("aae")
}

fn globish(pattern: &str, relative: &str) -> bool {
    let file = relative.rsplit('/').next().unwrap_or(relative);
    if let Some(prefix) = pattern.strip_suffix('*') {
        return relative.starts_with(prefix) || file.starts_with(prefix);
    }
    relative == pattern || file == pattern || relative.contains(&format!("/{pattern}/"))
}

fn to_walked(root: &Path, entry: &DirEntry, extra_globs: &[String]) -> Option<WalkedFile> {
    let rel = entry.path().strip_prefix(root).ok()?;
    let rel_str = rel.to_string_lossy();
    if extra_globs.iter().any(|g| globish(g, &rel_str)) {
        return None;
    }
    let filename = entry.file_name().to_string_lossy().into_owned();
    let relative_dir: Vec<String> = rel
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .components()
        .filter_map(|c| match c {
            std::path::Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect();
    let meta = entry.metadata().ok()?;
    let mtime = DateTime::<Utc>::from(meta.modified().ok()?);
    let size_bytes = i64::try_from(meta.len()).ok()?;
    Some(WalkedFile {
        path: entry.path().to_path_buf(),
        relative_dir,
        filename,
        size_bytes,
        mtime,
        inode: inode_of(&meta),
    })
}

fn inode_of(meta: &Metadata) -> Option<i64> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        i64::try_from(meta.ino()).ok()
    }
    #[cfg(not(unix))]
    {
        let _ = meta;
        None
    }
}
