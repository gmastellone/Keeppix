use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use keeppix_db::Db;
use keeppix_domain::AuthContext;

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

#[derive(Clone)]
pub struct AppState {
    pub db: Db,
    pub session_ttl: Duration,
    pub data_dir: PathBuf,
    pub on_authenticated: Option<Arc<dyn Fn() + Send + Sync>>,
    pub tickets: TicketStore,
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
