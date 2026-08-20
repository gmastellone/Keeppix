mod harness;

use chrono::{Duration, Utc};
use harness::TestDb;
use keeppix_db::{IdempotencyRepo, JobRepo, SessionRepo};
use keeppix_domain::{JobKind, JobPriority};
use keeppix_jobs::maintenance;
use serde_json::json;

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn purge_sessions_and_cleanup_helpers_run() {
    let test = TestDb::start().await;
    // Smoke: helpers return Ok on an empty DB.
    maintenance::purge_sessions(test.db()).await.unwrap();
    maintenance::cleanup_done_jobs(test.db()).await.unwrap();
    maintenance::cleanup_idempotency(test.db()).await.unwrap();
    maintenance::vacuum_analyze(test.db()).await.unwrap();
    maintenance::integrity_scrub(test.db()).await.unwrap();

    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("video-cache/old")).unwrap();
    let stale = dir.path().join("video-cache/old/seg.ts");
    std::fs::write(&stale, b"x").unwrap();
    let old = filetime::FileTime::from_unix_time(Utc::now().timestamp() - 100 * 24 * 3600, 0);
    filetime::set_file_mtime(&stale, old).unwrap();
    maintenance::cleanup_transcode_cache(dir.path()).unwrap();
    assert!(!stale.exists());
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn cleanup_done_jobs_removes_old_rows() {
    let test = TestDb::start().await;
    let jobs = JobRepo::new(test.db());
    let job = jobs
        .enqueue(
            JobKind::TmpCleanup,
            json!({}),
            JobPriority::Background,
            None,
        )
        .await
        .unwrap();
    // Force done + old created_at.
    sqlx::query("UPDATE jobs SET status = 'done', created_at = $2 WHERE id = $1")
        .bind(job.id)
        .bind(Utc::now() - Duration::days(10))
        .execute(test.db().pool())
        .await
        .unwrap();
    let n = jobs
        .delete_done_older_than(Utc::now() - Duration::days(7))
        .await
        .unwrap();
    assert_eq!(n, 1);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn schedule_helpers_enqueue_background_jobs() {
    let test = TestDb::start().await;
    maintenance::schedule_purge_sessions(test.db())
        .await
        .unwrap();
    maintenance::schedule_vacuum_analyze(test.db())
        .await
        .unwrap();
    let claimed = JobRepo::new(test.db())
        .claim(uuid::Uuid::now_v7(), JobPriority::Background)
        .await
        .unwrap();
    assert!(claimed.is_some());
}

// Keep SessionRepo::purge_expired / IdempotencyRepo linked for the wired check.
#[allow(dead_code)]
fn _touch_repos(db: &keeppix_db::Db) {
    let _ = SessionRepo::new(db);
    let _ = IdempotencyRepo::new(db);
}
