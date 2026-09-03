mod harness;

use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use chrono::{TimeZone, Utc};
use harness::TestServer;
use keeppix_db::{AssetRepo, FolderRepo, LibraryRepo, OperationsRepo};
use keeppix_domain::{
    AssetId, AssetKind, AssetName, AuthContext, FolderId, NewAsset, NewLibrary, OperationStatus,
    SystemRole, UserId,
};
use keeppix_jobs::{ActivityTracker, IngestHandler, WorkerPool};
use serde_json::json;

/// `POST /assets/batch/rename` enqueues a `JobKind::BulkRename` instead of
/// renaming inline within the request (see the comment at the top of
/// `routes/rename.rs`) — same pattern as `scan.rs::drain_workers`, runs
/// the worker until the queue is empty.
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

#[allow(clippy::expect_used, clippy::unwrap_used)]
async fn setup_admin(server: &TestServer) -> UserId {
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
        .expect("setup request");
    let body: serde_json::Value = response.json().await.expect("JSON body");
    body["user"]["id"]
        .as_str()
        .expect("user id")
        .parse()
        .expect("valid uuid")
}

#[allow(clippy::expect_used, clippy::unwrap_used)]
async fn ensure_folder(server: &TestServer, admin: UserId, root: &std::path::Path) -> FolderId {
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let library = LibraryRepo::new(&server.db)
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
        .id;
    fs::create_dir_all(root.join("2024")).expect("folder on disk");
    FolderRepo::new(&server.db)
        .ensure_path(library, &["2024"])
        .await
        .expect("folder")
        .id
}

#[allow(clippy::expect_used, clippy::unwrap_used)]
async fn seed_indexed_asset(
    server: &TestServer,
    folder: FolderId,
    root: &std::path::Path,
    filename: &str,
) -> AssetId {
    fs::write(root.join("2024").join(filename), b"content").expect("file on disk");

    let repo = AssetRepo::new(&server.db);
    let asset = repo
        .upsert_discovered(NewAsset {
            folder_id: folder,
            filename: AssetName::parse(filename).expect("name"),
            size_bytes: 9,
            mtime: Utc.with_ymd_and_hms(2024, 6, 1, 12, 0, 0).unwrap(),
            inode: None,
            kind: AssetKind::Image,
        })
        .await
        .expect("asset")
        .unwrap()
        .id;
    repo.set_indexed(
        asset,
        Utc.with_ymd_and_hms(2024, 6, 1, 10, 0, 0).unwrap(),
        100,
        100,
    )
    .await
    .expect("indexing");
    asset
}

