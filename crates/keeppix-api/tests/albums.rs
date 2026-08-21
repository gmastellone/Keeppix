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
        .expect("richiesta di setup");
    let body: serde_json::Value = response.json().await.expect("corpo JSON");
    body["user"]["id"]
        .as_str()
        .expect("id utente")
        .parse()
        .expect("uuid valido")
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
        .expect("libreria")
        .id;
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
    filename: &str,
    kind: AssetKind,
) {
    let repo = AssetRepo::new(&server.db);
    let asset = repo
        .upsert_discovered(NewAsset {
            folder_id: folder,
            filename: AssetName::parse(filename).expect("nome"),
            size_bytes: 10,
            mtime: Utc.with_ymd_and_hms(2024, 7, 1, 0, 0, 0).unwrap(),
            inode: None,
            kind,
        })
        .await
        .expect("asset")
        .expect("nuovo asset");
    repo.set_indexed(
        asset.id,
        Utc.with_ymd_and_hms(2024, 7, 2, 12, 0, 0).unwrap(),
        1,
        1,
    )
    .await
    .expect("indexed");
}

/// L'esito di `refresh` è l'involucro di riuscita parziale del Task 1: le
/// foto entrate nell'album finiscono in `succeeded`.
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
        "solo la foto deve entrare, non il video"
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

/// Un album senza `rule` non è aggiornabile: `400`, non `403`/`500`.
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

/// Un utente senza permesso sull'album riceve `403`, mai `404`: niente
/// oracolo di esistenza, nemmeno sul refresh.
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
