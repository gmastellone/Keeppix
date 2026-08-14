use std::path::{Path, PathBuf};
use std::time::Duration;

use keeppix_db::{Db, JobRepo, LibraryRepo};
use keeppix_domain::{JobKind, JobPriority, LibraryId};
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher as _};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::JobError;

pub const DEFAULT_DEBOUNCE: Duration = Duration::from_secs(2);
pub const DEFAULT_POLL: Duration = Duration::from_secs(15 * 60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatcherMode {
    Native,
    Polling { every: Duration },
}

/// NFS/SMB/rclone → polling. Su Linux, inotify assente o troppo basso → polling.
#[must_use]
pub fn mode_for(root: &Path) -> WatcherMode {
    if is_network_fs(root) {
        return WatcherMode::Polling {
            every: DEFAULT_POLL,
        };
    }
    #[cfg(target_os = "linux")]
    {
        if !inotify_watches_ok() {
            tracing::warn!(
                "fs.inotify.max_user_watches missing or low; \
                 sysctl fs.inotify.max_user_watches=524288"
            );
            return WatcherMode::Polling {
                every: DEFAULT_POLL,
            };
        }
    }
    WatcherMode::Native
}

/// Accoda una riscansione. Dedup sulla chiave `discover:{id}`.
///
/// # Errors
/// Database.
pub async fn enqueue_rescan(db: &Db, library_id: LibraryId) -> Result<(), JobError> {
    JobRepo::new(db)
        .enqueue(
            JobKind::DiscoverLibrary,
            serde_json::json!({ "library_id": library_id.to_string() }),
            JobPriority::Background,
            Some(&format!("discover:{library_id}")),
        )
        .await?;
    Ok(())
}

/// Avvia un watcher per ogni libreria. Le librerie create dopo il boot
/// restano scoperte fino al riavvio (ponytail: 1c può rinfrescare).
///
/// # Errors
/// Database.
pub async fn spawn_all(db: &Db, debounce: Duration) -> Result<Vec<JoinHandle<()>>, JobError> {
    let libs = LibraryRepo::new(db).list_for_scan().await?;
    let mut handles = Vec::new();
    for lib in libs {
        let mode = mode_for(&lib.root_path);
        if matches!(mode, WatcherMode::Polling { .. }) {
            tracing::warn!(
                library = %lib.id,
                root = %lib.root_path.display(),
                "watcher_mode=polling"
            );
        }
        handles.push(spawn(db.clone(), lib.id, lib.root_path, debounce, mode));
    }
    Ok(handles)
}

#[must_use]
pub fn spawn(
    db: Db,
    library_id: LibraryId,
    root: PathBuf,
    debounce: Duration,
    mode: WatcherMode,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        if let Err(e) = run(db, library_id, root, debounce, mode).await {
            tracing::error!(error = %e, "library watcher");
        }
    })
}

async fn run(
    db: Db,
    library_id: LibraryId,
    root: PathBuf,
    debounce: Duration,
    mode: WatcherMode,
) -> Result<(), JobError> {
    match mode {
        WatcherMode::Native => watch_native(db, library_id, root, debounce).await,
        WatcherMode::Polling { every } => watch_poll(db, library_id, every).await,
    }
}

async fn watch_poll(db: Db, library_id: LibraryId, every: Duration) -> Result<(), JobError> {
    loop {
        tokio::time::sleep(every).await;
        enqueue_rescan(&db, library_id).await?;
    }
}

async fn watch_native(
    db: Db,
    library_id: LibraryId,
    root: PathBuf,
    debounce: Duration,
) -> Result<(), JobError> {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut watcher = RecommendedWatcher::new(
        move |res: Result<Event, notify::Error>| {
            let _ = tx.send(res);
        },
        notify::Config::default(),
    )
    .map_err(|e| JobError::Worker(format!("notify: {e}")))?;
    watcher
        .watch(&root, RecursiveMode::Recursive)
        .map_err(|e| JobError::Worker(format!("watch {}: {e}", root.display())))?;

    let mut pending = false;
    loop {
        if pending {
            tokio::select! {
                ev = rx.recv() => {
                    match ev {
                        None => return Ok(()),
                        Some(Ok(event)) if interesting(&event) => pending = true,
                        Some(_) => {}
                    }
                }
                () = tokio::time::sleep(debounce) => {
                    enqueue_rescan(&db, library_id).await?;
                    pending = false;
                }
            }
        } else {
            match rx.recv().await {
                None => return Ok(()),
                Some(Ok(event)) if interesting(&event) => pending = true,
                Some(_) => {}
            }
        }
    }
}

fn interesting(event: &Event) -> bool {
    event.paths.iter().any(|p| !is_hidden_path(p))
}

fn is_hidden_path(path: &Path) -> bool {
    path.components().any(|c| {
        matches!(
            c,
            std::path::Component::Normal(s) if s.to_string_lossy().starts_with('.')
        )
    })
}

fn is_network_fs(root: &Path) -> bool {
    #[cfg(target_os = "linux")]
    {
        linux_is_network(root)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = root;
        false
    }
}

#[cfg(target_os = "linux")]
fn linux_is_network(root: &Path) -> bool {
    let Ok(mounts) = std::fs::read_to_string("/proc/mounts") else {
        return false;
    };
    let canon = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let mut best: Option<(usize, bool)> = None;
    for line in mounts.lines() {
        let mut parts = line.split_whitespace();
        let _dev = parts.next();
        let Some(mp) = parts.next() else { continue };
        let Some(fstype) = parts.next() else { continue };
        let mp = mp.replace("\\040", " ");
        if canon.starts_with(&mp) {
            let network = matches!(
                fstype,
                "nfs" | "nfs4" | "cifs" | "smb3" | "fuse.rclone" | "fuse"
            );
            let len = mp.len();
            if best.is_none_or(|(l, _)| len > l) {
                best = Some((len, network));
            }
        }
    }
    best.is_some_and(|(_, n)| n)
}

#[cfg(target_os = "linux")]
fn inotify_watches_ok() -> bool {
    std::fs::read_to_string("/proc/sys/fs/inotify/max_user_watches")
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .is_some_and(|n| n >= 8192)
}

/// Scrive il probe hardware in `system_settings.capabilities`.
///
/// # Errors
/// Database.
pub async fn persist_capabilities(db: &Db) -> Result<(), JobError> {
    let caps = keeppix_media::probe();
    let value = serde_json::to_value(&caps).map_err(|e| JobError::Worker(e.to_string()))?;
    keeppix_db::SettingsRepo::new(db)
        .put_json("capabilities", &value)
        .await?;
    Ok(())
}
