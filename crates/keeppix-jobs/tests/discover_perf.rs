//! Budget di discovery: la configurazione di produzione non deve dormire
//! cinque secondi per file (Fase 2R / DIFETTO 1).

mod harness;

use std::time::{Duration, Instant};

use harness::TestDb;
use keeppix_db::{AssetRepo, LibraryRepo};
use keeppix_domain::{AuthContext, NewLibrary, SystemRole};
use keeppix_jobs::discover;

#[allow(clippy::expect_used)]
async fn seed_library(test: &TestDb, root: &std::path::Path) -> keeppix_domain::Library {
    let admin = harness::seed_admin(test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    LibraryRepo::new(test.db())
        .create(
            &ctx,
            NewLibrary {
                name: "Foto".to_owned(),
                owner_id: admin,
                root_path: root.to_path_buf(),
                exclude_patterns: vec![],
            },
        )
        .await
        .expect("libreria")
}

/// 1.000 file con `mtime` nel passato: nessuno sta arrivando, quindi nessuno
/// deve costare un'attesa di stabilità.
///
/// Con il difetto (5 s per file) questo test impiegherebbe ~83 minuti: va
/// osservato fallire per timeout, che è già la prova.
#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn discovering_a_thousand_settled_files_takes_seconds_not_hours() {
    let test = TestDb::start().await;
    let dir = tempfile::tempdir().unwrap();

    for i in 0..1_000 {
        let p = dir.path().join(format!("IMG_{i:04}.jpg"));
        std::fs::write(&p, b"\xFF\xD8\xFF\xE0\x00\x10JFIF\x00").unwrap();
        let old = filetime::FileTime::from_unix_time(chrono::Utc::now().timestamp() - 7200, 0);
        filetime::set_file_mtime(&p, old).unwrap();
    }

    let library = seed_library(&test, dir.path()).await;

    let start = Instant::now();
    // CONFIGURAZIONE DI PRODUZIONE, non Duration::ZERO.
    keeppix_jobs::discover::run(
        test.db(),
        library.id,
        keeppix_jobs::PRODUCTION_SETTLED_AFTER,
    )
    .await
    .unwrap();
    let elapsed = start.elapsed();
    eprintln!("MEASUREMENT discovery 1000 settled files: {elapsed:?}");

    assert!(
        elapsed < Duration::from_secs(30),
        "1.000 file assestati hanno richiesto {elapsed:?}: la discovery sta \
         dormendo per file invece di saltare i file già fermi"
    );

    let count = AssetRepo::new(test.db())
        .count_in_library(library.id)
        .await
        .unwrap();
    assert_eq!(count, 1000);
}

/// Gli asset devono comparire DURANTE la scansione, non solo alla fine.
#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn assets_appear_while_the_scan_is_still_running() {
    let test = TestDb::start().await;
    let dir = tempfile::tempdir().unwrap();

    for i in 0..2_000 {
        let p = dir.path().join(format!("IMG_{i:04}.jpg"));
        std::fs::write(&p, b"\xFF\xD8\xFF\xE0\x00\x10JFIF\x00").unwrap();
        let old = filetime::FileTime::from_unix_time(chrono::Utc::now().timestamp() - 7200, 0);
        filetime::set_file_mtime(&p, old).unwrap();
    }
    let library = seed_library(&test, dir.path()).await;
    let library_id = library.id;
    let db = test.db().clone();

    let scan = tokio::spawn(async move {
        discover::run(&db, library_id, keeppix_jobs::PRODUCTION_SETTLED_AFTER).await
    });

    let mut saw_progress = false;
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        if scan.is_finished() {
            break;
        }
        let n = AssetRepo::new(test.db())
            .count_in_library(library_id)
            .await
            .unwrap();
        if n > 0 {
            saw_progress = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    let result = scan.await.expect("join");
    result.expect("discover");
    assert!(
        saw_progress,
        "nessun asset era visibile prima della fine dello scan: \
         la discovery accumula ancora tutto in RAM"
    );
}

/// Un file che sta ancora arrivando non blocca la scansione: viene rimandato.
#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn a_file_still_being_written_is_deferred_not_waited_for() {
    let test = TestDb::start().await;
    let dir = tempfile::tempdir().unwrap();

    let settled = dir.path().join("old.jpg");
    std::fs::write(&settled, b"\xFF\xD8\xFF\xE0\x00\x10JFIF\x00").unwrap();
    let old = filetime::FileTime::from_unix_time(chrono::Utc::now().timestamp() - 7200, 0);
    filetime::set_file_mtime(&settled, old).unwrap();

    let inflight = dir.path().join("new.jpg");
    std::fs::write(&inflight, b"\xFF\xD8\xFF\xE0\x00\x10JFIF\x00").unwrap();
    // mtime = ora: sotto SETTLED_AFTER → InFlight.

    let library = seed_library(&test, dir.path()).await;

    let start = Instant::now();
    discover::run(
        test.db(),
        library.id,
        keeppix_jobs::PRODUCTION_SETTLED_AFTER,
    )
    .await
    .unwrap();
    let elapsed = start.elapsed();

    assert!(
        elapsed < Duration::from_secs(2),
        "un file InFlight ha fatto dormire la discovery: {elapsed:?}"
    );

    let count = AssetRepo::new(test.db())
        .count_in_library(library.id)
        .await
        .unwrap();
    assert_eq!(
        count, 1,
        "solo il file assestato deve essere indicizzato ora"
    );

    let deferred: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM jobs \
          WHERE kind = 'discover_library' AND status = 'pending' AND run_after > now()",
    )
    .fetch_one(test.db().pool())
    .await
    .unwrap();
    assert_eq!(
        deferred, 1,
        "deve restare un ricontrollo in coda con run_after nel futuro"
    );
}
