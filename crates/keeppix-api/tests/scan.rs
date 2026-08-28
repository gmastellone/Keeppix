mod harness;

use std::fs;
use std::time::Duration;

use harness::TestServer;
use keeppix_domain::{AuthContext, SystemRole};
use keeppix_jobs::{ActivityTracker, IngestHandler, WorkerPool};
use serde_json::json;

#[allow(clippy::expect_used)]
async fn setup_admin(server: &TestServer) {
    let response = server
        .client
        .post(server.url("/api/v1/setup"))
        .json(&json!({
            "username": "giovanni",
            "display_name": "Giovanni",
            "password": "correct horse battery staple"
        }))
        .send()
        .await
        .expect("setup");
    assert_eq!(response.status(), 201);
}

#[allow(clippy::expect_used)]
fn library_dir(server: &TestServer, name: &str) -> std::path::PathBuf {
    let path = server.photos_root.join(name);
    fs::create_dir_all(&path).expect("library dir");
    path
}

#[allow(clippy::expect_used)]
async fn drain_workers(server: &TestServer, data_dir: &std::path::Path) {
    let tracker = std::sync::Arc::new(ActivityTracker::new());
    let handler = IngestHandler {
        db: server.db.clone(),
        data_dir: data_dir.to_path_buf(),
        stability_wait: Duration::ZERO,
        trash_retention_days: keeppix_db::TRASH_RETENTION_DAYS,
        database_url: server.database_url.clone(),
        config_path: None,
        activity: tracker.clone(),
    };
    let pool = WorkerPool::new(
        server.db.clone(),
        handler,
        tracker,
        512 * 1024 * 1024,
        keeppix_jobs::default_night_window(),
        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
    );
    let start = std::time::Instant::now();
    loop {
        assert!(
            start.elapsed() < Duration::from_secs(60),
            "workers timed out"
        );
        if !pool.step().await.expect("step") {
            let pending: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM jobs \
                 WHERE status = 'running' \
                    OR (status = 'pending' AND run_after <= now())",
            )
            .fetch_one(server.db.pool())
            .await
            .expect("count");
            if pending == 0 {
                break;
            }
        }
    }
}

/// The request that actually enqueues the job must be able to follow it —
/// without an `operation_id` there's nothing to cancel or show on the
/// WebSocket.
#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn starting_a_scan_returns_an_operation_id_that_reaches_done() {
    let server = TestServer::start().await;
    setup_admin(&server).await;
    let root = library_dir(&server, "scan-op-id");
    fs::write(root.join("a.jpg"), b"x").unwrap();

    let created = server
        .client
        .post(server.url("/api/v1/libraries"))
        .json(&json!({
            "name": "ScanOp",
            "root_path": root.to_string_lossy(),
        }))
        .send()
        .await
        .unwrap();
    let id = created.json::<serde_json::Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let scan = server
        .client
        .post(server.url(&format!("/api/v1/libraries/{id}/scan")))
        .send()
        .await
        .unwrap();
    assert_eq!(scan.status(), 202);
    let body: serde_json::Value = scan.json().await.unwrap();
    let operation_id: keeppix_domain::OperationId =
        body["operation_id"].as_str().unwrap().parse().unwrap();

    drain_workers(&server, &server.data_dir.join("op-id-data")).await;

    let admin = admin_id(&server).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let ops = keeppix_db::OperationsRepo::new(&server.db);
    let op = ops.find(&ctx, operation_id).await.unwrap();
    assert_eq!(op.status, keeppix_domain::OperationStatus::Done);
    assert_eq!(op.done, 1);
    assert_eq!(op.succeeded.len(), 1);
}

