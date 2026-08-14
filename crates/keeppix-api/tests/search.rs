mod harness;

use chrono::{TimeZone, Utc};
use harness::TestServer;
use keeppix_db::{AssetRepo, FolderRepo, LibraryRepo, UserRepo};
use keeppix_domain::{
    AssetKind, AssetName, AuthContext, NewAsset, NewLibrary, SystemRole, Username,
};
use serde_json::json;

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn search_from_ast_does_not_run_user_sql() {
    let server = TestServer::start().await;
    seed_photo(&server, "grecia.jpg").await;

    let response = server
        .client
        .post(server.url("/api/v1/search"))
        .json(&json!({
            "ast": { "op": "text", "value": "grecia" }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["assets"].as_array().unwrap().len(), 1);

    let injected = server
        .client
        .post(server.url("/api/v1/search"))
        .json(&json!({
            "ast": { "op": "text", "value": "grecia'; drop table assets; --" }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(injected.status(), 200);
    let empty: serde_json::Value = injected.json().await.unwrap();
    assert!(empty["assets"].as_array().unwrap().is_empty());
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn saved_searches_round_trip() {
    let server = TestServer::start().await;
    setup(&server).await;
    let created = server
        .client
        .post(server.url("/api/v1/saved-searches"))
        .json(&json!({ "name": "Grecia", "query_text": "grecia" }))
        .send()
        .await
        .unwrap();
    assert_eq!(created.status(), 201);

    let list = server
        .client
        .get(server.url("/api/v1/saved-searches"))
        .send()
        .await
        .unwrap();
    assert_eq!(list.status(), 200);
    let body: serde_json::Value = list.json().await.unwrap();
    assert_eq!(body[0]["name"], "Grecia");
}

#[allow(clippy::unwrap_used)]
async fn setup(server: &TestServer) {
    server
        .client
        .post(server.url("/api/v1/setup"))
        .json(&json!({
            "username": "giovanni",
            "display_name": "Giovanni",
            "password": "correct horse battery staple"
        }))
        .send()
        .await
        .unwrap();
}

#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn seed_photo(server: &TestServer, name: &str) {
    setup(server).await;
    let username = Username::parse("giovanni").unwrap();
    let (user, _) = UserRepo::new(&server.db)
        .find_by_username(&username)
        .await
        .unwrap()
        .expect("admin");
    let ctx = AuthContext::user(user.id, SystemRole::Admin);
    let library = LibraryRepo::new(&server.db)
        .create(
            &ctx,
            NewLibrary {
                name: "Foto".to_owned(),
                owner_id: user.id,
                root_path: std::path::PathBuf::from("/mnt/foto"),
                exclude_patterns: vec![],
            },
        )
        .await
        .unwrap();
    let folder = FolderRepo::new(&server.db)
        .ensure_path(library.id, &[])
        .await
        .unwrap();
    let a = AssetRepo::new(&server.db)
        .upsert_discovered(NewAsset {
            folder_id: folder.id,
            filename: AssetName::parse(name).unwrap(),
            size_bytes: 10,
            mtime: Utc.with_ymd_and_hms(2024, 7, 1, 0, 0, 0).unwrap(),
            inode: Some(1),
            kind: AssetKind::Image,
        })
        .await
        .unwrap();
    AssetRepo::new(&server.db)
        .set_indexed(
            a.id,
            Utc.with_ymd_and_hms(2024, 7, 1, 12, 0, 0).unwrap(),
            1,
            1,
        )
        .await
        .unwrap();
}
