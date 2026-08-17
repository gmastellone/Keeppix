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
    /// Unica impostazione obbligatoria.
    pub database_url: String,
    pub bind: SocketAddr,
    pub data_dir: PathBuf,
    pub db_max_connections: u32,
    pub session_ttl_secs: u64,
    pub log_format: LogFormat,
    /// Origini ammesse per CORS e per la verifica dell'`Origin` sul WebSocket.
    pub allowed_origins: Vec<String>,
    /// Radici sotto cui può vivere una libreria (`KEEPPIX_LIBRARY_ROOTS`).
    /// Un `root_path` fuori da queste → `422 keeppix/path-not-allowed`.
    pub library_roots: Vec<PathBuf>,
    /// Intervallo del watcher in modo polling (`KEEPPIX_WATCH_POLL_SECS`).
    /// Default 15 minuti: su un Pi non si vuole una riscansione continua.
    pub watch_poll_secs: u64,
    /// Qualità WebP con perdita dei derivati (`KEEPPIX_WEBP_QUALITY`).
    /// Default 82: sotto 75 si vede, sopra 88 si paga per poco.
    pub webp_quality: u8,
    /// Metodo libwebp 0–6 (`KEEPPIX_WEBP_METHOD`). 0 è veloce e più
    /// grosso; 4 è lento. Default 2: in release ~2× più veloce di 4 con
    /// ~3% in più di peso, rapporto derivati ancora sotto l'1%.
    pub webp_method: u8,
    /// Tetto della cache dei derivati `full` pigri (`KEEPPIX_FULL_CACHE_BYTES`).
    pub full_cache_bytes: u64,
    /// Giorni prima che il cestino si svuoti da solo (`KEEPPIX_TRASH_RETENTION_DAYS`).
    pub trash_retention_days: i64,
}

/// Valori usati quando né l'ambiente né il file dicono nulla.
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
        }
    }
}

impl Config {
    /// Precedenza: variabili d'ambiente → file toml → default.
    ///
    /// # Errors
    /// Se `DATABASE_URL` manca o se un valore non è del tipo atteso.
    pub fn load(config_path: Option<&Path>) -> Result<Self, anyhow::Error> {
        let mut figment = Figment::from(Serialized::defaults(Defaults::default()));

        if let Some(path) = config_path
            && path.exists()
        {
            figment = figment.merge(Toml::file(path));
        }

        let figment = figment
            // `split(",")` così `KEEPPIX_LIBRARY_ROOTS=/photos,/data/extra`
            // (e `allowed_origins`) diventano liste senza JSON.
            .merge(Env::prefixed("KEEPPIX_").split(","))
            // `DATABASE_URL` senza prefisso: è la convenzione attesa da chiunque.
            .merge(Env::raw().only(&["DATABASE_URL"]));

        figment.extract().map_err(|e| {
            if e.to_string().contains("database_url") {
                // Messaggio interamente in inglese: è un errore di avvio
                // rivolto all'operatore, e la localizzazione è compito del
                // frontend. `es.` era italiano in una frase inglese.
                anyhow::anyhow!("DATABASE_URL is required (e.g. postgres://user:pw@host/keeppix)")
            } else {
                anyhow::Error::new(e)
            }
        })
    }
}
