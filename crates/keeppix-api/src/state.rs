use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use keeppix_db::Db;
use keeppix_domain::{AuthContext, SessionToken};

#[derive(Clone, Default)]
pub struct TicketStore {
    inner: Arc<Mutex<HashMap<String, (AuthContext, Instant)>>>,
}

impl TicketStore {
    #[must_use]
    pub fn issue(&self, ctx: AuthContext) -> String {
        let id = uuid::Uuid::now_v7().simple().to_string();
        if let Ok(mut guard) = self.inner.lock() {
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
}
