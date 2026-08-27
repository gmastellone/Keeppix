use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use keeppix_db::{Db, JobRepo, LibraryRepo};
use keeppix_domain::{JobKind, JobPriority, LibraryId, OperationId};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher as _};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::JobError;
use crate::discover;

pub const DEFAULT_DEBOUNCE: Duration = Duration::from_secs(2);
pub const DEFAULT_POLL: Duration = Duration::from_secs(15 * 60);
/// No more than one native rescan every so often, even if events keep
/// arriving. Closes the Access→stat→Access loop and covers noisy editors.
pub const MIN_RESCAN: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatcherMode {
    Native,
    Polling { every: Duration },
}

/// NFS/SMB/rclone → polling. On Linux, missing or too-low inotify → polling.
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

/// Enqueue a rescan. Deduplicated on the `discover:{id}` key.
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

/// Like [`enqueue_rescan`], but carries an `operation_id` to advance via
/// `operations` while the scan proceeds.
///
/// Shares the same `dedup_key` as [`enqueue_rescan`]: if a scan for this
/// library is already `pending`/`running` (enqueued by the watcher or by an
/// earlier request), that one wins and the returned payload does not carry
/// our `operation_id`. Returns `true` only if the enqueued job is really
/// the one that follows our operation — the caller must close the
/// operation, not leave it `running` forever with no job advancing it.
///
/// # Errors
/// Database.
pub async fn enqueue_rescan_with_operation(
    db: &Db,
    library_id: LibraryId,
    operation_id: OperationId,
) -> Result<bool, JobError> {
    let job = JobRepo::new(db)
        .enqueue(
            JobKind::DiscoverLibrary,
            serde_json::json!({
                "library_id": library_id.to_string(),
                "operation_id": operation_id.to_string(),
            }),
            JobPriority::Background,
            Some(&format!("discover:{library_id}")),
        )
        .await?;
    Ok(discover::operation_id_from_payload(&job.payload) == Some(operation_id))
}

/// Registry of active watchers: libraries created after boot must be
/// watched without restarting the process.
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

    /// Starts a watcher for every library already present (boot).
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

    /// Starts the watcher for a library if it isn't already active.
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

/// Starts a watcher for every already-known library and returns the
/// registry to keep alive (and pass to `AppState`) so subsequent creates
/// can call [`LibraryWatchers::ensure`].
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
                    // Minimum cadence per library, independent of event
                    // volume. The first rescan (last_rescan = None) doesn't
                    // wait.
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

/// Writes the hardware probe to `system_settings.capabilities`. Measures
/// real video acceleration and AI host facts (RAM, cores, and the ms of
/// inference on the local model) and persists the JSON result.
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

/// Reads `extra.ai` back from `system_settings.capabilities`. This is the
/// reader `get_json` needed: the scheduler uses these numbers for the
/// Full/Reduced/Off levels.
///
/// # Errors
/// Database, o JSON corrotto.
pub async fn load_ai_host_facts(db: &Db) -> Result<Option<keeppix_media::AiHostFacts>, JobError> {
    let Some(value) = keeppix_db::SettingsRepo::new(db)
        .get_json("capabilities")
        .await?
    else {
        return Ok(None);
    };
    let Some(ai) = value.get("extra").and_then(|extra| extra.get("ai")) else {
        return Ok(None);
    };
    let facts: keeppix_media::AiHostFacts = serde_json::from_value(ai.clone())
        .map_err(|e| JobError::Worker(format!("capabilities.extra.ai: {e}")))?;
    Ok(Some(facts))
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
            "Access is not a modification: it must not trigger a rescan"
        );
    }

    #[test]
    fn modify_on_a_visible_file_stays_interesting() {
        let ev = event(
            EventKind::Modify(ModifyKind::Data(DataChange::Any)),
            "/photos/DSC_0042.ARW",
        );
        assert!(interesting(&ev), "Modify must keep triggering discovery");
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
            "the first rescan must not wait"
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
