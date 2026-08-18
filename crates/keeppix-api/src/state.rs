use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use keeppix_db::Db;
use keeppix_domain::{AuthContext, SessionToken, ShareToken};

use crate::ratelimit::RateLimiter;

#[derive(Clone, Default)]
pub struct TicketStore {
    inner: Arc<Mutex<HashMap<String, (AuthContext, Instant)>>>,
}

impl TicketStore {
    #[must_use]
    pub fn issue(&self, ctx: AuthContext) -> String {
        let id = uuid::Uuid::now_v7().simple().to_string();
        if let Ok(mut guard) = self.inner.lock() {
            sweep_expired(&mut guard);
            cap_user_tickets(&mut guard, ctx.user_id(), 7);
            guard.insert(id.clone(), (ctx, Instant::now() + Duration::from_secs(30)));
        }
        id
    }

    #[must_use]
    pub fn consume(&self, id: &str) -> Option<AuthContext> {
        let mut guard = self.inner.lock().ok()?;
        let (ctx, expires) = guard.remove(id)?;
        (Instant::now() <= expires).then_some(ctx)
    }
}

fn sweep_expired<K, V>(map: &mut HashMap<K, (V, Instant)>) {
    let now = Instant::now();
    map.retain(|_, (_, exp)| *exp > now);
}

fn cap_user_tickets(
    map: &mut HashMap<String, (AuthContext, Instant)>,
    user: Option<keeppix_domain::UserId>,
    keep: usize,
) {
    let Some(uid) = user else {
        return;
    };
    let mine: Vec<String> = map
        .iter()
        .filter(|(_, (ctx, _))| ctx.user_id() == Some(uid))
        .map(|(k, _)| k.clone())
        .collect();
    for extra in mine.into_iter().skip(keep) {
        map.remove(&extra);
    }
}

type CachedSession = (AuthContext, Instant);

#[derive(Clone, Default)]
pub struct SessionCache {
    // ponytail: keyed by token digest only. A family-wide revoke leaves
    // sibling tokens cached up to 30s. Index by family_id if theft-detection
    // must be immediate on every device.
    inner: Arc<Mutex<HashMap<[u8; 32], CachedSession>>>,
}

impl SessionCache {
    #[must_use]
    pub fn get(&self, token: &SessionToken) -> Option<AuthContext> {
        let digest = token.digest();
        let mut guard = self.inner.lock().ok()?;
        let (ctx, expires) = guard.get(&digest).cloned()?;
        if Instant::now() > expires {
            guard.remove(&digest);
            return None;
        }
        Some(ctx)
    }

    pub fn put(&self, token: &SessionToken, ctx: AuthContext) {
        if let Ok(mut guard) = self.inner.lock() {
            sweep_expired(&mut guard);
            guard.insert(
                token.digest(),
                (ctx, Instant::now() + Duration::from_secs(30)),
            );
        }
    }

    pub fn drop_token(&self, token: &SessionToken) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.remove(&token.digest());
        }
    }

    /// Svuota la cache. Usato dopo revoke di massa (disable / cambio password)
    /// così un token già revocato non resta valido fino a 30s.
    pub fn clear(&self) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.clear();
        }
    }
}

const SHARE_UNLOCK_TTL: Duration = Duration::from_secs(60 * 60);

type UnlockMap = HashMap<[u8; 32], (uuid::Uuid, Instant)>;

/// Opaque unlock tokens issued after a correct share-link password.
/// In-process on purpose (same reason as the rate limiter: single node, no
/// Redis). A restart asks guests to re-enter the password.
#[derive(Clone, Default)]
pub struct ShareUnlockStore {
    inner: Arc<Mutex<UnlockMap>>,
}

impl ShareUnlockStore {
    #[must_use]
    pub fn issue(&self, link_id: uuid::Uuid) -> ShareToken {
        let token = ShareToken::generate();
        if let Ok(mut guard) = self.inner.lock() {
            sweep_expired(&mut guard);
            guard.insert(token.digest(), (link_id, Instant::now() + SHARE_UNLOCK_TTL));
        }
        token
    }

    #[must_use]
    pub fn check(&self, link_id: uuid::Uuid, token: &ShareToken) -> bool {
        let Ok(mut guard) = self.inner.lock() else {
            return false;
        };
        let Some((stored_link, expires)) = guard.get(&token.digest()).copied() else {
            return false;
        };
        if Instant::now() > expires {
            guard.remove(&token.digest());
            return false;
        }
        stored_link == link_id
    }
}

