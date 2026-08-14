use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use keeppix_db::Db;

#[derive(Clone)]
pub struct AppState {
    pub db: Db,
    pub session_ttl: Duration,
    pub data_dir: PathBuf,
    pub on_authenticated: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl AppState {
    #[must_use]
    pub fn new(db: Db, session_ttl_secs: u64, data_dir: PathBuf) -> Self {
        Self {
            db,
            session_ttl: Duration::from_secs(session_ttl_secs),
            data_dir,
            on_authenticated: None,
        }
    }

    #[must_use]
    pub fn with_on_authenticated(mut self, hook: Arc<dyn Fn() + Send + Sync>) -> Self {
        self.on_authenticated = Some(hook);
        self
    }
}
