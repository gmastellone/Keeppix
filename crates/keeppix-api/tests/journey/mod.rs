//! Shared helpers for HTTP-only user-journey integration tests.
#![allow(dead_code)]

use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::harness::{self, TestServer};
use keeppix_domain::LibraryId;
use keeppix_jobs::{ActivityTracker, IngestHandler, WorkerPool};
use serde_json::json;

pub const ADMIN_PASSWORD: &str = "correct horse battery staple";

pub struct FixtureArchive {
    pub root: PathBuf,
    pub photo_count: i64,
}

#[allow(clippy::expect_used)]
pub fn tiny_fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../keeppix-jobs/tests/fixtures/tiny.jpg")
}

/// Two subfolders, three `tiny.jpg` copies each — six real JPEGs under `photos_root`.
#[allow(clippy::expect_used)]
pub fn build_fixture_archive(server: &TestServer) -> FixtureArchive {
    let root = server
        .photos_root
        .join(format!("archive-{}", uuid::Uuid::now_v7().simple()));
    let tiny = tiny_fixture_path();
    let bytes = fs::read(&tiny).expect("tiny.jpg");
    let folders = ["album-a", "album-b"];
    let mut photo_count = 0_i64;
    for folder in folders {
        let dir = root.join(folder);
        fs::create_dir_all(&dir).expect("fixture dir");
        for i in 0..3 {
            fs::write(dir.join(format!("photo-{i}.jpg")), &bytes).expect("fixture file");
            photo_count += 1;
        }
    }
    FixtureArchive { root, photo_count }
}

#[allow(clippy::expect_used)]
pub async fn setup_admin(server: &TestServer) {
    let response = server
        .client
        .post(server.url("/api/v1/setup"))
        .json(&json!({
            "username": "giovanni",
            "display_name": "Giovanni",
            "password": ADMIN_PASSWORD
        }))
        .send()
        .await
        .expect("setup");
    assert_eq!(response.status(), 201);
}

#[allow(clippy::expect_used)]
pub async fn login_as(server: &TestServer, username: &str, password: &str) -> reqwest::Client {
    let client = reqwest::Client::builder()
        .cookie_store(true)
        .default_headers(harness::client_headers())
        .build()
        .expect("client");
    let response = client
        .post(server.url("/api/v1/auth/login"))
        .json(&json!({ "username": username, "password": password }))
        .send()
        .await
        .expect("login");
    assert_eq!(response.status(), 200);
    client
}

#[allow(clippy::expect_used)]
pub async fn create_library(
    server: &TestServer,
    name: &str,
    root_path: &std::path::Path,
) -> String {
    let response = server
        .client
        .post(server.url("/api/v1/libraries"))
        .json(&json!({
            "name": name,
            "root_path": root_path.to_string_lossy(),
        }))
        .send()
        .await
        .expect("create library");
    assert_eq!(response.status(), 201);
    response
        .json::<serde_json::Value>()
        .await
        .expect("library json")["id"]
        .as_str()
        .expect("library id")
        .to_owned()
}

#[allow(clippy::expect_used)]
pub async fn start_scan(server: &TestServer, library_id: &str) {
    let response = server
        .client
        .post(server.url(&format!("/api/v1/libraries/{library_id}/scan")))
        .send()
        .await
        .expect("start scan");
    assert_eq!(response.status(), 202);
}

fn ingest_pool(server: &TestServer) -> WorkerPool<IngestHandler> {
    let handler = IngestHandler {
        db: server.db.clone(),
        data_dir: server.data_dir.clone(),
        stability_wait: Duration::ZERO,
        trash_retention_days: keeppix_db::TRASH_RETENTION_DAYS,
    };
    WorkerPool::new(
        server.db.clone(),
        handler,
        std::sync::Arc::new(ActivityTracker::new()),
        512 * 1024 * 1024,
        keeppix_jobs::default_night_window(),
        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
    )
}

#[allow(clippy::expect_used)]
async fn pending_jobs(server: &TestServer) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM jobs WHERE status IN ('pending','running')")
        .fetch_one(server.db.pool())
        .await
        .expect("pending jobs")
}

#[allow(clippy::expect_used)]
async fn indexed_in_library(server: &TestServer, library_id: LibraryId) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM assets a \
         JOIN folders fo ON fo.id = a.folder_id \
         WHERE fo.library_id = $1 \
           AND a.status = 'indexed' \
           AND a.taken_at_utc IS NOT NULL",
    )
    .bind(library_id.as_uuid())
    .fetch_one(server.db.pool())
    .await
    .expect("indexed count")
}

#[allow(clippy::expect_used)]
async fn scan_status_body(server: &TestServer, library_id: &str) -> serde_json::Value {
    let response = server
        .client
        .get(server.url(&format!("/api/v1/libraries/{library_id}/scan")))
        .send()
        .await
        .expect("scan status");
    assert_eq!(response.status(), 200);
    response.json().await.expect("scan json")
}

