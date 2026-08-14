use std::time::Duration;

use keeppix_db::Db;

#[derive(Clone)]
pub struct AppState {
    pub db: Db,
    pub session_ttl: Duration,
}

impl AppState {
    #[must_use]
    pub const fn new(db: Db, session_ttl_secs: u64) -> Self {
        Self {
            db,
            session_ttl: Duration::from_secs(session_ttl_secs),
        }
    }
}
