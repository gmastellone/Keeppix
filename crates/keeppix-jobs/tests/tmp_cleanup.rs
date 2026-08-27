#![allow(clippy::unwrap_used, clippy::expect_used)]

mod harness;

use std::fs;
use std::path::{Path, PathBuf};

use harness::TestDb;
use keeppix_db::{FolderRepo, JobRepo, LibraryRepo, NewUploadSession, UploadSessionRepo};
use keeppix_domain::{AuthContext, JobKind, JobPriority, NewLibrary, SystemRole, UserId};

#[allow(clippy::expect_used, clippy::unwrap_used)]
fn temp_library_root() -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "keeppix-jobs-tmp-cleanup-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("root dir");
    root
}

#[allow(clippy::expect_used, clippy::unwrap_used)]
async fn seed_library(test: &TestDb, owner: UserId, root: &Path) -> keeppix_domain::LibraryId {
    LibraryRepo::new(test.db())
        .create(
            &AuthContext::user(owner, SystemRole::Admin),
            NewLibrary {
                name: "Foto".to_owned(),
                owner_id: owner,
                root_path: root.to_path_buf(),
                exclude_patterns: vec![],
            },
        )
        .await
        .expect("library")
        .id
}

/// `keeppix_jobs::tmp_cleanup::run` must clean up an expired session
/// without touching a live one — this isn't a duplicate of the
/// `keeppix-db`-level test, it verifies the job is wired correctly to the
/// repo.
#[tokio::test]
async fn run_removes_only_expired_upload_sessions() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let root = temp_library_root();
    let library = seed_library(&test, admin, &root).await;
    let folder = FolderRepo::new(test.db())
        .ensure_path(library, &["2024"])
        .await
        .unwrap();
    fs::create_dir_all(root.join("2024")).unwrap();
    let ctx = AuthContext::user(admin, SystemRole::Admin);

    let repo = UploadSessionRepo::new(test.db());
    let expired = repo
        .create(
            &ctx,
            NewUploadSession {
                target_folder_id: folder.id,
                filename: "scaduto.jpg".to_owned(),
                expected_size: 3,
                expected_hash: None,
                client_mtime: None,
            },
        )
        .await
        .unwrap();
    fs::write(&expired.temp_path, b"abc").unwrap();
    sqlx::query(
        "UPDATE upload_sessions SET expires_at = now() - interval '1 second' WHERE id = $1",
    )
    .bind(expired.id.as_uuid())
    .execute(test.db().pool())
    .await
    .unwrap();

    let live = repo
        .create(
            &ctx,
            NewUploadSession {
                target_folder_id: folder.id,
                filename: "vivo.jpg".to_owned(),
                expected_size: 3,
                expected_hash: None,
                client_mtime: None,
            },
        )
        .await
        .unwrap();
    fs::write(&live.temp_path, b"abc").unwrap();

    keeppix_jobs::tmp_cleanup::run(test.db()).await.unwrap();

    assert!(!expired.temp_path.exists());
    let expired_rows: i64 =
        sqlx::query_scalar("SELECT count(*) FROM upload_sessions WHERE id = $1")
            .bind(expired.id.as_uuid())
            .fetch_one(test.db().pool())
            .await
            .unwrap();
    assert_eq!(expired_rows, 0);

    assert!(live.temp_path.exists());
    let live_rows: i64 = sqlx::query_scalar("SELECT count(*) FROM upload_sessions WHERE id = $1")
        .bind(live.id.as_uuid())
        .fetch_one(test.db().pool())
        .await
        .unwrap();
    assert_eq!(live_rows, 1);

    let _ = fs::remove_dir_all(&root);
}

/// `schedule` enqueues a deduplicated run — a second call while the first
/// is still pending does not create a second one.
#[tokio::test]
async fn schedule_enqueues_a_deduplicated_background_job() {
    let test = TestDb::start().await;

    keeppix_jobs::tmp_cleanup::schedule(test.db())
        .await
        .unwrap();
    keeppix_jobs::tmp_cleanup::schedule(test.db())
        .await
        .unwrap();

    let jobs: Vec<(String, i16)> =
        sqlx::query_as("SELECT kind, priority FROM jobs WHERE dedup_key = 'tmp_cleanup'")
            .fetch_all(test.db().pool())
            .await
            .unwrap();
    assert_eq!(jobs.len(), 1, "two schedules must not create two jobs");
    assert_eq!(jobs[0].0, JobKind::TmpCleanup.as_str());
    assert_eq!(jobs[0].1, JobPriority::Background.as_i16());

    let repo = JobRepo::new(test.db());
    assert!(
        repo.count_for_dedup_key("tmp_cleanup").await.unwrap() >= 1,
        "the repo must see the row that was just enqueued"
    );
}
