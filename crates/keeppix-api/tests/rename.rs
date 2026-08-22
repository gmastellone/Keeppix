mod harness;

use std::fs;
use std::path::PathBuf;

use chrono::{TimeZone, Utc};
use harness::TestServer;
use keeppix_db::{AssetRepo, FolderRepo, LibraryRepo, OperationsRepo};
use keeppix_domain::{
    AssetId, AssetKind, AssetName, AuthContext, FolderId, NewAsset, NewLibrary, OperationStatus,
    SystemRole, UserId,
};
use serde_json::json;

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
        .expect("richiesta di setup");
    let body: serde_json::Value = response.json().await.expect("corpo JSON");
    body["user"]["id"]
        .as_str()
        .expect("id utente")
        .parse()
        .expect("uuid valido")
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
        .expect("libreria")
        .id;
    fs::create_dir_all(root.join("2024")).expect("cartella su disco");
    FolderRepo::new(&server.db)
        .ensure_path(library, &["2024"])
        .await
        .expect("cartella")
        .id
}

#[allow(clippy::expect_used, clippy::unwrap_used)]
async fn seed_indexed_asset(
    server: &TestServer,
    folder: FolderId,
    root: &std::path::Path,
    filename: &str,
) -> AssetId {
    fs::write(root.join("2024").join(filename), b"contenuto").expect("file su disco");

    let repo = AssetRepo::new(&server.db);
    let asset = repo
        .upsert_discovered(NewAsset {
            folder_id: folder,
            filename: AssetName::parse(filename).expect("nome"),
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
    .expect("indicizzazione");
    asset
}

#[allow(clippy::expect_used, clippy::unwrap_used)]
fn temp_root() -> PathBuf {
    let root = std::env::temp_dir().join(format!("keeppix-api-rename-{}", uuid::Uuid::now_v7()));
    fs::create_dir_all(&root).expect("radice di test");
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
        "preview non tocca il disco"
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
    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();

    let operation_id: keeppix_domain::OperationId =
        body["operation_id"].as_str().unwrap().parse().unwrap();
    assert_eq!(body["outcome"]["succeeded"].as_array().unwrap().len(), 2);
    assert!(body["outcome"]["failed"].as_array().unwrap().is_empty());
    let batch_id = body["outcome"]["batch_id"]
        .as_str()
        .expect("batch_id")
        .to_owned();

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

    let response = server
        .client
        .post(server.url(&format!("/api/v1/assets/batch/rename/{batch_id}/undo")))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["outcome"]["succeeded"].as_array().unwrap().len(), 2);

    assert!(root.join("2024").join("a.jpg").is_file());
    assert!(root.join("2024").join("b.jpg").is_file());
    assert!(!root.join("2024").join("vacanza_01.JPG").exists());

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
    let _target = seed_indexed_asset(&server, folder, &root, "TARGET.JPG").await;

    let response = server
        .client
        .post(server.url("/api/v1/assets/batch/rename"))
        .json(&json!({ "asset_ids": [a.to_string(), b.to_string()], "schema": "target" }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();

    // `a` and `b` both compute to `target.JPG`: collisions are only known
    // for certain at write time (Ruling, `rename.rs`), so `apply` processes
    // the group sequentially — the first one through wins the name, the
    // second finds it already taken and fails with `collision`.
    assert_eq!(body["outcome"]["succeeded"].as_array().unwrap().len(), 1);
    let failed = body["outcome"]["failed"].as_array().unwrap();
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0]["reason"], "collision");

    assert!(root.join("2024").join("target.JPG").is_file());
    assert!(root.join("2024").join("TARGET.JPG").is_file());

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
