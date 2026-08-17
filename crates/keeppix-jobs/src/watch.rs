use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use keeppix_db::{Db, JobRepo, LibraryRepo};
use keeppix_domain::{JobKind, JobPriority, LibraryId};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher as _};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::JobError;

pub const DEFAULT_DEBOUNCE: Duration = Duration::from_secs(2);
pub const DEFAULT_POLL: Duration = Duration::from_secs(15 * 60);
/// Non più di una riscansione nativa ogni tanto, anche se gli eventi
/// continuano ad arrivare. Chiude il ciclo Access→stat→Access (D4) e
/// copre editor rumorosi.
pub const MIN_RESCAN: Duration = Duration::from_secs(30);

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

/// Registro dei watcher attivi: le librerie create dopo il boot devono
/// essere sorvegliate senza riavviare il processo.
#[derive(Clone)]
pub struct LibraryWatchers {
    db: Db,
    debounce: Duration,
    poll: Duration,
    inner: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<LibraryId, JoinHandle<()>>>>,
}

impl LibraryWatchers {
    #[must_use]
    pub fn new(db: Db, debounce: Duration) -> Self {
        Self::with_poll(db, debounce, DEFAULT_POLL)
    }

    #[must_use]
    pub fn with_poll(db: Db, debounce: Duration, poll: Duration) -> Self {
        Self {
            db,
            debounce,
            poll,
            inner: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
        }
    }

    /// Avvia un watcher per ogni libreria già presente (boot).
    ///
    /// # Errors
    /// Database.
    pub async fn spawn_existing(&self) -> Result<(), JobError> {
        let libs = LibraryRepo::new(&self.db).list_for_scan().await?;
        for lib in libs {
            self.ensure(lib.id, lib.root_path);
        }
        Ok(())
    }

    /// Avvia il watcher per una libreria se non è già attivo.
    pub fn ensure(&self, library_id: LibraryId, root: PathBuf) {
        let Ok(mut guard) = self.inner.lock() else {
            return;
        };
        if guard.contains_key(&library_id) {
            return;
        }
        let mut mode = mode_for(&root);
        if matches!(mode, WatcherMode::Polling { .. }) {
            mode = WatcherMode::Polling { every: self.poll };
        }
        if let WatcherMode::Polling { every } = mode {
            tracing::warn!(
                library = %library_id,
                root = %root.display(),
                poll_secs = every.as_secs(),
                "watcher_mode=polling"
            );
        }
        let handle = spawn(self.db.clone(), library_id, root, self.debounce, mode);
        guard.insert(library_id, handle);
    }
}

/// Avvia un watcher per ogni libreria già nota e restituisce il registro
/// da tenere vivo (e da passare a `AppState`) così le create successive
/// possono chiamare [`LibraryWatchers::ensure`].
///
/// # Errors
/// Database.
pub async fn spawn_all(
    db: &Db,
    debounce: Duration,
    poll: Duration,
) -> Result<LibraryWatchers, JobError> {
    let watchers = LibraryWatchers::with_poll(db.clone(), debounce, poll);
    watchers.spawn_existing().await?;
    Ok(watchers)
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
    let mut last_rescan: Option<Instant> = None;
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
                    // Cadenza minima per libreria, indipendente dal volume
                    // di eventi. La prima riscansione (last_rescan = None)
                    // non attende.
                    if let Some(last) = last_rescan
                        && !due_for_rescan(Some(last), Instant::now(), MIN_RESCAN)
                    {
                        tokio::time::sleep(MIN_RESCAN.saturating_sub(last.elapsed())).await;
                    }
                    enqueue_rescan(&db, library_id).await?;
                    last_rescan = Some(Instant::now());
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
    matches!(
        event.kind,
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
    ) && event.paths.iter().any(|p| !is_hidden_path(p))
}

fn due_for_rescan(last: Option<Instant>, now: Instant, min: Duration) -> bool {
    last.is_none_or(|t| now.saturating_duration_since(t) >= min)
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
/// Oggi il valore è `"unprobed"`: non c'è un rilevamento (Fase 6).
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

#[cfg(test)]
mod tests {
    use super::*;
    use notify::EventKind;
    use notify::event::{AccessKind, AccessMode, DataChange, EventAttributes, ModifyKind};

    fn event(kind: EventKind, path: &str) -> Event {
        Event {
            kind,
            paths: vec![PathBuf::from(path)],
            attrs: EventAttributes::new(),
        }
    }

    #[test]
    fn access_on_a_visible_file_is_not_interesting() {
        let ev = event(
            EventKind::Access(AccessKind::Open(AccessMode::Any)),
            "/photos/DSC_0042.ARW",
        );
        assert!(
            !interesting(&ev),
            "Access non è una modifica: non deve innescare una riscansione (D4)"
        );
    }

    #[test]
    fn modify_on_a_visible_file_stays_interesting() {
        let ev = event(
            EventKind::Modify(ModifyKind::Data(DataChange::Any)),
            "/photos/DSC_0042.ARW",
        );
        assert!(
            interesting(&ev),
            "Modify deve continuare a innescare la discovery"
        );
    }

    #[test]
    fn create_and_remove_on_a_visible_file_are_interesting() {
        assert!(interesting(&event(
            EventKind::Create(notify::event::CreateKind::File),
            "/photos/nuova.ARW",
        )));
        assert!(interesting(&event(
            EventKind::Remove(notify::event::RemoveKind::File),
            "/photos/vecchia.ARW",
        )));
    }

    #[test]
    fn generic_any_and_other_are_not_interesting() {
        assert!(!interesting(&event(EventKind::Any, "/photos/DSC.ARW")));
        assert!(!interesting(&event(EventKind::Other, "/photos/DSC.ARW")));
    }

    #[test]
    fn a_hidden_path_is_never_interesting_even_on_modify() {
        let ev = event(
            EventKind::Modify(ModifyKind::Data(DataChange::Any)),
            "/photos/.DS_Store",
        );
        assert!(!interesting(&ev));
    }

    #[test]
    fn rescan_is_due_only_after_the_minimum_interval() {
        let t0 = std::time::Instant::now();
        assert!(
            due_for_rescan(None, t0, MIN_RESCAN),
            "la prima riscansione non deve aspettare"
        );
        assert!(!due_for_rescan(
            Some(t0),
            t0 + Duration::from_secs(29),
            MIN_RESCAN
        ));
        assert!(due_for_rescan(
            Some(t0),
            t0 + Duration::from_secs(30),
            MIN_RESCAN
        ));
    }
}
