## Task 8: Configurazione, telemetria e CLI del server

**Files:**
- Create: `crates/keeppix-server/src/config.rs`
- Create: `crates/keeppix-server/src/telemetry.rs`
- Create: `crates/keeppix-server/tests/config.rs`
- Create: `.env.example`
- Modify: `crates/keeppix-server/src/main.rs`, `crates/keeppix-server/Cargo.toml`

**Interfaces:**
- Consumes: `Db` (Task 4).
- Produces:
  - `Config { database_url: String, bind: SocketAddr, data_dir: PathBuf, db_max_connections: u32, session_ttl_secs: u64, log_format: LogFormat, allowed_origins: Vec<String> }` con `Config::load(config_path: Option<&Path>) -> Result<Config, anyhow::Error>`.
  - `LogFormat::{Json, Pretty}`.
  - `telemetry::init(format: LogFormat)`.
  - CLI: `keeppix serve` (default), `keeppix migrate`, `keeppix healthcheck`.

Precedenza: variabili d'ambiente con prefisso `KEEPPIX_` → `config.toml` → default. `DATABASE_URL` è accettata anche senza prefisso, perché è la convenzione che tutti si aspettano.

- [ ] **Step 1: Aggiungere le dipendenze**

```bash
cargo add figment --features toml,env -p keeppix-server
cargo add clap --features derive,env -p keeppix-server
cargo add tracing-subscriber --features json,env-filter -p keeppix-server
cargo add axum tower-http --features fs,trace,compression-br,set-header,cors -p keeppix-server
cargo add serde anyhow keeppix-api --path crates/keeppix-api -p keeppix-server
cargo add --dev tempfile -p keeppix-server
```

- [ ] **Step 2: Scrivere i test che falliscono**

`crates/keeppix-server/tests/config.rs`:

```rust
use std::io::Write as _;

use keeppix_server::config::{Config, LogFormat};

/// I test manipolano variabili d'ambiente di processo: vanno eseguiti in serie.
/// `cargo test -- --test-threads=1` è imposto dallo script di verifica.
fn clear_env() {
    for key in ["DATABASE_URL", "KEEPPIX_BIND", "KEEPPIX_DATA_DIR", "KEEPPIX_LOG_FORMAT"] {
        unsafe { std::env::remove_var(key) };
    }
}

#[test]
fn database_url_is_required() {
    clear_env();
    assert!(Config::load(None).is_err(), "senza DATABASE_URL il caricamento fallisce");
}

#[test]
fn defaults_are_applied() {
    clear_env();
    unsafe { std::env::set_var("DATABASE_URL", "postgres://localhost/keeppix") };

    let cfg = Config::load(None).unwrap();
    assert_eq!(cfg.bind.port(), 5673);
    assert_eq!(cfg.data_dir, std::path::PathBuf::from("/data"));
    assert_eq!(cfg.session_ttl_secs, 60 * 60 * 24 * 30);
    assert!(matches!(cfg.log_format, LogFormat::Json));
}

#[test]
fn environment_overrides_the_file() {
    clear_env();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let mut file = std::fs::File::create(&path).unwrap();
    writeln!(file, "database_url = \"postgres://from-file/keeppix\"").unwrap();
    writeln!(file, "bind = \"0.0.0.0:1111\"").unwrap();

    unsafe { std::env::set_var("KEEPPIX_BIND", "0.0.0.0:2222") };

    let cfg = Config::load(Some(&path)).unwrap();
    assert_eq!(cfg.bind.port(), 2222, "l'ambiente vince sul file");
    assert_eq!(cfg.database_url, "postgres://from-file/keeppix", "il file vince sul default");
}

#[test]
fn bare_database_url_is_accepted() {
    clear_env();
    unsafe { std::env::set_var("DATABASE_URL", "postgres://bare/keeppix") };
    assert_eq!(Config::load(None).unwrap().database_url, "postgres://bare/keeppix");
}
```

- [ ] **Step 3: Eseguire e verificare il fallimento**

Run: `cargo test -p keeppix-server --test config -- --test-threads=1`
Expected: FAIL — `unresolved import keeppix_server::config`.

- [ ] **Step 4: Trasformare il binario in libreria + binario**

In `crates/keeppix-server/Cargo.toml`, prima di `[[bin]]`:

```toml
[lib]
name = "keeppix_server"
path = "src/lib.rs"
```

