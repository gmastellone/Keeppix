mod harness;

use chrono::{TimeZone, Utc};
use harness::{TestServer, client_headers};
use keeppix_db::{AssetRepo, FolderRepo, LibraryRepo, UserRepo};
use keeppix_domain::{
    AssetKind, AssetName, AuthContext, FolderId, NewAsset, NewLibrary, NewUser, Password,
    SystemRole, Username, hash_password,
};
use keeppix_media::derivative_paths;
use serde_json::json;

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn unknown_thumb_hash_is_forbidden_not_an_oracle() {
    let server = TestServer::start().await;
    setup(&server).await;
    let hash = "0".repeat(64);
    let response = server
        .client
        .get(server.url(&format!("/media/thumb/{hash}")))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 403);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["type"], "keeppix/forbidden");
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn existing_thumb_is_immutable() {
    let server = TestServer::start().await;
    let (folder, _) = seed_library(&server).await;
    let hash = [0xab_u8; 32];
    let asset = AssetRepo::new(&server.db)
        .upsert_discovered(photo(folder, "a.jpg"))
        .await
        .unwrap()
        .unwrap();
    AssetRepo::new(&server.db)
        .set_hash(asset.id, hash)
        .await
        .unwrap();
    let (thumb, _) = derivative_paths(&server.data_dir, &hash);
    std::fs::create_dir_all(thumb.parent().unwrap()).unwrap();
    std::fs::write(&thumb, b"RIFFWEBP").unwrap();

    let hex = "ab".repeat(32);
    let response = server
        .client
        .get(server.url(&format!("/media/thumb/{hex}")))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(
        response.headers().get("cache-control").unwrap(),
        "private, max-age=31536000, immutable"
    );
    assert_eq!(
        response.headers().get("content-type").unwrap(),
        "image/webp"
    );
    assert_eq!(response.bytes().await.unwrap().as_ref(), b"RIFFWEBP");
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn probing_someone_elses_existing_thumb_is_forbidden() {
    let server = TestServer::start().await;
    let (folder, _) = seed_library(&server).await;
    let hash = [0xcd_u8; 32];
    let asset = AssetRepo::new(&server.db)
        .upsert_discovered(photo(folder, "secret.jpg"))
        .await
        .unwrap()
        .unwrap();
    AssetRepo::new(&server.db)
        .set_hash(asset.id, hash)
        .await
        .unwrap();
    let (thumb, _) = derivative_paths(&server.data_dir, &hash);
    std::fs::create_dir_all(thumb.parent().unwrap()).unwrap();
    std::fs::write(&thumb, b"RIFFWEBP").unwrap();
    seed_user(&server, "luca").await;

    let client = reqwest::Client::builder()
        .cookie_store(true)
        .default_headers(client_headers())
        .build()
        .unwrap();
    let login = client
        .post(server.url("/api/v1/auth/login"))
        .json(&json!({
            "username": "luca",
            "password": "correct horse battery staple"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(login.status(), 200);

    let hex = "cd".repeat(32);
    let response = client
        .get(server.url(&format!("/media/thumb/{hex}")))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 403);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["type"], "keeppix/forbidden");

    let original = client
        .get(server.url(&format!("/media/original/{}", asset.id)))
        .send()
        .await
        .unwrap();
    assert_eq!(original.status(), 403);
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn original_serves_a_byte_range_by_id() {
    let server = TestServer::start().await;
    let (folder, root) = seed_library(&server).await;
    let path = root.join("orig.jpg");
    std::fs::write(&path, b"0123456789").unwrap();
    let asset = AssetRepo::new(&server.db)
        .upsert_discovered(photo(folder, "orig.jpg"))
        .await
        .unwrap()
        .unwrap();

    let response = server
        .client
        .get(server.url(&format!("/media/original/{}", asset.id)))
        .header("range", "bytes=2-5")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 206);
    assert_eq!(response.headers().get("accept-ranges").unwrap(), "bytes");
    assert_eq!(response.bytes().await.unwrap().as_ref(), b"2345");
}

#[allow(clippy::unwrap_used, clippy::expect_used)]
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

#[allow(clippy::unwrap_used)]
fn photo(folder: FolderId, name: &str) -> NewAsset {
    NewAsset {
        folder_id: folder,
        filename: AssetName::parse(name).unwrap(),
        size_bytes: 10,
        mtime: Utc.with_ymd_and_hms(2024, 7, 1, 0, 0, 0).unwrap(),
        inode: Some(1),
        kind: AssetKind::Image,
    }
}

#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn seed_library(server: &TestServer) -> (FolderId, std::path::PathBuf) {
    setup(server).await;
    let username = Username::parse("giovanni").unwrap();
    let (user, _) = UserRepo::new(&server.db)
        .find_by_username(&username)
        .await
        .unwrap()
        .expect("admin");
    let ctx = AuthContext::user(user.id, SystemRole::Admin);
    let root = server.data_dir.join("library");
    std::fs::create_dir_all(&root).unwrap();
    let library = LibraryRepo::new(&server.db)
        .create(
            &ctx,
            NewLibrary {
                name: "Foto".to_owned(),
                owner_id: user.id,
                root_path: root.clone(),
                exclude_patterns: vec![],
            },
        )
        .await
        .unwrap();
    let folder = FolderRepo::new(&server.db)
        .ensure_path(library.id, &[])
        .await
        .unwrap();
    (folder.id, root)
}

#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn seed_user(server: &TestServer, username: &str) {
    let username_parsed = Username::parse("giovanni").unwrap();
    let (admin, _) = UserRepo::new(&server.db)
        .find_by_username(&username_parsed)
        .await
        .unwrap()
        .expect("admin");
    let ctx = AuthContext::user(admin.id, SystemRole::Admin);
    let password = Password::parse("correct horse battery staple").unwrap();
    UserRepo::new(&server.db)
        .create(
            &ctx,
            NewUser {
                username: Username::parse(username).unwrap(),
                email: None,
                display_name: username.to_owned(),
                password_hash: hash_password(&password).unwrap().as_str().to_owned(),
                role: SystemRole::User,
            },
        )
        .await
        .unwrap();
}
