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
        _ => serve(config, db, cli.config).await,
    }
}

async fn serve(config: Config, db: Db, config_path: PathBuf) -> anyhow::Result<()> {
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

    log_hardware_probe(&db).await;
    log_pgvector_status(&db).await;
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

    keeppix_jobs::regions::recover_interrupted_downloads(&db)
        .await
        .context("interrupted region download repair")?;
    let tracker = std::sync::Arc::new(keeppix_jobs::ActivityTracker::new());
    let handler = keeppix_jobs::IngestHandler {
        db: db.clone(),
        data_dir: config.data_dir.clone(),
        stability_wait: keeppix_jobs::PRODUCTION_SETTLED_AFTER,
        trash_retention_days: config.trash_retention_days,
        database_url: config.database_url.clone(),
        config_path: Some(config_path),
        activity: tracker.clone(),
    };
    let night = keeppix_jobs::default_night_window();
    let workers = keeppix_jobs::worker_count(
        std::thread::available_parallelism()
            .map(std::num::NonZero::get)
            .unwrap_or(2),
    );
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

    let mut state =
        keeppix_api::AppState::new(db, config.session_ttl_secs, config.data_dir.clone())
            .with_database_url(config.database_url.clone())
            .with_on_authenticated({
                let tracker = tracker.clone();
                std::sync::Arc::new(move || tracker.notify_authenticated_request())
            })
            .with_on_viewport_activity({
                let tracker = tracker.clone();
                std::sync::Arc::new(move || tracker.notify_viewport_activity())
            })
            .with_allowed_origins(config.allowed_origins.clone())
            .with_library_roots(config.library_roots.clone())
            .with_full_cache_bytes(config.full_cache_bytes)
            .with_server_name(config.server_name.clone());
    if let Some(watchers) = library_watchers {
        state = state.with_library_watchers(watchers);
    }
    let app = keeppix_server::embed::mount(keeppix_api::router_parts(state));

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn log_hardware_probe(db: &Db) {
    if let Err(e) = keeppix_jobs::watch::persist_capabilities(db).await {
        tracing::warn!(error = %e, "hardware probe failed");
        return;
    }
    if let Ok(Some(ai)) = keeppix_jobs::watch::load_ai_host_facts(db).await {
        tracing::info!(
            free_ram_bytes = ai.free_ram_bytes,
            cpu_cores = ai.cpu_cores,
            inference_status = %ai.inference_status,
            "ai host probe"
        );
    }
}

/// Fase 7 Task 3: se pgvector manca (Postgres esterno senza l'estensione)
/// Keeppix parte comunque; AI resta spenta e il log spiega il comando da
/// eseguire. Una galleria non deve rifiutarsi di avviarsi per i tag.
async fn log_pgvector_status(db: &Db) {
    match keeppix_db::persist_pgvector_status(db).await {
        Ok(status) if status.ai_disabled() => {
            let message = status
                .message
                .as_deref()
                .unwrap_or("pgvector is not available");
            tracing::warn!(
                enable_command = status.enable_command.as_deref().unwrap_or(""),
                "{message}"
            );
        }
        Ok(status) => {
            tracing::info!(
                available = status.available,
                enabled = status.enabled,
                "pgvector probe"
            );
        }
        Err(e) => tracing::warn!(error = %e, "pgvector probe failed"),
    }
}

#[allow(clippy::too_many_lines)]
async fn spawn_maintenance(db: Db) {
    if let Err(e) = keeppix_jobs::cleanup_trash::schedule(&db).await {
        tracing::warn!(error = %e, "trash cleanup could not be scheduled");
    }
    if let Err(e) = keeppix_jobs::retry_derives::schedule(&db).await {
        tracing::warn!(error = %e, "error-asset retry could not be scheduled");
    }
    if let Err(e) = keeppix_jobs::regions::schedule_reap_stale(&db).await {
        tracing::warn!(error = %e, "stale-job reaper could not be scheduled");
    }
    if let Err(e) = keeppix_jobs::tmp_cleanup::schedule(&db).await {
        tracing::warn!(error = %e, "upload tmp cleanup could not be scheduled");
    }
    if let Err(e) = keeppix_jobs::embed::schedule_backfill(&db).await {
        tracing::warn!(error = %e, "AI embed backfill could not be scheduled");
    }
    if let Err(e) = keeppix_jobs::detect_faces::schedule_backfill(&db).await {
        tracing::warn!(error = %e, "face detection backfill could not be scheduled");
    }
    if let Err(e) = keeppix_jobs::maintenance::schedule_purge_sessions(&db).await {
        tracing::warn!(error = %e, "session purge could not be scheduled");
    }
    if let Err(e) = keeppix_jobs::maintenance::schedule_cleanup_done_jobs(&db).await {
        tracing::warn!(error = %e, "done-jobs cleanup could not be scheduled");
    }
    if let Err(e) = keeppix_jobs::maintenance::schedule_cleanup_idempotency(&db).await {
        tracing::warn!(error = %e, "idempotency cleanup could not be scheduled");
    }
    if let Err(e) = keeppix_jobs::maintenance::schedule_cleanup_transcode_cache(&db).await {
        tracing::warn!(error = %e, "transcode cache cleanup could not be scheduled");
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
                if let Err(e) = keeppix_jobs::maintenance::schedule_purge_sessions(&db).await {
                    tracing::warn!(error = %e, "session purge could not be scheduled");
                }
                if let Err(e) = keeppix_jobs::maintenance::schedule_cleanup_done_jobs(&db).await {
                    tracing::warn!(error = %e, "done-jobs cleanup could not be scheduled");
                }
                if let Err(e) = keeppix_jobs::maintenance::schedule_cleanup_idempotency(&db).await {
                    tracing::warn!(error = %e, "idempotency cleanup could not be scheduled");
                }
                if let Err(e) =
                    keeppix_jobs::maintenance::schedule_cleanup_transcode_cache(&db).await
                {
                    tracing::warn!(error = %e, "transcode cache cleanup could not be scheduled");
                }
            }
        });
    }
    {
        // Un'ora, non 24h come il cestino: un temporaneo scaduto occupa
        // spazio reale su disco (fino a `expected_size` per sessione), non
        // solo una riga di manutenzione da smaltire con calma.
        let db = db.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(60 * 60));
            interval.tick().await;
            loop {
                interval.tick().await;
                if let Err(e) = keeppix_jobs::tmp_cleanup::schedule(&db).await {
                    tracing::warn!(error = %e, "upload tmp cleanup could not be scheduled");
                }
            }
        });
    }
    {
        let db = db.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(5 * 60));
            interval.tick().await;
            loop {
                interval.tick().await;
                if let Err(e) = keeppix_jobs::regions::schedule_reap_stale(&db).await {
                    tracing::warn!(error = %e, "stale-job reaper could not be scheduled");
                }
            }
        });
    }
    {
        // Nightly window work: VACUUM, backup dump, integrity scrub, restore proof.
        // Re-check every hour; only enqueue when inside the default night window
        // so Interactive daytime load is never queued for these heavy jobs.
        let db = db.clone();
        tokio::spawn(async move {
            let night = keeppix_jobs::default_night_window();
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(60 * 60));
            interval.tick().await;
            loop {
                interval.tick().await;
                let now = chrono::Utc::now().time();
                let in_night = if night.0 <= night.1 {
                    now >= night.0 && now < night.1
                } else {
                    now >= night.0 || now < night.1
                };
                if !in_night {
                    continue;
                }
                if let Err(e) = keeppix_jobs::maintenance::schedule_vacuum_analyze(&db).await {
                    tracing::warn!(error = %e, "vacuum could not be scheduled");
                }
                if let Err(e) = keeppix_jobs::backup::schedule(&db).await {
                    tracing::warn!(error = %e, "backup dump could not be scheduled");
                }
                if let Err(e) = keeppix_jobs::maintenance::schedule_integrity_scrub(&db).await {
                    tracing::warn!(error = %e, "integrity scrub could not be scheduled");
                }
                if let Err(e) = keeppix_jobs::backup::schedule_restore_proof(&db).await {
                    tracing::warn!(error = %e, "restore proof could not be scheduled");
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
