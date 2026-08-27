use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use figment::Figment;
use figment::providers::{Env, Format as _, Serialized, Toml};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    Json,
    Pretty,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// The only required setting.
    pub database_url: String,
    pub bind: SocketAddr,
    pub data_dir: PathBuf,
    pub db_max_connections: u32,
    pub session_ttl_secs: u64,
    pub log_format: LogFormat,
    /// Allowed origins for CORS and for verifying the WebSocket `Origin`.
    pub allowed_origins: Vec<String>,
    /// Roots under which a library may live (`KEEPPIX_LIBRARY_ROOTS`).
    /// A `root_path` outside these roots → `422 keeppix/path-not-allowed`.
    pub library_roots: Vec<PathBuf>,
    /// Watcher interval in polling mode (`KEEPPIX_WATCH_POLL_SECS`).
    /// Default 15 minutes: continuous rescanning is not something you want
    /// on a Pi.
    pub watch_poll_secs: u64,
    /// Lossy WebP quality for derivatives (`KEEPPIX_WEBP_QUALITY`).
    /// Default 82: below 75 the loss is visible, above 88 you're paying for
    /// little gain.
    pub webp_quality: u8,
    /// libwebp method 0-6 (`KEEPPIX_WEBP_METHOD`). 0 is fast and produces
    /// larger files; 4 is slow. Default 2: in release builds roughly 2x
    /// faster than 4 with about 3% more weight, with the derivative ratio
    /// still under 1%.
    pub webp_method: u8,
    /// Cap on the lazy `full`-size derivative cache (`KEEPPIX_FULL_CACHE_BYTES`).
    pub full_cache_bytes: u64,
    /// Days before the trash empties itself (`KEEPPIX_TRASH_RETENTION_DAYS`).
    pub trash_retention_days: i64,
    /// Name shown in the Profile (`KEEPPIX_SERVER_NAME`): "account on this
    /// Keeppix server". Purely cosmetic, no other behavior depends on this
    /// value.
    pub server_name: String,
}

/// Values used when neither the environment nor the config file say
/// anything.
#[derive(Debug, Serialize)]
struct Defaults {
    bind: SocketAddr,
    data_dir: PathBuf,
    db_max_connections: u32,
    session_ttl_secs: u64,
    log_format: LogFormat,
    allowed_origins: Vec<String>,
    library_roots: Vec<PathBuf>,
    watch_poll_secs: u64,
    webp_quality: u8,
    webp_method: u8,
    full_cache_bytes: u64,
    trash_retention_days: i64,
    server_name: String,
}

impl Default for Defaults {
    fn default() -> Self {
        Self {
            bind: SocketAddr::from(([0, 0, 0, 0], 5673)),
            data_dir: PathBuf::from("/data"),
            db_max_connections: 10,
            session_ttl_secs: 60 * 60 * 24 * 30,
            log_format: LogFormat::Json,
            allowed_origins: Vec::new(),
            library_roots: vec![PathBuf::from("/photos")],
            watch_poll_secs: 15 * 60,
            webp_quality: keeppix_jobs::DEFAULT_WEBP_QUALITY,
            webp_method: keeppix_jobs::DEFAULT_WEBP_METHOD,
            full_cache_bytes: keeppix_jobs::DEFAULT_FULL_CACHE_BYTES,
            trash_retention_days: keeppix_db::TRASH_RETENTION_DAYS,
            server_name: "Keeppix".to_owned(),
        }
    }
}

impl Config {
    /// Precedence: environment variables → toml file → defaults.
    ///
    /// # Errors
    /// If `DATABASE_URL` is missing, or if a value isn't of the expected
    /// type.
    pub fn load(config_path: Option<&Path>) -> Result<Self, anyhow::Error> {
        let mut figment = Figment::from(Serialized::defaults(Defaults::default()));

        if let Some(path) = config_path
            && path.exists()
        {
            figment = figment.merge(Toml::file(path));
        }

        let figment = figment
            // `split(",")` so that `KEEPPIX_LIBRARY_ROOTS=/photos,/data/extra`
            // (and `allowed_origins`) become lists without needing JSON.
            .merge(Env::prefixed("KEEPPIX_").split(","))
            // `DATABASE_URL` without a prefix: it's the convention every
            // operator expects.
            .merge(Env::raw().only(&["DATABASE_URL"]));

        figment.extract().map_err(|e| {
            if e.to_string().contains("database_url") {
                // Message entirely in English: this is a startup error
                // aimed at the operator, and localization is the frontend's
                // job. Watch for stray non-English words creeping into this
                // string, as has happened before.
                anyhow::anyhow!("DATABASE_URL is required (e.g. postgres://user:pw@host/keeppix)")
            } else {
                anyhow::Error::new(e)
            }
        })
    }
}
