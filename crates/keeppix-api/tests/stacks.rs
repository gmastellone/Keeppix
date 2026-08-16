mod harness;

use std::fs;
use std::path::PathBuf;

use chrono::{TimeZone, Utc};
use harness::TestServer;
use keeppix_db::{AssetRepo, FolderRepo, LibraryRepo, StackRepo};
use keeppix_domain::{
    AssetId, AssetKind, AssetName, AuthContext, NewAsset, NewLibrary, SystemRole, UserId,
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
fn temp_root(server: &TestServer, name: &str) -> PathBuf {
    let root = server.photos_root.join(name);
    fs::create_dir_all(&root).expect("radice");
    root
}

#[allow(clippy::expect_used)]
async fn seed_stacked_pair(
    server: &TestServer,
    admin: UserId,
    root: &std::path::Path,
) -> (AssetId, AssetId) {
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let library = LibraryRepo::new(&server.db)
        .create(
            &ctx,
            NewLibrary {
                name: "StackLib".to_owned(),
                owner_id: admin,
                root_path: root.to_path_buf(),
                exclude_patterns: vec![],
            },
        )
        .await
        .expect("libreria")
        .id;
    let folder = FolderRepo::new(&server.db)
        .ensure_path(library, &["2024"])
        .await
        .expect("cartella")
        .id;
    fs::create_dir_all(root.join("2024")).expect("cartella su disco");

    let raw = AssetRepo::new(&server.db)
        .upsert_discovered(NewAsset {
            folder_id: folder,
            filename: AssetName::parse("DSC_0042.ARW").expect("nome"),
            size_bytes: 1000,
            mtime: Utc.with_ymd_and_hms(2024, 6, 1, 12, 0, 0).unwrap(),
            inode: None,
            kind: AssetKind::RawImage,
        })
        .await
        .expect("raw")
        .id;
    let jpeg = AssetRepo::new(&server.db)
        .upsert_discovered(NewAsset {
            folder_id: folder,
            filename: AssetName::parse("DSC_0042.JPG").expect("nome"),
            size_bytes: 500,
            mtime: Utc.with_ymd_and_hms(2024, 6, 1, 12, 0, 0).unwrap(),
            inode: None,
            kind: AssetKind::Image,
        })
        .await
        .expect("jpeg")
        .id;

    StackRepo::new(&server.db)
        .regroup_folder(folder)
        .await
        .expect("stack");

    (raw, jpeg)
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn stack_get_returns_all_members_with_primary_marked() {
    let server = TestServer::start().await;
    let admin = setup_admin(&server).await;
    let root = temp_root(&server, "members");
    let (raw, jpeg) = seed_stacked_pair(&server, admin, &root).await;

    let response = server
        .client
        .get(server.url(&format!("/api/v1/assets/{jpeg}/stack")))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    let members = body["members"].as_array().unwrap();
    assert_eq!(members.len(), 2);
    assert_eq!(body["primary_asset_id"].as_str().unwrap(), raw.to_string());
    let primary_flags: Vec<bool> = members
        .iter()
        .map(|m| m["is_primary"].as_bool().unwrap())
        .collect();
    assert_eq!(primary_flags.iter().filter(|&&p| p).count(), 1);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn stack_set_primary_promotes_the_chosen_member() {
    let server = TestServer::start().await;
    let admin = setup_admin(&server).await;
    let root = temp_root(&server, "primary");
    let (_raw, jpeg) = seed_stacked_pair(&server, admin, &root).await;

    let response = server
        .client
        .post(server.url(&format!("/api/v1/assets/{jpeg}/stack/primary")))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 204);

    let body = server
        .client
        .get(server.url(&format!("/api/v1/assets/{jpeg}/stack")))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    assert_eq!(body["primary_asset_id"].as_str().unwrap(), jpeg.to_string());
}
