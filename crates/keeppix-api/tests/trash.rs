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
        .expect("richiesta di setup");
    let body: serde_json::Value = response.json().await.expect("corpo JSON");
    body["user"]["id"]
        .as_str()
        .expect("id utente")
        .parse()
        .expect("uuid valido")
}

#[allow(clippy::expect_used)]
async fn seed_asset(server: &TestServer, admin: UserId, root: &std::path::Path) -> AssetId {
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
    let folder: FolderId = FolderRepo::new(&server.db)
        .ensure_path(library, &["2024"])
        .await
        .expect("cartella")
        .id;
    fs::create_dir_all(root.join("2024")).expect("cartella su disco");
    fs::write(root.join("2024").join("foto.jpg"), b"contenuto").expect("file su disco");

    AssetRepo::new(&server.db)
        .upsert_discovered(NewAsset {
            folder_id: folder,
            filename: AssetName::parse("foto.jpg").expect("nome"),
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
fn temp_root() -> PathBuf {
    let root = std::env::temp_dir().join(format!("keeppix-api-trash-{}", uuid::Uuid::now_v7()));
    fs::create_dir_all(&root).expect("radice di test");
    root
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn deleting_to_trash_then_restoring_round_trips_the_file() {
    let server = TestServer::start().await;
    let admin = setup_admin(&server).await;
    let root = temp_root();
    let asset_id = seed_asset(&server, admin, &root).await;
    let original = root.join("2024").join("foto.jpg");

    let response = server
        .client
        .delete(server.url(&format!("/api/v1/assets/{asset_id}")))
        .json(&json!({ "disk_action": "moved_to_trash" }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 204);
    assert!(!original.exists(), "il file è passato nel cestino");

    let response = server
        .client
        .post(server.url(&format!("/api/v1/assets/{asset_id}/restore")))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 204);
    assert!(original.is_file(), "il file torna al percorso originale");

    let _ = fs::remove_dir_all(&root);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn an_unrecognised_disk_action_is_a_bad_request() {
    let server = TestServer::start().await;
    let admin = setup_admin(&server).await;
    let root = temp_root();
    let asset_id = seed_asset(&server, admin, &root).await;

    let response = server
        .client
        .delete(server.url(&format!("/api/v1/assets/{asset_id}")))
        .json(&json!({ "disk_action": "delete_forever_please" }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 400);
    let problem: serde_json::Value = response.json().await.unwrap();
    assert_eq!(problem["type"], "keeppix/invalid-disk-action");

    let _ = fs::remove_dir_all(&root);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn probing_someone_elses_asset_is_forbidden_not_found() {
    let server = TestServer::start().await;
    let admin = setup_admin(&server).await;
    let root = temp_root();
    let asset_id = seed_asset(&server, admin, &root).await;

    // Un secondo utente, autenticato ma senza alcuna libreria propria: non
    // vede l'asset dell'admin.
    keeppix_db::UserRepo::new(&server.db)
        .create(
            &AuthContext::user(admin, SystemRole::Admin),
            keeppix_domain::NewUser {
                username: keeppix_domain::Username::parse("mario").unwrap(),
                email: None,
                display_name: "Mario".to_owned(),
                password_hash: keeppix_domain::hash_password(
                    &keeppix_domain::Password::parse("correct horse battery staple").unwrap(),
                )
                .unwrap()
                .as_str()
                .to_owned(),
                role: SystemRole::User,
            },
        )
        .await
        .unwrap();

    // `server.client` porta un cookie store: accedere come mario sostituisce
    // il cookie di sessione dell'admin con il suo, sullo stesso client.
    let login = server
        .client
        .post(server.url("/api/v1/auth/login"))
        .json(&json!({ "username": "mario", "password": "correct horse battery staple" }))
        .send()
        .await
        .unwrap();
    assert_eq!(login.status(), 200);

    let response = server
        .client
        .delete(server.url(&format!("/api/v1/assets/{asset_id}")))
        .json(&json!({ "disk_action": "kept" }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 403);

    let _ = fs::remove_dir_all(&root);
}
