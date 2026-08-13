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
            .merge(Env::prefixed("KEEPPIX_"))
            // `DATABASE_URL` senza prefisso: è la convenzione attesa da chiunque.
            .merge(Env::raw().only(&["DATABASE_URL"]));

        figment.extract().map_err(|e| {
            if e.to_string().contains("database_url") {
                anyhow::anyhow!("DATABASE_URL is required (es. postgres://user:pw@host/keeppix)")
            } else {
                anyhow::Error::new(e)
            }
        })
    }
}
