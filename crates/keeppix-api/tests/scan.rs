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
    let handler = IngestHandler {
        db: server.db.clone(),
        data_dir: data_dir.to_path_buf(),
        stability_wait: Duration::ZERO,
    };
    let pool = WorkerPool::new(
        server.db.clone(),
        handler,
        std::sync::Arc::new(ActivityTracker::new()),
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
                "SELECT count(*) FROM jobs WHERE status IN ('pending','running')",
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
    assert_eq!(n, 1, "dedup_key deve collassare le due richieste");
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

    // Lascia avviare il watcher avviato alla create.
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
            "watcher non ha accodato discover dopo un file nuovo"
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
    // Solo discovery: root mancante → offline, niente cancellazioni.
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