#[allow(clippy::expect_used, clippy::unwrap_used)]
fn temp_root() -> PathBuf {
    let root = std::env::temp_dir().join(format!("keeppix-api-rename-{}", uuid::Uuid::now_v7()));
    fs::create_dir_all(&root).expect("test root");
    root
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn preview_computes_names_and_writes_nothing() {
    let server = TestServer::start().await;
    let admin = setup_admin(&server).await;
    let root = temp_root();
    let folder = ensure_folder(&server, admin, &root).await;
    let a = seed_indexed_asset(&server, folder, &root, "a.jpg").await;

    let response = server
        .client
        .post(server.url("/api/v1/assets/batch/rename/preview"))
        .json(&json!({ "asset_ids": [a.to_string()], "schema": "vacanza_{n:2}" }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    let items = body.as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["current_name"], "a.jpg");
    assert_eq!(items[0]["new_name"], "vacanza_01.JPG");
    assert_eq!(items[0]["collides"], false);

    assert!(
        root.join("2024").join("a.jpg").is_file(),
        "preview does not touch the disk"
    );
    assert!(!root.join("2024").join("vacanza_01.JPG").exists());

    let _ = fs::remove_dir_all(&root);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn apply_batch_renames_on_disk_and_tracks_a_finished_operation() {
    let server = TestServer::start().await;
    let admin = setup_admin(&server).await;
    let root = temp_root();
    let folder = ensure_folder(&server, admin, &root).await;
    let a = seed_indexed_asset(&server, folder, &root, "a.jpg").await;
    let b = seed_indexed_asset(&server, folder, &root, "b.jpg").await;

    let response = server
        .client
        .post(server.url("/api/v1/assets/batch/rename"))
        .json(&json!({ "asset_ids": [a.to_string(), b.to_string()], "schema": "vacanza_{n:2}" }))
        .send()
        .await
        .unwrap();
    // 202 right away, the actual work runs in the background
    // (JobKind::BulkRename) — see the comment at the top of routes/rename.rs.
    assert_eq!(response.status(), 202);
    let body: serde_json::Value = response.json().await.unwrap();
    let operation_id: keeppix_domain::OperationId =
        body["operation_id"].as_str().unwrap().parse().unwrap();

    drain_workers(&server, &server.data_dir.join("rename-apply-data")).await;

    assert!(root.join("2024").join("vacanza_01.JPG").is_file());
    assert!(root.join("2024").join("vacanza_02.JPG").is_file());
    assert!(!root.join("2024").join("a.jpg").exists());

    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let operation = OperationsRepo::new(&server.db)
        .find(&ctx, operation_id)
        .await
        .unwrap();
    assert_eq!(operation.status, OperationStatus::Done);
    assert_eq!(operation.total, Some(2));
    assert_eq!(operation.done, 2);
    assert_eq!(operation.phase, "renaming");
    assert_eq!(operation.succeeded.len(), 2);

    // `undo` runs in the background too now (JobKind::RenameUndo) — batch_id
    // read from the database since apply's 202 response doesn't carry it.
    let batch_id: uuid::Uuid =
        sqlx::query_scalar("SELECT id FROM rename_batches ORDER BY id LIMIT 1")
            .fetch_one(server.db.pool())
            .await
            .unwrap();

    let response = server
        .client
        .post(server.url(&format!("/api/v1/assets/batch/rename/{batch_id}/undo")))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 202);
    let body: serde_json::Value = response.json().await.unwrap();
    let undo_operation_id: keeppix_domain::OperationId =
        body["operation_id"].as_str().unwrap().parse().unwrap();

    drain_workers(&server, &server.data_dir.join("rename-undo-data")).await;

    assert!(root.join("2024").join("a.jpg").is_file());
    assert!(root.join("2024").join("b.jpg").is_file());
    assert!(!root.join("2024").join("vacanza_01.JPG").exists());

    let undo_operation = OperationsRepo::new(&server.db)
        .find(&ctx, undo_operation_id)
        .await
        .unwrap();
    assert_eq!(undo_operation.status, OperationStatus::Done);
    assert_eq!(undo_operation.total, Some(2));
    assert_eq!(undo_operation.done, 2);
    assert_eq!(undo_operation.phase, "undoing");
    assert_eq!(undo_operation.succeeded.len(), 2);

    let _ = fs::remove_dir_all(&root);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn apply_batch_reports_a_collision_without_blocking_the_rest() {
    let server = TestServer::start().await;
    let admin = setup_admin(&server).await;
    let root = temp_root();
    let folder = ensure_folder(&server, admin, &root).await;
    let a = seed_indexed_asset(&server, folder, &root, "a.jpg").await;
    let b = seed_indexed_asset(&server, folder, &root, "b.jpg").await;
    // Named "OTHER", not a same-word different-case "TARGET": on a
    // case-insensitive filesystem (the default on macOS/Windows, and
    // common for external/cloud-synced storage), "target.JPG" and
    // "TARGET.JPG" are the same path — move_asset's on-disk collision
    // check (assets.rs) correctly refuses to rename into that, since
    // doing so would silently overwrite the pre-existing file. This decoy
    // only needs to prove that an unrelated existing asset doesn't
    // interfere with the batch, so its name must not collide by case.
    let _other = seed_indexed_asset(&server, folder, &root, "OTHER.JPG").await;

    let response = server
        .client
        .post(server.url("/api/v1/assets/batch/rename"))
        .json(&json!({ "asset_ids": [a.to_string(), b.to_string()], "schema": "target" }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 202);
    let body: serde_json::Value = response.json().await.unwrap();
    let operation_id: keeppix_domain::OperationId =
        body["operation_id"].as_str().unwrap().parse().unwrap();

    drain_workers(&server, &server.data_dir.join("rename-collision-data")).await;

    // `a` and `b` both compute to `target.JPG`: collisions are only known
    // for certain at write time (see `rename.rs`), so `apply` processes
    // the group sequentially — the first one through wins the name, the
    // second finds it already taken and fails with `collision`. The route
    // responds right away with just an `operation_id` (the work runs in
    // the background): the precise reason for the failure
    // (`"collision"`) no longer survives to the caller — only
    // `succeeded`/`done`/`total` remain readable on the `Operation`, the
    // same limitation already present for every other background
    // operation in this system (`LibraryScan` doesn't report why a file
    // failed, only how many succeeded).
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let operation = OperationsRepo::new(&server.db)
        .find(&ctx, operation_id)
        .await
        .unwrap();
    assert_eq!(operation.status, OperationStatus::Done);
    assert_eq!(operation.total, Some(2), "both compute a new name");
    assert_eq!(
        operation.done, 1,
        "only the first of the two wins the contested name"
    );
    assert_eq!(operation.succeeded.len(), 1);

    assert!(root.join("2024").join("target.JPG").is_file());
    assert!(root.join("2024").join("OTHER.JPG").is_file());

    let _ = fs::remove_dir_all(&root);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn apply_batch_rejects_a_lot_larger_than_the_hard_cap() {
    let server = TestServer::start().await;
    let _admin = setup_admin(&server).await;

    let ids: Vec<String> = (0..=keeppix_api::batch::MAX_BATCH_ASSETS)
        .map(|_| AssetId::new().to_string())
        .collect();

    let response = server
        .client
        .post(server.url("/api/v1/assets/batch/rename"))
        .json(&json!({ "asset_ids": ids, "schema": "{n:3}" }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 400);
    let body: serde_json::Value = response.json().await.unwrap();
    assert!(body["type"].as_str().unwrap().contains("batch-too-large"));
}
