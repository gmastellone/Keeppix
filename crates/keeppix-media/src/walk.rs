use std::fs::Metadata;
use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::{DateTime, Utc};
use walkdir::{DirEntry, WalkDir};

/// A file whose `mtime` is older than this isn't still arriving: one
/// `stat`, no waiting. Matches `keeppix_jobs::PRODUCTION_SETTLED_AFTER`.
pub const SETTLED_AFTER: Duration = Duration::from_secs(60);

/// Outcome of [`freshness`]: settled (can be indexed) or still arriving
/// (the caller defers it, without sleeping).
#[derive(Debug)]
pub enum Freshness {
    Settled(Metadata),
    InFlight,
}

/// A file seen by the walker: `stat` only, never opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalkedFile {
    pub path: PathBuf,
    pub relative_dir: Vec<String>,
    pub filename: String,
    pub size_bytes: i64,
    pub mtime: DateTime<Utc>,
    pub inode: Option<i64>,
}

/// Two `stat` calls with different `size` or `mtime`: the file is still arriving.
#[must_use]
pub fn is_stable(first: &Metadata, second: &Metadata) -> bool {
    first.len() == second.len() && first.modified().ok() == second.modified().ok()
}

/// Never sleeps. A file untouched for more than `settled_after` is
/// settled: one `stat` and done. Only recently touched files are
/// ambiguous, and those get deferred by the caller instead of blocking it.
///
/// # Errors
/// I/O from the `stat` call.
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

/// Re-stats after `wait` **only if** `wait` is zero (legacy test
/// compatibility): otherwise delegates to [`freshness`] with `wait` as the
/// age threshold. **Never sleeps.**
///
/// # Errors
/// I/O from the `stat` call.
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
