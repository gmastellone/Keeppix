//! Discovery budget: the production configuration must not sleep five
//! seconds per file.

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
        .expect("library")
}

/// 1,000 files with a past `mtime`: none of them is arriving, so none
/// should cost a stability wait.
///
/// With the bug (5 s per file) this test would take ~83 minutes: it should
/// be observed failing by timeout, which is already the proof.
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
    // PRODUCTION CONFIGURATION, not Duration::ZERO.
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
        "1,000 settled files took {elapsed:?}: discovery is sleeping per \
         file instead of skipping files already settled"
    );

    let count = AssetRepo::new(test.db())
        .count_in_library(library.id)
        .await
        .unwrap();
    assert_eq!(count, 1000);
}

/// Assets must appear DURING the scan, not only at the end.
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
        "no asset was visible before the scan finished: \
         discovery is still buffering everything in RAM"
    );
}

/// A file still being written does not block the scan: it is deferred.
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
    // mtime = now: below SETTLED_AFTER → InFlight.

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
        "an InFlight file made discovery sleep: {elapsed:?}"
    );

    let count = AssetRepo::new(test.db())
        .count_in_library(library.id)
        .await
        .unwrap();
    assert_eq!(count, 1, "only the settled file should be indexed now");

    let deferred: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM jobs \
          WHERE kind = 'discover_library' AND status = 'pending' AND run_after > now()",
    )
    .fetch_one(test.db().pool())
    .await
    .unwrap();
    assert_eq!(
        deferred, 1,
        "a recheck must remain queued with run_after in the future"
    );
}