Creare `crates/keeppix-server/src/lib.rs`:

```rust
pub mod config;
pub mod telemetry;
```

- [ ] **Step 5: Implementare `config.rs`**

```rust
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
            bind: "0.0.0.0:5673".parse().expect("literal socket address"),
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
```

- [ ] **Step 6: Implementare `telemetry.rs`**

```rust
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt;

use crate::config::LogFormat;

/// Inizializza il logging. `RUST_LOG` sovrascrive il livello predefinito.
pub fn init(format: LogFormat) {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,sqlx=warn,tower_http=info"));

    match format {
        LogFormat::Json => fmt().json().with_env_filter(filter).init(),
        LogFormat::Pretty => fmt().pretty().with_env_filter(filter).init(),
    }
}
```

- [ ] **Step 7: Eseguire i test della configurazione**

Run: `cargo test -p keeppix-server --test config -- --test-threads=1`
Expected: PASS — 4 test.

- [ ] **Step 8: Scrivere `main.rs` con i tre sottocomandi**

```rust
use std::path::PathBuf;

use anyhow::Context as _;
use clap::{Parser, Subcommand};
use keeppix_db::Db;
use keeppix_server::config::Config;
use keeppix_server::telemetry;

#[derive(Parser)]
#[command(name = "keeppix", version)]
struct Cli {
    /// Percorso del file di configurazione.
    #[arg(long, env = "KEEPPIX_CONFIG", default_value = "/data/config.toml")]
    config: PathBuf,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Avvia il server (comportamento predefinito).
    Serve,
    /// Applica le migrazioni ed esce.
    Migrate,
    /// Verifica che il server locale risponda. Usato da HEALTHCHECK in Docker,
    /// dove non esistono né shell né curl.
    Healthcheck,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    if matches!(cli.command, Some(Command::Healthcheck)) {
        return healthcheck().await;
    }

    let config = Config::load(Some(&cli.config))?;
    telemetry::init(config.log_format);

    let db = Db::connect(&config.database_url, config.db_max_connections)
        .await
        .context("connessione al database")?;
    db.migrate().await.context("applicazione delle migrazioni")?;

    match cli.command {
        Some(Command::Migrate) => {
            tracing::info!("migrations applied");
            Ok(())
        }
        _ => serve(config, db).await,
    }
}

async fn serve(config: Config, db: Db) -> anyhow::Result<()> {
    let listener = tokio::net::TcpListener::bind(config.bind).await?;
    tracing::info!(addr = %config.bind, "keeppix listening");

    let app = keeppix_api::router(keeppix_api::AppState::new(db, config.session_ttl_secs));

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

/// Chiusura garbata su SIGTERM (Docker) e Ctrl-C (sviluppo).
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c().await.ok();
    };

    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut sig) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            sig.recv().await;
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
    tracing::info!("shutting down");
}

async fn healthcheck() -> anyhow::Result<()> {
    let port = std::env::var("KEEPPIX_BIND")
        .ok()
        .and_then(|b| b.rsplit(':').next().and_then(|p| p.parse::<u16>().ok()))
        .unwrap_or(5673);

    let stream = tokio::net::TcpStream::connect(("127.0.0.1", port)).await?;
    drop(stream);
    Ok(())
}
```

- [ ] **Step 9: Scrivere `.env.example`**

```bash
# Unica variabile obbligatoria.
DATABASE_URL=postgres://keeppix:changeme@localhost:5432/keeppix

# Opzionali: mostrati con i valori predefiniti.
# KEEPPIX_BIND=0.0.0.0:5673
# KEEPPIX_DATA_DIR=/data
# KEEPPIX_DB_MAX_CONNECTIONS=10
# KEEPPIX_SESSION_TTL_SECS=2592000
# KEEPPIX_LOG_FORMAT=json
# KEEPPIX_ALLOWED_ORIGINS=["https://foto.example.com"]
# RUST_LOG=info,sqlx=warn
```

- [ ] **Step 10: Verificare compilazione e lint**

Il codice non compilerà finché `keeppix_api::router` e `AppState` non esistono (Task 9). Verificare solo il crate config:

Run: `cargo test -p keeppix-server --test config -- --test-threads=1 && cargo fmt --check`
Expected: PASS.

- [ ] **Step 11: Commit**

```bash
git add crates/keeppix-server .env.example
git commit -m "feat(server): add layered config, telemetry and cli subcommands"
```

---