/// A second `POST /scan` while the first job is still queued must not
/// create an orphaned operation — one with no job that would ever advance
/// it, staying `running` forever.
#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn a_scan_request_while_one_is_already_queued_reports_no_operation() {
    let server = TestServer::start().await;
    setup_admin(&server).await;
    let root = library_dir(&server, "scan-op-dedup");
    fs::write(root.join("a.jpg"), b"x").unwrap();

    let created = server
        .client
        .post(server.url("/api/v1/libraries"))
        .json(&json!({
            "name": "ScanOpDedup",
            "root_path": root.to_string_lossy(),
        }))
        .send()
        .await
        .unwrap();
    let id = created.json::<serde_json::Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let first = server
        .client
        .post(server.url(&format!("/api/v1/libraries/{id}/scan")))
        .send()
        .await
        .unwrap();
    assert_eq!(first.status(), 202);
    let first_body: serde_json::Value = first.json().await.unwrap();
    assert!(first_body["operation_id"].is_string());

    let second = server
        .client
        .post(server.url(&format!("/api/v1/libraries/{id}/scan")))
        .send()
        .await
        .unwrap();
    assert_eq!(second.status(), 202);
    let second_body: serde_json::Value = second.json().await.unwrap();
    assert!(
        second_body["operation_id"].is_null(),
        "the first scan's job wins the dedup: the second request has nothing to follow"
    );
}

use harness::spawn_worker_pool;

/// Cancelling midway (after seeing some successes) leaves exactly what
/// was applied, like `BulkOutcome` — not a rollback. Same property proven
/// at the `keeppix-jobs` level (`discover_operations.rs`), here actually
/// going through the HTTP route.
#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn cancelling_a_scan_via_the_api_leaves_a_partial_bulk_outcome() {
    // Discover writes in multi-row batches of `PRODUCTION_BATCH_SIZE`
    // files — with fewer files than a full batch, the entire scan writes
    // in a single statement, with no "midway" window to cancel.
    const TOTAL: usize = 5 * keeppix_jobs::PRODUCTION_BATCH_SIZE;
    let server = TestServer::start().await;
    setup_admin(&server).await;
    let root = library_dir(&server, "scan-cancel");
    for n in 0..TOTAL {
        fs::write(root.join(format!("{n:03}.jpg")), b"x").unwrap();
    }

    let created = server
        .client
        .post(server.url("/api/v1/libraries"))
        .json(&json!({
            "name": "ScanCancel",
            "root_path": root.to_string_lossy(),
        }))
        .send()
        .await
        .unwrap();
    let id = created.json::<serde_json::Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let scan = server
        .client
        .post(server.url(&format!("/api/v1/libraries/{id}/scan")))
        .send()
        .await
        .unwrap();
    assert_eq!(scan.status(), 202);
    let operation_id = scan.json::<serde_json::Value>().await.unwrap()["operation_id"]
        .as_str()
        .unwrap()
        .to_owned();

    let worker = spawn_worker_pool(
        server.db.clone(),
        server.database_url.clone(),
        server.data_dir.join("cancel-data"),
    );

    let admin = admin_id(&server).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let ops = keeppix_db::OperationsRepo::new(&server.db);
    let op_id: keeppix_domain::OperationId = operation_id.parse().unwrap();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    loop {
        assert!(
            tokio::time::Instant::now() < deadline,
            "the scan hasn't made enough progress to be cancellable"
        );
        if ops.find(&ctx, op_id).await.unwrap().done >= 3 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(2)).await;
    }

    let cancel = server
        .client
        .post(server.url(&format!("/api/v1/operations/{operation_id}/cancel")))
        .send()
        .await
        .unwrap();
    assert_eq!(cancel.status(), 200);
    let outcome: serde_json::Value = cancel.json().await.unwrap();
    let succeeded = outcome["succeeded"].as_array().unwrap();
    assert!(
        !succeeded.is_empty(),
        "the response must list at least what was already applied at the moment of cancellation"
    );
    assert!(outcome["failed"].as_array().unwrap().is_empty());

    tokio::time::timeout(Duration::from_secs(20), worker)
        .await
        .expect("the worker did not stop after cancellation")
        .unwrap();

    let finished = ops.find(&ctx, op_id).await.unwrap();
    assert_eq!(finished.status, keeppix_domain::OperationStatus::Cancelled);
    assert!(
        finished.done < i64::try_from(TOTAL).unwrap(),
        "a cancellation that still completes every file proved nothing: done={}",
        finished.done
    );
    assert!(finished.done > 0);

    let asset_count: i64 = sqlx::query_scalar("SELECT count(*) FROM assets")
        .fetch_one(server.db.pool())
        .await
        .unwrap();
    assert_eq!(
        asset_count, finished.done,
        "the written assets must match the final partial outcome, not the whole folder"
    );
}

