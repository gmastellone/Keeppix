mod harness;

use harness::TestServer;
use keeppix_db::{
    AlbumRepo, AssetRepo, FolderRepo, LibraryRepo, NewAlbum, NewShareLink, ShareLinkRepo,
};
use keeppix_domain::{
    AssetKind, AssetName, AuthContext, FolderId, NewAsset, NewLibrary, SystemRole,
};
use serde_json::json;

#[allow(clippy::expect_used, clippy::unwrap_used)]
async fn setup_admin(server: &TestServer) -> keeppix_domain::UserId {
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
    let body: serde_json::Value = response.json().await.expect("json");
    body["user"]["id"]
        .as_str()
        .expect("id")
        .parse()
        .expect("uuid")
}

#[allow(clippy::expect_used, clippy::unwrap_used)]
async fn seed_folder(server: &TestServer, admin: keeppix_domain::UserId) -> FolderId {
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let library = LibraryRepo::new(&server.db)
        .create(
            &ctx,
            NewLibrary {
                name: "Foto".to_owned(),
                owner_id: admin,
                root_path: std::path::PathBuf::from("/mnt/nas"),
                exclude_patterns: vec![],
            },
        )
        .await
        .expect("library")
        .id;
    FolderRepo::new(&server.db)
        .ensure_path(library, &["2024", "Urbino"])
        .await
        .expect("folder")
        .id
}

fn assert_no_per_row_count_fields(value: &serde_json::Value, context: &str) {
    match value {
        serde_json::Value::Array(items) => {
            for item in items {
                assert_no_per_row_count_fields(item, context);
            }
        }
        serde_json::Value::Object(map) => {
            // `item_count` is checked separately per endpoint below (it's
            // deliberately added to share links only): `asset_count` and
            // `member_count` never shipped and stay forbidden everywhere.
            for forbidden in ["asset_count", "member_count"] {
                assert!(
                    !map.contains_key(forbidden),
                    "{context} must not expose `{forbidden}`"
                );
            }
        }
        _ => {}
    }
}

fn assert_no_item_count_field(value: &serde_json::Value, context: &str) {
    match value {
        serde_json::Value::Array(items) => {
            for item in items {
                assert_no_item_count_field(item, context);
            }
        }
        serde_json::Value::Object(map) => {
            assert!(
                !map.contains_key("item_count"),
                "{context} must not expose `item_count`"
            );
        }
        _ => {}
    }
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn sidebar_endpoints_do_not_expose_per_row_counts() {
    let server = TestServer::start().await;
    let admin = setup_admin(&server).await;
    let folder = seed_folder(&server, admin).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);

    AssetRepo::new(&server.db)
        .upsert_discovered(NewAsset {
            folder_id: folder,
            filename: AssetName::parse("a.jpg").expect("name"),
            size_bytes: 10,
            mtime: chrono::Utc::now(),
            inode: None,
            kind: AssetKind::Image,
        })
        .await
        .expect("asset");

    AlbumRepo::new(&server.db)
        .create(
            &ctx,
            NewAlbum {
                name: "Album".to_owned(),
                description: String::new(),
                rule: None,
            },
        )
        .await
        .expect("album");

    ShareLinkRepo::new(&server.db)
        .create(
            &ctx,
            NewShareLink {
                object_type: "folder".to_owned(),
                object_id: folder.as_uuid(),
                password_hash: None,
                expires_at: None,
                max_views: None,
                allow_download: true,
                allow_original: false,
                allow_upload: false,
                allow_cdn_cache: false,
                hide_metadata: true,
                upload_quota_bytes: None,
            },
        )
        .await
        .expect("link");

    let folders: serde_json::Value = server
        .client
        .get(server.url("/api/v1/folders/tree?roots=true"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_no_per_row_count_fields(&folders, "GET /folders/tree");
    assert_no_item_count_field(&folders, "GET /folders/tree");

    let albums: serde_json::Value = server
        .client
        .get(server.url("/api/v1/albums"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_no_per_row_count_fields(&albums, "GET /albums");
    assert_no_item_count_field(&albums, "GET /albums");

    let links: serde_json::Value = server
        .client
        .get(server.url("/api/v1/share/links"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_no_per_row_count_fields(&links, "GET /share/links");
    assert!(
        links[0]["view_count"].is_number(),
        "view_count stays: it counts visits to the link, not shared items"
    );
    assert_eq!(
        links[0]["item_count"], 0,
        "item_count is present but zero — the only asset is discovered, not indexed"
    );
}
