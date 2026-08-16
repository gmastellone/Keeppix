mod harness;

use std::fs;
use std::path::PathBuf;

use chrono::{TimeZone, Utc};
use harness::TestServer;
use keeppix_db::{AssetRepo, FolderRepo, LibraryRepo, UserRepo};
use keeppix_domain::{
    AssetId, AssetKind, AssetName, AuthContext, NewAsset, NewLibrary, NewUser, Password,
    SystemRole, UserId, Username, hash_password,
};
use serde_json::json;

#[allow(clippy::expect_used)]
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
        .expect("setup");
    assert_eq!(response.status(), 201);
    response.json::<serde_json::Value>().await.expect("json")["user"]["id"]
        .as_str()
        .expect("id")
        .parse()
        .expect("uuid")
}

#[allow(clippy::expect_used)]
async fn login_as(server: &TestServer, username: &str, password: &str) -> reqwest::Client {
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
fn temp_root(server: &TestServer, name: &str) -> PathBuf {
    let root = server.photos_root.join(name);
    fs::create_dir_all(&root).expect("radice");
    root
}

#[allow(clippy::expect_used)]
async fn seed_library(
    server: &TestServer,
    admin: UserId,
    root: &std::path::Path,
) -> keeppix_domain::LibraryId {
    LibraryRepo::new(&server.db)
        .create(
            &AuthContext::user(admin, SystemRole::Admin),
            NewLibrary {
                name: "Foto".to_owned(),
                owner_id: admin,
                root_path: root.to_path_buf(),
                exclude_patterns: vec![],
            },
        )
        .await
        .expect("libreria")
        .id
}

#[allow(clippy::expect_used)]
async fn seed_asset(
    server: &TestServer,
    library: keeppix_domain::LibraryId,
    root: &std::path::Path,
    folder: &str,
    filename: &str,
) -> AssetId {
    let folder_id = FolderRepo::new(&server.db)
        .ensure_path(library, &[folder])
        .await
        .expect("cartella")
        .id;
    fs::create_dir_all(root.join(folder)).expect("cartella su disco");
    fs::write(root.join(folder).join(filename), b"contenuto").expect("file");

    AssetRepo::new(&server.db)
        .upsert_discovered(NewAsset {
            folder_id,
            filename: AssetName::parse(filename).expect("nome"),
            size_bytes: 9,
            mtime: Utc.with_ymd_and_hms(2024, 6, 1, 12, 0, 0).unwrap(),
            inode: None,
            kind: AssetKind::Image,
        })
        .await
        .expect("asset")
        .id
}

#[allow(clippy::expect_used)]
async fn trash_asset(server: &TestServer, asset_id: AssetId) {
    let response = server
        .client
        .delete(server.url(&format!("/api/v1/assets/{asset_id}")))
        .json(&json!({ "disk_action": "moved_to_trash" }))
        .send()
        .await
        .expect("delete");
    assert_eq!(response.status(), 204);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn trash_list_is_keyset_paginated() {
    let server = TestServer::start().await;
    let admin = setup_admin(&server).await;
    let root = temp_root(&server, "paginated");
    let library = seed_library(&server, admin, &root).await;

    let mut ids = Vec::new();
    for i in 0..3 {
        ids.push(seed_asset(&server, library, &root, "2024", &format!("foto-{i}.jpg")).await);
    }
    for id in &ids {
        trash_asset(&server, *id).await;
    }

    let page1 = server
        .client
        .get(server.url("/api/v1/trash?limit=2"))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    assert_eq!(page1["items"].as_array().unwrap().len(), 2);
    let cursor = page1["next_cursor"]
        .as_str()
        .expect("next_cursor sulla prima pagina");

    let page2 = server
        .client
        .get(server.url(&format!("/api/v1/trash?limit=2&cursor={cursor}")))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    assert_eq!(page2["items"].as_array().unwrap().len(), 1);
    assert!(page2["next_cursor"].is_null());
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn trash_list_shows_days_remaining() {
    let server = TestServer::start().await;
    let admin = setup_admin(&server).await;
    let root = temp_root(&server, "days");
    let library = seed_library(&server, admin, &root).await;
    let asset_id = seed_asset(&server, library, &root, "2024", "foto.jpg").await;
    trash_asset(&server, asset_id).await;

    let page = server
        .client
        .get(server.url("/api/v1/trash"))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    let item = &page["items"][0];
    assert_eq!(item["days_remaining"].as_i64().unwrap(), 30);
    assert!(item["deleted_at"].is_string());
    assert_eq!(item["disk_action"], "moved_to_trash");
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn emptying_trash_requires_owner_or_admin() {
    let server = TestServer::start().await;
    let admin = setup_admin(&server).await;
    let root = temp_root(&server, "empty-forbidden");
    let library = seed_library(&server, admin, &root).await;
    let asset_id = seed_asset(&server, library, &root, "2024", "foto.jpg").await;
    trash_asset(&server, asset_id).await;

    UserRepo::new(&server.db)
        .create(
            &AuthContext::user(admin, SystemRole::Admin),
            NewUser {
                username: Username::parse("mario").unwrap(),
                email: None,
                display_name: "Mario".to_owned(),
                password_hash: hash_password(&Password::parse("mario-password-ok").unwrap())
                    .unwrap()
                    .as_str()
                    .to_owned(),
                role: SystemRole::User,
            },
        )
        .await
        .unwrap();

    let mario = login_as(&server, "mario", "mario-password-ok").await;
    let response = mario
        .post(server.url("/api/v1/trash/empty"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 403);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn a_plain_user_does_not_see_someone_elses_trash() {
    let server = TestServer::start().await;
    let admin = setup_admin(&server).await;
    let root = temp_root(&server, "isolation");
    let library = seed_library(&server, admin, &root).await;
    let asset_id = seed_asset(&server, library, &root, "2024", "segreta.jpg").await;
    trash_asset(&server, asset_id).await;

    UserRepo::new(&server.db)
        .create(
            &AuthContext::user(admin, SystemRole::Admin),
            NewUser {
                username: Username::parse("mario").unwrap(),
                email: None,
                display_name: "Mario".to_owned(),
                password_hash: hash_password(&Password::parse("mario-password-ok").unwrap())
                    .unwrap()
                    .as_str()
                    .to_owned(),
                role: SystemRole::User,
            },
        )
        .await
        .unwrap();

    let mario = login_as(&server, "mario", "mario-password-ok").await;
    let page = mario
        .get(server.url("/api/v1/trash"))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    assert!(page["items"].as_array().unwrap().is_empty());
}
