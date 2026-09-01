#![allow(clippy::unwrap_used, clippy::expect_used)]

mod harness;

use chrono::Utc;
use harness::TestDb;
use keeppix_domain::{JobKind, JobPriority};
use keeppix_jobs::{ActivityTracker, IngestHandler, WorkerPool};

fn handler(test: &TestDb, tracker: std::sync::Arc<ActivityTracker>) -> IngestHandler {
    IngestHandler {
        db: test.db().clone(),
        data_dir: std::env::temp_dir().join(format!("kpx-ab-{}", uuid::Uuid::now_v7())),
        stability_wait: std::time::Duration::ZERO,
        trash_retention_days: keeppix_db::TRASH_RETENTION_DAYS,
        database_url: test.database_url().to_owned(),
        config_path: None,
        activity: tracker,
    }
}

/// The default worker still respects `EnergyProfile::Interactive`: with a
/// `Background`-priority job in the queue and a session that looks active,
/// it finds nothing to do — matches production behavior before this fix.
#[tokio::test]
async fn a_default_worker_leaves_background_work_alone_while_interactive() {
    let test = TestDb::start().await;
    keeppix_db::JobRepo::new(test.db())
        .enqueue(
            JobKind::PurgeSessions,
            serde_json::json!({}),
            JobPriority::Background,
            None,
        )
        .await
        .unwrap();

    let tracker = std::sync::Arc::new(ActivityTracker::new());
    tracker.notify_authenticated_request_at(Utc::now());
    let pool = WorkerPool::new(
        test.db().clone(),
        handler(&test, tracker.clone()),
        tracker,
        512 * 1024 * 1024,
        keeppix_jobs::default_night_window(),
        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
    );

    assert!(
        !pool.step().await.unwrap(),
        "a default worker must not claim Background work while the session looks Interactive"
    );
}

/// The whole point of the fix: an `always_background` worker keeps making
/// progress on the same queue even while the session looks Interactive —
/// bulk processing degrades to *slower*, not *stopped*, while someone is
/// actively using the app.
#[tokio::test]
async fn an_always_background_worker_keeps_working_while_interactive() {
    let test = TestDb::start().await;
    keeppix_db::JobRepo::new(test.db())
        .enqueue(
            JobKind::PurgeSessions,
            serde_json::json!({}),
            JobPriority::Background,
            None,
        )
        .await
        .unwrap();

    let tracker = std::sync::Arc::new(ActivityTracker::new());
    tracker.notify_authenticated_request_at(Utc::now());
    let pool = WorkerPool::new(
        test.db().clone(),
        handler(&test, tracker.clone()),
        tracker,
        512 * 1024 * 1024,
        keeppix_jobs::default_night_window(),
        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
    )
    .with_always_background(true);

    assert!(
        pool.step().await.unwrap(),
        "an always_background worker must still claim Background work while Interactive"
    );
}

/// `Paused` is a deliberate, explicit "stop everything" request — even an
/// `always_background` worker must honor it, unlike the passive
/// `Interactive` signal it's designed to see past.
#[tokio::test]
async fn an_always_background_worker_still_respects_an_explicit_pause() {
    let test = TestDb::start().await;
    keeppix_db::JobRepo::new(test.db())
        .enqueue(
            JobKind::PurgeSessions,
            serde_json::json!({}),
            JobPriority::Background,
            None,
        )
        .await
        .unwrap();

    let tracker = std::sync::Arc::new(ActivityTracker::new());
    let paused = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let pool = WorkerPool::new(
        test.db().clone(),
        handler(&test, tracker.clone()),
        tracker,
        512 * 1024 * 1024,
        keeppix_jobs::default_night_window(),
        paused,
    )
    .with_always_background(true);

    assert!(
        !pool.step().await.unwrap(),
        "an explicit pause must stop even an always_background worker"
    );
}