/// Like every other probeable id in the system: someone who is neither
/// owner nor admin gets `Forbidden`, not `NotFound` — otherwise the
/// endpoint becomes an existence oracle over in-flight `operation_id`s.
#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn cancelling_someone_elses_operation_is_forbidden() {
    let server = TestServer::start().await;
    setup_admin(&server).await;
    let admin = admin_id(&server).await;

    keeppix_db::UserRepo::new(&server.db)
        .create(
            &AuthContext::user(admin, SystemRole::Admin),
            keeppix_domain::NewUser {
                username: keeppix_domain::Username::parse("mario").unwrap(),
                email: None,
                display_name: "Mario".to_owned(),
                password_hash: keeppix_domain::hash_password(
                    &keeppix_domain::Password::parse("mario-password-ok").unwrap(),
                )
                .unwrap()
                .as_str()
                .to_owned(),
                role: SystemRole::User,
            },
        )
        .await
        .unwrap();

    let ops = keeppix_db::OperationsRepo::new(&server.db);
    let admin_ctx = AuthContext::user(admin, SystemRole::Admin);
    let operation = ops
        .create(&admin_ctx, keeppix_domain::OperationKind::LibraryScan)
        .await
        .unwrap();

    let mario = reqwest::Client::builder()
        .cookie_store(true)
        .default_headers(harness::client_headers())
        .build()
        .unwrap();
    let login = mario
        .post(server.url("/api/v1/auth/login"))
        .json(&json!({ "username": "mario", "password": "mario-password-ok" }))
        .send()
        .await
        .unwrap();
    assert_eq!(login.status(), 200);

    let response = mario
        .post(server.url(&format!("/api/v1/operations/{}/cancel", operation.id)))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 403);
    let problem: serde_json::Value = response.json().await.unwrap();
    assert_eq!(problem["type"], "keeppix/forbidden");
}

