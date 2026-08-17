//! Verifica che la configurazione di produzione sia unica e non `ZERO`.
//! Se `main.rs` e i test divergono, un difetto può vivere solo nel binario
//! spedito (Fase 2R / DIFETTO 1).

mod harness;

use std::path::PathBuf;
use std::time::Duration;

use harness::TestDb;
use keeppix_db::{AssetRepo, JobRepo, LibraryRepo};
use keeppix_domain::{AuthContext, JobKind, JobPriority, NewLibrary, SystemRole};
use keeppix_jobs::{
    IngestHandler, JobHandler, PRODUCTION_BATCH_SIZE, PRODUCTION_SETTLED_AFTER,
    PRODUCTION_STABILITY_WAIT,
};

/// Stesso wiring di `keeppix-server/src/main.rs`.
fn production_ingest_handler(db: keeppix_db::Db, data_dir: PathBuf) -> IngestHandler {
    IngestHandler {
        db,
        data_dir,
        stability_wait: PRODUCTION_SETTLED_AFTER,
        trash_retention_days: keeppix_db::TRASH_RETENTION_DAYS,
    }
}

#[test]
fn production_constants_are_non_zero_and_match_baseline() {
    assert_ne!(
        PRODUCTION_STABILITY_WAIT,
        Duration::ZERO,
        "la ricontrollo InFlight non deve essere disabilitato in produzione"
    );
    assert_eq!(PRODUCTION_STABILITY_WAIT, Duration::from_secs(5));

    assert_ne!(
        PRODUCTION_SETTLED_AFTER,
        Duration::ZERO,
        "la soglia Settled non deve essere ZERO — altrimenti ogni file sembra in arrivo"
    );
    assert_eq!(PRODUCTION_SETTLED_AFTER, Duration::from_secs(60));

    assert_ne!(PRODUCTION_BATCH_SIZE, 0);
    assert_eq!(PRODUCTION_BATCH_SIZE, 500);
}

#[test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
fn default_webp_quality_is_eighty_two() {
    assert_eq!(
        keeppix_jobs::DEFAULT_WEBP_QUALITY,
        82,
        "sotto 75 si vede; sopra 88 si paga per poco"
    );
    let deploy =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/DEPLOY.md"))
            .expect("DEPLOY.md");
    assert!(
        deploy.contains("KEEPPIX_WEBP_QUALITY"),
        "il default deve essere documentato per l'operatore"
    );
    assert!(
        deploy.contains("| `82` | Qualità WebP"),
        "DEPLOY.md deve citare il default 82"
    );
}

#[test]
fn default_watch_poll_is_fifteen_minutes() {
    assert_eq!(
        keeppix_jobs::watch::DEFAULT_POLL,
        Duration::from_secs(15 * 60),
        "polling troppo frequente riaccoderebbe discover in loop su un Pi"
    );
}

#[test]
fn native_min_rescan_is_thirty_seconds() {
    assert_eq!(
        keeppix_jobs::watch::MIN_RESCAN,
        Duration::from_secs(30),
        "senza cadenza minima un bind mount rumoroso riaccoda discover in loop"
    );
}

#[test]
fn production_ingest_handler_uses_settled_after_not_zero() {
    // `keeppix-server/src/main.rs` passa `PRODUCTION_SETTLED_AFTER` al campo
    // `stability_wait` dell'`IngestHandler` — non `ZERO`, non
    // `PRODUCTION_STABILITY_WAIT` (quello è solo per `run_after` su InFlight).
    assert_eq!(PRODUCTION_SETTLED_AFTER, Duration::from_secs(60));
    assert_ne!(PRODUCTION_SETTLED_AFTER, Duration::ZERO);
    assert_ne!(PRODUCTION_STABILITY_WAIT, PRODUCTION_SETTLED_AFTER);
}

/// Il dispatcher di produzione deve indicizzare file assestati senza dormire
/// per file — stesso budget di `discover_perf.rs`.
#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn production_handler_discovers_settled_files_within_budget() {
    let test = TestDb::start().await;
    let dir = tempfile::tempdir().unwrap();

    for i in 0..200 {
        let p = dir.path().join(format!("IMG_{i:04}.jpg"));
        std::fs::write(&p, b"\xFF\xD8\xFF\xE0\x00\x10JFIF\x00").unwrap();
        let old = filetime::FileTime::from_unix_time(chrono::Utc::now().timestamp() - 7200, 0);
        filetime::set_file_mtime(&p, old).unwrap();
    }

    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let library = LibraryRepo::new(test.db())
        .create(
            &ctx,
            NewLibrary {
                name: "Prod cfg".to_owned(),
                owner_id: admin,
                root_path: dir.path().to_path_buf(),
                exclude_patterns: vec![],
            },
        )
        .await
        .expect("libreria");

    JobRepo::new(test.db())
        .enqueue(
            JobKind::DiscoverLibrary,
            serde_json::json!({ "library_id": library.id.to_string() }),
            JobPriority::Background,
            Some(&format!("discover:{}", library.id)),
        )
        .await
        .expect("enqueue");

    let handler = production_ingest_handler(test.db().clone(), dir.path().to_path_buf());
    let job = JobRepo::new(test.db())
        .claim(uuid::Uuid::now_v7(), JobPriority::Background)
        .await
        .expect("claim")
        .expect("job in coda");

    let start = std::time::Instant::now();
    handler
        .handle(&job)
        .await
        .expect("discover via handler produzione");
    let elapsed = start.elapsed();
    eprintln!("MEASUREMENT production handler discover 200 settled: {elapsed:?}");

    // 30 s come `discover_perf.rs`, che il commento sopra dichiarava già di
    // seguire mentre asseriva 5 s.
    //
    // La soglia si deriva dal difetto che il test protegge, non da quanto è
    // veloce la macchina di turno: il difetto è `restat_if_stable` che dormiva
    // 5 s **per file** (D1 della Fase 2R), quindi su 200 file costerebbe ~1000
    // secondi. Qualsiasi soglia molto sotto quel valore lo cattura; 30 s lascia
    // 33× di margine sul difetto e smette di misurare l'umore del runner.
    //
    // A 5 s non lo faceva: in CI questo test ha misurato 5,95 s
    // (run 32026578024) per contesa sul Postgres condiviso — su 200 file il
    // costo fisso di seed, creazione libreria e round-trip domina, e non ha
    // niente a che vedere con un sonno per file. Un budget che passa o fallisce
    // secondo il carico del runner non prova ciò che il suo nome afferma.
    assert!(
        elapsed < Duration::from_secs(30),
        "handler con PRODUCTION_SETTLED_AFTER ha impiegato {elapsed:?} su 200 file assestati: \
         la discovery sta dormendo per file invece di saltare i file già fermi"
    );

    let count = AssetRepo::new(test.db())
        .count_in_library(library.id)
        .await
        .unwrap();
    assert_eq!(count, 200);
}
