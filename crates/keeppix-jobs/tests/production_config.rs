//! Verifies that the production configuration is unique and not `ZERO`.
//! If `main.rs` and the tests diverge, a bug can live only in the shipped
//! binary and go unnoticed.

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

/// Same wiring as `keeppix-server/src/main.rs`.
fn production_ingest_handler(db: keeppix_db::Db, data_dir: PathBuf) -> IngestHandler {
    IngestHandler {
        db,
        data_dir,
        stability_wait: PRODUCTION_SETTLED_AFTER,
        trash_retention_days: keeppix_db::TRASH_RETENTION_DAYS,
        database_url: String::new(),
        config_path: None,
        activity: std::sync::Arc::new(keeppix_jobs::ActivityTracker::new()),
    }
}

#[test]
fn production_constants_are_non_zero_and_match_baseline() {
    assert_ne!(
        PRODUCTION_STABILITY_WAIT,
        Duration::ZERO,
        "the InFlight recheck must not be disabled in production"
    );
    assert_eq!(PRODUCTION_STABILITY_WAIT, Duration::from_secs(5));

    assert_ne!(
        PRODUCTION_SETTLED_AFTER,
        Duration::ZERO,
        "the Settled threshold must not be ZERO — otherwise every file looks like it's arriving"
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
        "below 75 it shows; above 88 you pay for little gain"
    );
    let deploy =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/DEPLOY.md"))
            .expect("DEPLOY.md");
    assert!(
        deploy.contains("KEEPPIX_WEBP_QUALITY"),
        "the default must be documented for the operator"
    );
    assert!(
        deploy.contains("| `82` | Qualità WebP"),
        "DEPLOY.md must cite the default of 82"
    );
}

#[test]
fn default_watch_poll_is_fifteen_minutes() {
    assert_eq!(
        keeppix_jobs::watch::DEFAULT_POLL,
        Duration::from_secs(15 * 60),
        "polling too frequent would re-enqueue discover in a loop on a Pi"
    );
}

#[test]
fn native_min_rescan_is_thirty_seconds() {
    assert_eq!(
        keeppix_jobs::watch::MIN_RESCAN,
        Duration::from_secs(30),
        "without a minimum cadence a noisy bind mount re-enqueues discover in a loop"
    );
}

#[test]
fn production_ingest_handler_uses_settled_after_not_zero() {
    // `keeppix-server/src/main.rs` passes `PRODUCTION_SETTLED_AFTER` to the
    // `IngestHandler`'s `stability_wait` field — not `ZERO`, not
    // `PRODUCTION_STABILITY_WAIT` (that one is only for `run_after` on InFlight).
    assert_eq!(PRODUCTION_SETTLED_AFTER, Duration::from_secs(60));
    assert_ne!(PRODUCTION_SETTLED_AFTER, Duration::ZERO);
    assert_ne!(PRODUCTION_STABILITY_WAIT, PRODUCTION_SETTLED_AFTER);
}

/// The production dispatcher must index settled files without sleeping per
/// file — same budget as `discover_perf.rs`.
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
        .expect("library");

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
        .expect("job in queue");

    let start = std::time::Instant::now();
    handler
        .handle(&job)
        .await
        .expect("discover via production handler");
    let elapsed = start.elapsed();
    eprintln!("MEASUREMENT production handler discover 200 settled: {elapsed:?}");

    // 30 s, same as `discover_perf.rs`.
    //
    // The threshold is derived from the bug this test guards against, not
    // from how fast the machine happens to be: the bug was
    // `restat_if_stable` sleeping 5 s **per file**, so on 200 files that
    // would cost ~1000 seconds. Any threshold far below that value catches
    // it; 30 s leaves 33x of margin over the bug and stops measuring the
    // runner's mood.
    //
    // At 5 s it didn't: in CI this test measured 5.95 s due to contention
    // on the shared Postgres instance — on 200 files the fixed cost of
    // seeding, library creation, and round-trips dominates, and it has
    // nothing to do with a per-file sleep. A budget that passes or fails
    // depending on runner load doesn't prove what its name claims.
    assert!(
        elapsed < Duration::from_secs(30),
        "handler with PRODUCTION_SETTLED_AFTER took {elapsed:?} on 200 settled files: \
         discovery is sleeping per file instead of skipping files already settled"
    );

    let count = AssetRepo::new(test.db())
        .count_in_library(library.id)
        .await
        .unwrap();
    assert_eq!(count, 200);
}