#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn admin_id(server: &TestServer) -> keeppix_domain::UserId {
    server
        .client
        .get(server.url("/api/v1/auth/me"))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap()["user"]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap()
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn starting_a_scan_creates_assets() {
    let server = TestServer::start().await;
    setup_admin(&server).await;
    let root = library_dir(&server, "scan-one");
    let tiny = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../keeppix-jobs/tests/fixtures/tiny.jpg");
    fs::copy(&tiny, root.join("a.jpg")).unwrap();

    let created = server
        .client
        .post(server.url("/api/v1/libraries"))
        .json(&json!({
            "name": "Scan",
            "root_path": root.to_string_lossy(),
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(created.status(), 201);
    let id = created.json::<serde_json::Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let scan = server
        .client
        .post(server.url(&format!("/api/v1/libraries/{id}/scan")))
        .send()
        .await
        .unwrap();
    assert_eq!(scan.status(), 202);

    let data_dir = server.data_dir.join("derivatives-home");
    drain_workers(&server, &data_dir).await;

    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM assets")
        .fetch_one(server.db.pool())
        .await
        .unwrap();
    assert_eq!(n, 1);

    let status = server
        .client
        .get(server.url(&format!("/api/v1/libraries/{id}/scan")))
        .send()
        .await
        .unwrap();
    assert_eq!(status.status(), 200);
    let body: serde_json::Value = status.json().await.unwrap();
    assert_eq!(body["asset_count"], 1);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn starting_a_scan_twice_does_not_double_the_work() {
    let server = TestServer::start().await;
    setup_admin(&server).await;
    let root = library_dir(&server, "scan-dedup");
    fs::write(root.join("a.jpg"), b"x").unwrap();

    let created = server
        .client
        .post(server.url("/api/v1/libraries"))
        .json(&json!({
            "name": "Dedup",
            "root_path": root.to_string_lossy(),
        }))
        .send()
        .await
        .unwrap();
    let id = created.json::<serde_json::Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let first = server
        .client
        .post(server.url(&format!("/api/v1/libraries/{id}/scan")))
        .send()
        .await
        .unwrap();
    assert_eq!(first.status(), 202);
    let second = server
        .client
        .post(server.url(&format!("/api/v1/libraries/{id}/scan")))
        .send()
        .await
        .unwrap();
    assert_eq!(second.status(), 202);

    let n: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM jobs WHERE kind = 'discover_library' \
         AND status IN ('pending','running')",
    )
    .fetch_one(server.db.pool())
    .await
    .unwrap();
    assert_eq!(n, 1, "dedup_key must collapse the two requests");
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn a_library_created_after_boot_is_watched() {
    let server = TestServer::start().await;
    setup_admin(&server).await;
    let root = library_dir(&server, "watched-live");

    let created = server
        .client
        .post(server.url("/api/v1/libraries"))
        .json(&json!({
            "name": "Live",
            "root_path": root.to_string_lossy(),
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(created.status(), 201);
    let id = created.json::<serde_json::Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let library_id: keeppix_domain::LibraryId = id.parse().unwrap();

    // Let the watcher started at creation time spin up.
    tokio::time::sleep(Duration::from_millis(400)).await;
    sqlx::query("UPDATE jobs SET status = 'done' WHERE kind = 'discover_library'")
        .execute(server.db.pool())
        .await
        .unwrap();

    let tiny = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../keeppix-jobs/tests/fixtures/tiny.jpg");
    fs::copy(&tiny, root.join("nuovo.jpg")).unwrap();

    let start = std::time::Instant::now();
    loop {
        assert!(
            start.elapsed() < Duration::from_secs(8),
            "watcher did not enqueue discover after a new file"
        );
        let n: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM jobs WHERE kind = 'discover_library' AND status = 'pending'",
        )
        .fetch_one(server.db.pool())
        .await
        .unwrap();
        if n > 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }

    keeppix_jobs::discover::run(&server.db, library_id, Duration::ZERO)
        .await
        .unwrap();
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM assets")
        .fetch_one(server.db.pool())
        .await
        .unwrap();
    assert_eq!(n, 1);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn an_unreachable_library_goes_offline_and_deletes_nothing() {
    let server = TestServer::start().await;
    setup_admin(&server).await;
    let root = library_dir(&server, "gone-soon");
    fs::write(root.join("foto.jpg"), b"x").unwrap();

    let created = server
        .client
        .post(server.url("/api/v1/libraries"))
        .json(&json!({
            "name": "Gone",
            "root_path": root.to_string_lossy(),
        }))
        .send()
        .await
        .unwrap();
    let id = created.json::<serde_json::Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let library_id: keeppix_domain::LibraryId = id.parse().unwrap();

    let scan = server
        .client
        .post(server.url(&format!("/api/v1/libraries/{id}/scan")))
        .send()
        .await
        .unwrap();
    assert_eq!(scan.status(), 202);
    drain_workers(&server, &server.data_dir.join("gone-data")).await;

    let before: i64 = sqlx::query_scalar("SELECT count(*) FROM assets")
        .fetch_one(server.db.pool())
        .await
        .unwrap();
    assert!(before >= 1);

    fs::remove_dir_all(&root).unwrap();

    let again = server
        .client
        .post(server.url(&format!("/api/v1/libraries/{id}/scan")))
        .send()
        .await
        .unwrap();
    assert_eq!(again.status(), 202);
    // Discovery only: missing root → offline, no deletions.
    keeppix_jobs::discover::run(&server.db, library_id, Duration::ZERO)
        .await
        .unwrap();

    let admin = server
        .client
        .get(server.url("/api/v1/auth/me"))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap()["user"]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let lib = keeppix_db::LibraryRepo::new(&server.db)
        .find_by_id(&AuthContext::user(admin, SystemRole::Admin), library_id)
        .await
        .unwrap();
    assert_eq!(lib.status, keeppix_domain::LibraryStatus::Offline);

    let after: i64 = sqlx::query_scalar("SELECT count(*) FROM assets")
        .fetch_one(server.db.pool())
        .await
        .unwrap();
    assert_eq!(before, after);
}
