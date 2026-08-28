mod harness;
mod journey;

use chrono::{TimeZone, Utc};
use harness::TestServer;
use journey::{create_user, login_as};
use keeppix_db::{AssetRepo, FolderRepo, LibraryRepo};
use keeppix_domain::{
    AssetKind, AssetName, AuthContext, FolderId, NewAsset, NewLibrary, SystemRole, UserId,
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
async fn seed_folder(server: &TestServer, admin: UserId) -> FolderId {
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
        .ensure_path(library, &["2024"])
        .await
        .expect("folder")
        .id
}

#[allow(clippy::expect_used, clippy::unwrap_used)]
async fn seed_indexed_asset(
    server: &TestServer,
    folder: FolderId,
    filename: &str,
    kind: AssetKind,
) {
    let repo = AssetRepo::new(&server.db);
    let asset = repo
        .upsert_discovered(NewAsset {
            folder_id: folder,
            filename: AssetName::parse(filename).expect("name"),
            size_bytes: 10,
            mtime: Utc.with_ymd_and_hms(2024, 7, 1, 0, 0, 0).unwrap(),
            inode: None,
            kind,
        })
        .await
        .expect("asset")
        .expect("new asset");
    repo.set_indexed(
        asset.id,
        Utc.with_ymd_and_hms(2024, 7, 2, 12, 0, 0).unwrap(),
        1,
        1,
    )
    .await
    .expect("indexed");
}

/// The result of `refresh` wraps partial success: photos added to the
/// album end up in `succeeded`.
#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn refresh_returns_added_ids_as_succeeded_bulk_outcome() {
    let server = TestServer::start().await;
    let admin = setup_admin(&server).await;
    let folder = seed_folder(&server, admin).await;
    seed_indexed_asset(&server, folder, "a.jpg", AssetKind::Image).await;
    seed_indexed_asset(&server, folder, "b.mov", AssetKind::Video).await;

    let created: serde_json::Value = server
        .client
        .post(server.url("/api/v1/albums"))
        .json(&json!({ "name": "Solo foto", "rule": { "op": "type", "value": "image" } }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let album_id = created["id"].as_str().unwrap();

    let response = server
        .client
        .post(server.url(&format!("/api/v1/albums/{album_id}/refresh")))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    let succeeded = body["succeeded"].as_array().unwrap();
    assert_eq!(
        succeeded.len(),
        1,
        "only the photo should be included, not the video"
    );
    assert!(body["failed"].as_array().unwrap().is_empty());

    let members: serde_json::Value = server
        .client
        .get(server.url(&format!("/api/v1/albums/{album_id}/assets")))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(members.as_array().unwrap().len(), 1);
}

/// An album without a `rule` cannot be refreshed: `400`, not `403`/`500`.
#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn refresh_without_a_rule_is_a_bad_request() {
    let server = TestServer::start().await;
    setup_admin(&server).await;

    let created: serde_json::Value = server
        .client
        .post(server.url("/api/v1/albums"))
        .json(&json!({ "name": "Manuale" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let album_id = created["id"].as_str().unwrap();

    let response = server
        .client
        .post(server.url(&format!("/api/v1/albums/{album_id}/refresh")))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 400);
    let problem: serde_json::Value = response.json().await.unwrap();
    assert_eq!(problem["type"], "keeppix/album-has-no-rule");
}

/// A user without permission on the album receives `403`, never `404`: no
/// existence oracle, not even on refresh.
#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn refresh_on_a_foreign_album_is_forbidden() {
    let server = TestServer::start().await;
    setup_admin(&server).await;
    create_user(&server, "mario", "mario-password-ok").await;
    let mario = login_as(&server, "mario", "mario-password-ok").await;

    let created: serde_json::Value = server
        .client
        .post(server.url("/api/v1/albums"))
        .json(&json!({ "name": "Privato", "rule": { "op": "type", "value": "image" } }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let album_id = created["id"].as_str().unwrap();

    let response = mario
        .post(server.url(&format!("/api/v1/albums/{album_id}/refresh")))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 403);
}

/// The reverse direction of `GET /albums/{id}/assets` — verified
/// end-to-end via HTTP; the visibility logic itself is already covered by
/// the `AlbumRepo::for_asset` tests in `keeppix-db`.
#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn list_for_asset_returns_only_the_albums_the_asset_belongs_to() {
    let server = TestServer::start().await;
    let admin = setup_admin(&server).await;
    let folder = seed_folder(&server, admin).await;
    seed_indexed_asset(&server, folder, "a.jpg", AssetKind::Image).await;
    let asset_id: uuid::Uuid = sqlx::query_scalar("SELECT id FROM assets WHERE filename = 'a.jpg'")
        .fetch_one(server.db.pool())
        .await
        .unwrap();

    let in_album: serde_json::Value = server
        .client
        .post(server.url("/api/v1/albums"))
        .json(&json!({ "name": "Vacanze" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let in_album_id = in_album["id"].as_str().unwrap();
    server
        .client
        .post(server.url("/api/v1/albums"))
        .json(&json!({ "name": "Non membro" }))
        .send()
        .await
        .unwrap();

    server
        .client
        .post(server.url(&format!("/api/v1/albums/{in_album_id}/assets/{asset_id}")))
        .send()
        .await
        .unwrap();

    let response = server
        .client
        .get(server.url(&format!("/api/v1/assets/{asset_id}/albums")))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    let albums = body.as_array().unwrap();
    assert_eq!(albums.len(), 1);
    assert_eq!(albums[0]["name"], "Vacanze");
}
