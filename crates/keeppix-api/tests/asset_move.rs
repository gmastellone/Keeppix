//! Fase 11 Task 7 (§13.3 campo 8, "Sposta in cartella") — `POST
//! /assets/batch/move`, primo consumatore di `AssetRepo::move_to_folder`.

mod harness;

use std::fs;
use std::path::PathBuf;

use chrono::{TimeZone, Utc};
use harness::TestServer;
use keeppix_db::{AssetRepo, FolderRepo, LibraryRepo};
use keeppix_domain::{
    AssetId, AssetKind, AssetName, AuthContext, FolderId, NewAsset, NewLibrary, SystemRole, UserId,
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
async fn ensure_folder(
    server: &TestServer,
    admin: UserId,
    root: &std::path::Path,
    segments: &[&str],
) -> FolderId {
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
    let mut dir = root.to_path_buf();
    for segment in segments {
        dir = dir.join(segment);
    }
    fs::create_dir_all(&dir).expect("cartella su disco");
    FolderRepo::new(&server.db)
        .ensure_path(library, segments)
        .await
        .expect("cartella")
        .id
}

#[allow(clippy::expect_used, clippy::unwrap_used)]
async fn seed_indexed_asset(
    server: &TestServer,
    folder: FolderId,
    dir: &std::path::Path,
    filename: &str,
) -> AssetId {
    fs::write(dir.join(filename), b"contenuto").expect("file su disco");

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
        .expect("indicizzazione");
    asset.expect("nuovo asset").id
}

#[allow(clippy::expect_used, clippy::unwrap_used)]
fn temp_root() -> PathBuf {
    let root =
        std::env::temp_dir().join(format!("keeppix-api-asset-move-{}", uuid::Uuid::now_v7()));
    fs::create_dir_all(&root).expect("radice di test");
    root
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn moves_every_asset_in_the_batch_keeping_its_filename() {
    let server = TestServer::start().await;
    let admin = setup_admin(&server).await;
    let root = temp_root();
    let src = ensure_folder(&server, admin, &root, &["2024"]).await;
    let dst = ensure_folder(&server, admin, &root, &["2024", "Scelte"]).await;
    let a = seed_indexed_asset(&server, src, &root.join("2024"), "a.jpg").await;
    let b = seed_indexed_asset(&server, src, &root.join("2024"), "b.jpg").await;

    let response = server
        .client
        .post(server.url("/api/v1/assets/batch/move"))
        .json(&json!({ "asset_ids": [a.to_string(), b.to_string()], "folder_id": dst.to_string() }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["succeeded"].as_array().unwrap().len(), 2);
    assert!(body["failed"].as_array().unwrap().is_empty());

    assert!(root.join("2024").join("Scelte").join("a.jpg").is_file());
    assert!(root.join("2024").join("Scelte").join("b.jpg").is_file());
    assert!(!root.join("2024").join("a.jpg").exists());

    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let moved = AssetRepo::new(&server.db)
        .find_by_id(&ctx, a)
        .await
        .unwrap();
    assert_eq!(moved.folder_id, dst);
    assert_eq!(moved.filename.as_str(), "a.jpg", "spostare non rinomina");

    let _ = fs::remove_dir_all(&root);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn a_collision_fails_only_that_asset_not_the_whole_batch() {
    let server = TestServer::start().await;
    let admin = setup_admin(&server).await;
    let root = temp_root();
    let src = ensure_folder(&server, admin, &root, &["2024"]).await;
    let dst = ensure_folder(&server, admin, &root, &["2024", "Scelte"]).await;
    let clean = seed_indexed_asset(&server, src, &root.join("2024"), "clean.jpg").await;
    let clashing = seed_indexed_asset(&server, src, &root.join("2024"), "taken.jpg").await;
    let _already_there =
        seed_indexed_asset(&server, dst, &root.join("2024").join("Scelte"), "taken.jpg").await;

    let response = server
        .client
        .post(server.url("/api/v1/assets/batch/move"))
        .json(&json!({ "asset_ids": [clean.to_string(), clashing.to_string()], "folder_id": dst.to_string() }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["succeeded"].as_array().unwrap(), &[clean.to_string()]);
    let failed = body["failed"].as_array().unwrap();
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0]["id"], clashing.to_string());
    assert_eq!(failed[0]["reason"], "collision");

    assert!(root.join("2024").join("Scelte").join("clean.jpg").is_file());
    assert!(
        root.join("2024").join("taken.jpg").is_file(),
        "il file in conflitto resta al suo posto"
    );

    let _ = fs::remove_dir_all(&root);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn rejects_a_batch_larger_than_the_hard_cap() {
    let server = TestServer::start().await;
    let _admin = setup_admin(&server).await;

    let ids: Vec<String> = (0..=keeppix_api::batch::MAX_BATCH_ASSETS)
        .map(|_| AssetId::new().to_string())
        .collect();

    let response = server
        .client
        .post(server.url("/api/v1/assets/batch/move"))
        .json(&json!({ "asset_ids": ids, "folder_id": FolderId::new().to_string() }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 400);
    let body: serde_json::Value = response.json().await.unwrap();
    assert!(body["type"].as_str().unwrap().contains("batch-too-large"));
}
