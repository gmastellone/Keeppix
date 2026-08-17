mod harness;

use chrono::{TimeZone, Timelike, Utc};
use harness::{TestServer, client_headers};
use keeppix_db::{AssetRepo, FolderRepo, LibraryRepo, UserRepo};
use keeppix_domain::{
    AssetKind, AssetName, AuthContext, FolderId, NewAsset, NewLibrary, NewUser, Password,
    SystemRole, Username, hash_password,
};
use serde_json::json;

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn timeline_buckets_require_auth() {
    let server = TestServer::start().await;
    let response = server
        .client
        .get(server.url("/api/v1/timeline/buckets"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 401);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["type"], "keeppix/unauthenticated");
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn buckets_return_month_counts_for_indexed_assets() {
    let server = TestServer::start().await;
    let (folder, _) = seed_library(&server).await;
    index_photo(&server, folder, "a.jpg", 2024, 7, 2).await;
    index_photo(&server, folder, "b.jpg", 2024, 8, 3).await;

    let response = server
        .client
        .get(server.url("/api/v1/timeline/buckets"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body.as_array().unwrap().len(), 2);
    assert_eq!(body[0]["month"], "2024-08");
    assert_eq!(body[0]["count"], 1);
    assert_eq!(body[1]["month"], "2024-07");
    assert_eq!(body[1]["count"], 1);
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn timeline_page_uses_keyset_cursor() {
    let server = TestServer::start().await;
    let (folder, _) = seed_library(&server).await;
    index_photo(&server, folder, "c.jpg", 2024, 7, 10).await;
    index_photo(&server, folder, "d.jpg", 2024, 7, 9).await;
    index_photo(&server, folder, "e.jpg", 2024, 7, 8).await;

    let first = server
        .client
        .get(server.url("/api/v1/timeline?bucket=2024-07&limit=2"))
        .send()
        .await
        .unwrap();
    assert_eq!(first.status(), 200);
    let page: serde_json::Value = first.json().await.unwrap();
    let items = page["assets"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    let cursor = page["next_cursor"].as_str().unwrap();

    let second = server
        .client
        .get(server.url(&format!(
            "/api/v1/timeline?bucket=2024-07&limit=2&cursor={cursor}"
        )))
        .send()
        .await
        .unwrap();
    assert_eq!(second.status(), 200);
    let next: serde_json::Value = second.json().await.unwrap();
    let rest = next["assets"].as_array().unwrap();
    assert_eq!(rest.len(), 1);
    assert_ne!(rest[0]["id"], items[0]["id"]);
    assert_ne!(rest[0]["id"], items[1]["id"]);
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn timeline_keyset_keeps_assets_that_share_a_truncated_second() {
    let server = TestServer::start().await;
    let (folder, _) = seed_library(&server).await;
    let base = Utc.with_ymd_and_hms(2024, 7, 10, 12, 0, 0).unwrap();
    index_photo_at(
        &server,
        folder,
        "late.jpg",
        base.with_nanosecond(500_000_000).unwrap(),
    )
    .await;
    index_photo_at(
        &server,
        folder,
        "mid.jpg",
        base.with_nanosecond(400_000_000).unwrap(),
    )
    .await;
    index_photo_at(
        &server,
        folder,
        "early.jpg",
        base.with_nanosecond(300_000_000).unwrap(),
    )
    .await;

    let first = server
        .client
        .get(server.url("/api/v1/timeline?bucket=2024-07&limit=2"))
        .send()
        .await
        .unwrap();
    assert_eq!(first.status(), 200);
    let page: serde_json::Value = first.json().await.unwrap();
    let first_ids: Vec<&str> = page["assets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["id"].as_str().unwrap())
        .collect();
    assert_eq!(first_ids.len(), 2);
    let cursor = page["next_cursor"].as_str().unwrap();

    let second = server
        .client
        .get(server.url(&format!(
            "/api/v1/timeline?bucket=2024-07&limit=2&cursor={cursor}"
        )))
        .send()
        .await
        .unwrap();
    assert_eq!(second.status(), 200);
    let next: serde_json::Value = second.json().await.unwrap();
    let rest: Vec<&str> = next["assets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        rest.len(),
        1,
        "sub-second timestamps must not vanish on page 2"
    );
    assert!(!first_ids.contains(&rest[0]));
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn folder_tree_lists_visible_folders() {
    let server = TestServer::start().await;
    let (root, _) = seed_library(&server).await;
    let child = FolderRepo::new(&server.db)
        .ensure_path(
            LibraryRepo::new(&server.db)
                .list(&admin_ctx(&server).await)
                .await
                .unwrap()[0]
                .id,
            &["Vacanze"],
        )
        .await
        .unwrap();

    let response = server
        .client
        .get(server.url("/api/v1/folders/tree"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let tree: serde_json::Value = response.json().await.unwrap();
    let folders = tree.as_array().unwrap();
    assert_eq!(folders.len(), 2);
    let ids: Vec<&str> = folders.iter().map(|f| f["id"].as_str().unwrap()).collect();
    assert!(ids.contains(&root.to_string().as_str()));
    assert!(ids.contains(&child.id.to_string().as_str()));
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn folder_children_include_direct_folders_and_assets() {
    let server = TestServer::start().await;
    let (root, library) = seed_library(&server).await;
    FolderRepo::new(&server.db)
        .ensure_path(library, &["Album"])
        .await
        .unwrap();
    index_photo(&server, root, "cover.jpg", 2024, 7, 1).await;

    let response = server
        .client
        .get(server.url(&format!("/api/v1/folders/{root}/children")))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["folders"].as_array().unwrap().len(), 1);
    assert_eq!(body["folders"][0]["name"], "Album");
    assert_eq!(body["assets"].as_array().unwrap().len(), 1);
    assert_eq!(body["assets"][0]["filename"], "cover.jpg");
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn probing_someone_elses_folder_is_forbidden() {
    let server = TestServer::start().await;
    let (folder, _) = seed_library(&server).await;
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

    let response = client
        .get(server.url(&format!("/api/v1/folders/{folder}/children")))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 403);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["type"], "keeppix/forbidden");
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn probing_someone_elses_library_buckets_is_forbidden() {
    let server = TestServer::start().await;
    let (_, library) = seed_library(&server).await;
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

    let response = client
        .get(server.url(&format!("/api/v1/timeline/buckets?library={library}")))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 403);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["type"], "keeppix/forbidden");
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

#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn admin_ctx(server: &TestServer) -> AuthContext {
    let username = Username::parse("giovanni").unwrap();
    let (user, _) = UserRepo::new(&server.db)
        .find_by_username(&username)
        .await
        .unwrap()
        .expect("admin");
    AuthContext::user(user.id, SystemRole::Admin)
}

#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn seed_library(server: &TestServer) -> (FolderId, keeppix_domain::LibraryId) {
    setup(server).await;
    let ctx = admin_ctx(server).await;
    let library = LibraryRepo::new(&server.db)
        .create(
            &ctx,
            NewLibrary {
                name: "Foto".to_owned(),
                owner_id: ctx.user_id().unwrap(),
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
    (folder.id, library.id)
}

#[allow(clippy::unwrap_used)]
async fn index_photo(server: &TestServer, folder: FolderId, name: &str, y: i32, m: u32, d: u32) {
    index_photo_at(
        server,
        folder,
        name,
        Utc.with_ymd_and_hms(y, m, d, 12, 0, 0).unwrap(),
    )
    .await;
}

#[allow(clippy::unwrap_used)]
async fn index_photo_at(
    server: &TestServer,
    folder: FolderId,
    name: &str,
    taken: chrono::DateTime<Utc>,
) {
    let assets = AssetRepo::new(&server.db);
    let a = assets
        .upsert_discovered(NewAsset {
            folder_id: folder,
            filename: AssetName::parse(name).unwrap(),
            size_bytes: 10,
            mtime: taken,
            inode: Some(1),
            kind: AssetKind::Image,
        })
        .await
        .unwrap()
        .unwrap();
    assets.set_indexed(a.id, taken, 1, 1).await.unwrap();
}

#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn seed_user(server: &TestServer, username: &str) {
    let password = Password::parse("correct horse battery staple").unwrap();
    UserRepo::new(&server.db)
        .create(
            &admin_ctx(server).await,
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
