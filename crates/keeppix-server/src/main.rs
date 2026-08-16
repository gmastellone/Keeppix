use std::path::{Path, PathBuf};

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
        return healthcheck(&cli.config).await;
    }

    let config = Config::load(Some(&cli.config))?;
    telemetry::init(config.log_format);

    let db = Db::connect(&config.database_url, config.db_max_connections)
        .await
        .context("connessione al database")?;
    db.migrate()
        .await
        .context("applicazione delle migrazioni")?;

    match cli.command {
        Some(Command::Migrate) => {
            tracing::info!("migrations applied");
            Ok(())
        }
        _ => serve(config, db).await,
    }
}

async fn serve(config: Config, db: Db) -> anyhow::Result<()> {
    if let Err(e) = keeppix_jobs::watch::persist_capabilities(&db).await {
        tracing::warn!(error = %e, "hardware probe failed");
    }
    let library_watchers =
        match keeppix_jobs::watch::spawn_all(&db, keeppix_jobs::watch::DEFAULT_DEBOUNCE).await {
            Ok(watchers) => Some(watchers),
            Err(e) => {
                tracing::warn!(error = %e, "library watchers failed to start");
                None
            }
        };

    let handler = keeppix_jobs::IngestHandler {
        db: db.clone(),
        data_dir: config.data_dir.clone(),
        stability_wait: keeppix_jobs::PRODUCTION_SETTLED_AFTER,
    };
    let night = keeppix_jobs::default_night_window();
    let workers = keeppix_jobs::worker_count(
        std::thread::available_parallelism()
            .map(std::num::NonZero::get)
            .unwrap_or(2),
    );
    let tracker = std::sync::Arc::new(keeppix_jobs::ActivityTracker::new());
    let paused = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    for _ in 0..workers {
        // ponytail: each worker has its own 512 MiB RamGate. Share one Arc
        // if RSS climbs under parallel derives.
        let pool = keeppix_jobs::WorkerPool::new(
            db.clone(),
            handler.clone(),
            tracker.clone(),
            512 * 1024 * 1024,
            night,
            paused.clone(),
        );
        tokio::spawn(async move {
            loop {
                match pool.step().await {
                    Ok(true) => {}
                    Ok(false) => tokio::time::sleep(std::time::Duration::from_millis(250)).await,
                    Err(e) => tracing::error!(error = %e, "worker step"),
                }
            }
        });
    }

    let listener = tokio::net::TcpListener::bind(config.bind).await?;
    tracing::info!(addr = %config.bind, "keeppix listening");

    let app = keeppix_server::embed::mount(keeppix_api::router_parts()).with_state({
        let mut state =
            keeppix_api::AppState::new(db, config.session_ttl_secs, config.data_dir.clone())
                .with_on_authenticated({
                    let tracker = tracker.clone();
                    std::sync::Arc::new(move || tracker.notify_authenticated_request())
                })
                .with_allowed_origins(config.allowed_origins.clone())
                .with_library_roots(config.library_roots.clone());
        if let Some(watchers) = library_watchers {
            state = state.with_library_watchers(watchers);
        }
        state
    });

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

async fn healthcheck(config_path: &Path) -> anyhow::Result<()> {
    let config = Config::load(Some(config_path))?;

    let stream = tokio::net::TcpStream::connect(("127.0.0.1", config.bind.port())).await?;
    drop(stream);
    Ok(())
}