#[derive(Clone)]
pub struct AppState {
    pub db: Db,
    pub session_ttl: Duration,
    pub data_dir: PathBuf,
    pub on_authenticated: Option<Arc<dyn Fn() + Send + Sync>>,
    pub tickets: TicketStore,
    pub sessions: SessionCache,
    pub allowed_origins: Vec<String>,
    /// Radici sotto le quali può puntare una libreria. Default produzione:
    /// `["/photos"]`. Validare `root_path` **dopo** `canonicalize`.
    pub library_roots: Vec<PathBuf>,
    /// Watcher delle librerie; `None` solo nei test che non li esercitano.
    pub library_watchers: Option<keeppix_jobs::watch::LibraryWatchers>,
    /// Tetto in byte della cache `*-full.webp`. Default 512 MiB.
    pub full_cache_bytes: u64,
    /// Demosaic RAW per `/media/full`. In produzione è `SandboxDemosaic`;
    /// i test iniettano un finto per non dipendere da `dcraw_emu`.
    pub demosaic: std::sync::Arc<dyn keeppix_jobs::raw::Demosaic>,
    /// Rate limiter for public share link access (per token).
    pub share_limiter: RateLimiter,
    /// Rate limiter for login attempts (per IP/username).
    pub login_limiter: RateLimiter,
    /// In-process unlock proofs for password-protected share links.
    pub share_unlocks: ShareUnlockStore,
}

impl AppState {
    #[must_use]
    pub fn new(db: Db, session_ttl_secs: u64, data_dir: PathBuf) -> Self {
        Self {
            db,
            session_ttl: Duration::from_secs(session_ttl_secs),
            data_dir,
            on_authenticated: None,
            tickets: TicketStore::default(),
            sessions: SessionCache::default(),
            allowed_origins: Vec::new(),
            library_roots: vec![PathBuf::from("/photos")],
            library_watchers: None,
            full_cache_bytes: keeppix_media::DEFAULT_FULL_CACHE_BYTES,
            demosaic: std::sync::Arc::new(keeppix_jobs::raw::SandboxDemosaic),
            share_limiter: RateLimiter::new(Duration::from_secs(60), 60),
            login_limiter: RateLimiter::new(Duration::from_secs(300), 10),
            share_unlocks: ShareUnlockStore::default(),
        }
    }

    #[must_use]
    pub fn with_on_authenticated(mut self, hook: Arc<dyn Fn() + Send + Sync>) -> Self {
        self.on_authenticated = Some(hook);
        self
    }

    #[must_use]
    pub fn with_allowed_origins(mut self, origins: Vec<String>) -> Self {
        self.allowed_origins = origins;
        self
    }

    #[must_use]
    pub fn with_library_roots(mut self, roots: Vec<PathBuf>) -> Self {
        self.library_roots = roots;
        self
    }

    #[must_use]
    pub fn with_library_watchers(mut self, watchers: keeppix_jobs::watch::LibraryWatchers) -> Self {
        self.library_watchers = Some(watchers);
        self
    }

    #[must_use]
    pub fn with_full_cache_bytes(mut self, bytes: u64) -> Self {
        self.full_cache_bytes = bytes;
        self
    }

    #[must_use]
    pub fn with_demosaic(mut self, demosaic: Arc<dyn keeppix_jobs::raw::Demosaic>) -> Self {
        self.demosaic = demosaic;
        self
    }

    #[must_use]
    pub const fn with_session_ttl(mut self, ttl: Duration) -> Self {
        self.session_ttl = ttl;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use keeppix_domain::{SystemRole, UserId};

    #[test]
    #[allow(clippy::unwrap_used)]
    fn issue_drops_expired_tickets() {
        let store = TicketStore::default();
        let ctx = AuthContext::user(UserId::new(), SystemRole::User);
        {
            let mut guard = store.inner.lock().unwrap();
            guard.insert(
                "stale".to_owned(),
                (
                    ctx.clone(),
                    Instant::now().checked_sub(Duration::from_secs(1)).unwrap(),
                ),
            );
        }
        let _issued = store.issue(ctx);
        let guard = store.inner.lock().unwrap();
        assert!(!guard.contains_key("stale"));
    }
}