/// Drains the ingest worker pool until no pending/running jobs remain.
#[allow(clippy::expect_used)]
pub async fn drain_workers(server: &TestServer, deadline: Instant) {
    let pool = ingest_pool(server);
    loop {
        assert!(Instant::now() < deadline, "drain_workers timed out");
        if !pool.step().await.expect("worker step") && pending_jobs(server).await == 0 {
            break;
        }
    }
}

/// Polls scan status over HTTP and steps workers until discovery is idle, every
/// ingest job has finished, and the expected number of assets are indexed with
/// `taken_at_utc` (timeline buckets need EXIF dates).
///
/// Returns elapsed wall time for ledger reporting.
#[allow(clippy::expect_used)]
pub async fn wait_for_scan(
    server: &TestServer,
    library_id: &str,
    expected_assets: i64,
    deadline: Instant,
) -> Duration {
    let started = Instant::now();
    let library_uuid: LibraryId = library_id.parse().expect("library uuid");
    let pool = ingest_pool(server);

    loop {
        assert!(
            Instant::now() < deadline,
            "wait_for_scan timed out after {:?}",
            started.elapsed()
        );

        if !pool.step().await.expect("worker step") && pending_jobs(server).await == 0 {
            let body = scan_status_body(server, library_id).await;
            let phase = body["phase"].as_str().unwrap_or("");
            let asset_count = body["asset_count"].as_i64().unwrap_or(0);
            let indexed = indexed_in_library(server, library_uuid).await;
            if phase == "idle" && asset_count >= expected_assets && indexed >= expected_assets {
                return started.elapsed();
            }
        }

        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[allow(clippy::expect_used)]
pub async fn scan_and_wait(
    server: &TestServer,
    library_id: &str,
    expected_assets: i64,
    deadline: Instant,
) -> Duration {
    start_scan(server, library_id).await;
    wait_for_scan(server, library_id, expected_assets, deadline).await
}

#[allow(clippy::expect_used)]
pub async fn create_user(server: &TestServer, username: &str, password: &str) -> String {
    let response = server
        .client
        .post(server.url("/api/v1/users"))
        .json(&json!({
            "username": username,
            "display_name": username,
            "password": password,
            "role": "user"
        }))
        .send()
        .await
        .expect("create user");
    assert_eq!(response.status(), 201);
    response
        .json::<serde_json::Value>()
        .await
        .expect("user json")["id"]
        .as_str()
        .expect("user id")
        .to_owned()
}

#[allow(clippy::expect_used)]
pub async fn folder_id_by_name(server: &TestServer, name: &str) -> String {
    let tree = server
        .client
        .get(server.url("/api/v1/folders/tree"))
        .send()
        .await
        .expect("folders tree")
        .json::<serde_json::Value>()
        .await
        .expect("tree json");
    tree.as_array()
        .expect("tree array")
        .iter()
        .find(|f| f["name"].as_str() == Some(name))
        .and_then(|f| f["id"].as_str())
        .unwrap_or_else(|| panic!("folder {name} not found"))
        .to_owned()
}

#[allow(clippy::expect_used)]
pub async fn grant_folder_viewer(server: &TestServer, subject_user_id: &str, folder_id: &str) {
    let response = server
        .client
        .post(server.url("/api/v1/permissions"))
        .json(&json!({
            "subject_type": "user",
            "subject_id": subject_user_id,
            "object_type": "folder",
            "object_id": folder_id,
            "role": "viewer",
            "inherit": true
        }))
        .send()
        .await
        .expect("grant permission");
    assert_eq!(response.status(), 201);
}

#[allow(clippy::expect_used)]
pub fn share_client(token: &str) -> reqwest::Client {
    let mut headers = harness::client_headers();
    headers.insert(
        reqwest::header::HeaderName::from_static("x-share-token"),
        reqwest::header::HeaderValue::from_str(token).expect("token header"),
    );
    reqwest::Client::builder()
        .default_headers(headers)
        .build()
        .expect("share client")
}

#[allow(clippy::expect_used)]
pub async fn create_share_link(
    server: &TestServer,
    object_type: &str,
    object_id: &str,
    password: Option<&str>,
) -> String {
    let mut body = json!({
        "object_type": object_type,
        "object_id": object_id,
    });
    if let Some(pw) = password {
        body["password"] = json!(pw);
    }
    let response = server
        .client
        .post(server.url("/api/v1/share/links"))
        .json(&body)
        .send()
        .await
        .expect("create share link");
    assert_eq!(response.status(), 201);
    response
        .json::<serde_json::Value>()
        .await
        .expect("link json")["token"]
        .as_str()
        .expect("token")
        .to_owned()
}
