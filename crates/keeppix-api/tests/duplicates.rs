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
async fn seed_asset_with_hash(
    server: &TestServer,
    folder: FolderId,
    root: &std::path::Path,
    filename: &str,
    hash: [u8; 32],
) -> AssetId {
    fs::write(root.join("2024").join(filename), b"content").expect("file on disk");

    let repo = AssetRepo::new(&server.db);
    let asset = repo
        .upsert_discovered(NewAsset {
            folder_id: folder,
            filename: AssetName::parse(filename).expect("name"),
            size_bytes: 1_000_000,
            mtime: Utc.with_ymd_and_hms(2024, 6, 1, 12, 0, 0).unwrap(),
            inode: None,
            kind: AssetKind::Image,
        })
        .await
        .expect("asset")
        .unwrap()
        .id;
    repo.set_hash(asset, hash).await.expect("hash");
    asset
}

#[allow(clippy::expect_used, clippy::unwrap_used)]
fn temp_root() -> PathBuf {
    let root =
        std::env::temp_dir().join(format!("keeppix-api-duplicates-{}", uuid::Uuid::now_v7()));
    fs::create_dir_all(&root).expect("test root");
    root
}

fn hex(hash: &[u8; 32]) -> String {
    hash.iter().fold(String::new(), |mut out, byte| {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
        out
    })
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn a_group_with_two_copies_lists_members_and_reclaimable_space() {
    let server = TestServer::start().await;
    let admin = setup_admin(&server).await;
    let root = temp_root();
    let folder = ensure_folder(&server, admin, &root).await;
    let hash = [7u8; 32];
    let a = seed_asset_with_hash(&server, folder, &root, "a.jpg", hash).await;
    let b = seed_asset_with_hash(&server, folder, &root, "b.jpg", hash).await;

    let groups: serde_json::Value = server
        .client
        .get(server.url("/api/v1/duplicates"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let group = groups
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["content_hash"] == hex(&hash))
        .expect("group present");
    assert_eq!(group["count"], 2);
    assert_eq!(
        group["reclaimable_bytes"], 1_000_000,
        "size_bytes * (copies - 1), not the total sum"
    );

    let members: serde_json::Value = server
        .client
        .get(server.url(&format!("/api/v1/duplicates/{}", hex(&hash))))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let ids: Vec<String> = members
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_str().unwrap().to_owned())
        .collect();
    assert!(ids.contains(&a.to_string()));
    assert!(ids.contains(&b.to_string()));

    let _ = fs::remove_dir_all(&root);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn resolving_a_group_trashes_every_member_except_the_one_kept() {
    let server = TestServer::start().await;
    let admin = setup_admin(&server).await;
    let root = temp_root();
    let folder = ensure_folder(&server, admin, &root).await;
    let hash = [9u8; 32];
    let a = seed_asset_with_hash(&server, folder, &root, "a.jpg", hash).await;
    let b = seed_asset_with_hash(&server, folder, &root, "b.jpg", hash).await;

    let response = server
        .client
        .post(server.url(&format!("/api/v1/duplicates/{}/resolve", hex(&hash))))
        .json(&json!({ "keep": a.to_string(), "disk_action": "moved_to_trash" }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["resolved"], 1);

    // Only one member remains visible as a duplicate: `b` is trashed.
    let members: serde_json::Value = server
        .client
        .get(server.url(&format!("/api/v1/duplicates/{}", hex(&hash))))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let ids: Vec<String> = members
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(ids, vec![a.to_string()]);
    let _ = b;

    let _ = fs::remove_dir_all(&root);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn resolving_with_a_keep_id_outside_the_group_is_forbidden() {
    let server = TestServer::start().await;
    let admin = setup_admin(&server).await;
    let root = temp_root();
    let folder = ensure_folder(&server, admin, &root).await;
    let hash = [11u8; 32];
    seed_asset_with_hash(&server, folder, &root, "a.jpg", hash).await;
    seed_asset_with_hash(&server, folder, &root, "b.jpg", hash).await;
    let stranger = seed_asset_with_hash(&server, folder, &root, "c.jpg", [12u8; 32]).await;

    let response = server
        .client
        .post(server.url(&format!("/api/v1/duplicates/{}/resolve", hex(&hash))))
        .json(&json!({ "keep": stranger.to_string(), "disk_action": "kept" }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 403);

    let _ = fs::remove_dir_all(&root);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn an_invalid_content_hash_is_a_bad_request() {
    let server = TestServer::start().await;
    setup_admin(&server).await;

    let response = server
        .client
        .get(server.url("/api/v1/duplicates/not-hex"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 400);
    let problem: serde_json::Value = response.json().await.unwrap();
    assert_eq!(problem["type"], "keeppix/invalid-content-hash");
}
