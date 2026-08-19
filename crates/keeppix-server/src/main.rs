use std::path::{Path, PathBuf};

use anyhow::Context as _;
use clap::{Parser, Subcommand};
use keeppix_db::Db;
use keeppix_server::config::Config;
use keeppix_server::telemetry;

const PLACES_CSV_PATH: &str = "/usr/share/keeppix/places.csv";
const TZ_BOUNDARIES_CSV_PATH: &str = "/usr/share/keeppix/tz_boundaries.csv";

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
    keeppix_jobs::set_webp_quality(config.webp_quality);
    keeppix_jobs::set_webp_method(config.webp_method);

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
    let imported = keeppix_db::PlaceRepo::new(&db)
        .seed_from_csv_if_empty(Path::new(PLACES_CSV_PATH))
        .await
        .context("GeoNames places import")?;
    if imported > 0 {
        tracing::info!(places = imported, "GeoNames places imported");
    }
    let imported = keeppix_db::GeoRepo::new(&db)
        .seed_timezones_from_csv_if_empty(Path::new(TZ_BOUNDARIES_CSV_PATH))
        .await
        .context("timezone boundaries import")?;
    if imported > 0 {
        tracing::info!(timezones = imported, "timezone boundaries imported");
    }

    if let Err(e) = keeppix_jobs::watch::persist_capabilities(&db).await {
        tracing::warn!(error = %e, "hardware probe failed");
    }
    let library_watchers = match keeppix_jobs::watch::spawn_all(
        &db,
        keeppix_jobs::watch::DEFAULT_DEBOUNCE,
        std::time::Duration::from_secs(config.watch_poll_secs),
    )
    .await
    {
        Ok(watchers) => Some(watchers),
        Err(e) => {
            tracing::warn!(error = %e, "library watchers failed to start");
            None
        }
    };

    keeppix_jobs::regions::repair_interrupted_downloads(&db)
        .await
        .context("interrupted region download repair")?;
    let handler = keeppix_jobs::IngestHandler {
        db: db.clone(),
        data_dir: config.data_dir.clone(),
        stability_wait: keeppix_jobs::PRODUCTION_SETTLED_AFTER,
        trash_retention_days: config.trash_retention_days,
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

    spawn_maintenance(db.clone()).await;

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
                .with_library_roots(config.library_roots.clone())
                .with_full_cache_bytes(config.full_cache_bytes);
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

async fn spawn_maintenance(db: Db) {
    if let Err(e) = keeppix_jobs::cleanup_trash::schedule(&db).await {
        tracing::warn!(error = %e, "trash cleanup could not be scheduled");
    }
    if let Err(e) = keeppix_jobs::retry_derives::schedule(&db).await {
        tracing::warn!(error = %e, "error-asset retry could not be scheduled");
    }
    {
        let db = db.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(24 * 60 * 60));
            interval.tick().await;
            loop {
                interval.tick().await;
                if let Err(e) = keeppix_jobs::cleanup_trash::schedule(&db).await {
                    tracing::warn!(error = %e, "trash cleanup could not be scheduled");
                }
            }
        });
    }
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(15 * 60));
        interval.tick().await;
        loop {
            interval.tick().await;
            if let Err(e) = keeppix_jobs::retry_derives::schedule(&db).await {
                tracing::warn!(error = %e, "error-asset retry could not be scheduled");
            }
        }
    });
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
