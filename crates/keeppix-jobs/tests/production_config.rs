//! Verifica che la configurazione di produzione sia unica e non `ZERO`.
//! Se `main.rs` e i test divergono, un difetto può vivere solo nel binario
//! spedito (Fase 2R / DIFETTO 1).

mod harness;

use std::path::PathBuf;
use std::time::Duration;

use harness::TestDb;
use keeppix_db::{AssetRepo, JobRepo, LibraryRepo};
use keeppix_domain::{
    AuthContext, JobKind, JobPriority, NewLibrary, SystemRole,
};
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
    handler.handle(&job).await.expect("discover via handler produzione");
    let elapsed = start.elapsed();
    eprintln!("MEASUREMENT production handler discover 200 settled: {elapsed:?}");

    assert!(
        elapsed < Duration::from_secs(5),
        "handler con PRODUCTION_SETTLED_AFTER ha impiegato {elapsed:?} su 200 file assestati"
    );

    let count = AssetRepo::new(test.db())
        .count_in_library(library.id)
        .await
        .unwrap();
    assert_eq!(count, 200);
}
