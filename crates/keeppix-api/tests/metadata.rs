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
        .expect("richiesta di setup");
    let body: serde_json::Value = response.json().await.expect("corpo JSON");
    body["user"]["id"]
        .as_str()
        .expect("id utente")
        .parse()
        .expect("uuid valido")
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
        .expect("libreria")
        .id;
    fs::create_dir_all(root.join("2024")).expect("cartella su disco");
    FolderRepo::new(&server.db)
        .ensure_path(library, &["2024"])
        .await
        .expect("cartella")
        .id
}

/// Un solo asset dentro una cartella già preparata da [`ensure_folder`]:
/// separato per poter seminare più asset sotto la stessa radice senza
/// ricreare la libreria (un secondo `create` sulla stessa `root_path`
/// fallirebbe con `Conflict`).
#[allow(clippy::expect_used, clippy::unwrap_used)]
async fn seed_indexed_asset(
    server: &TestServer,
    folder: FolderId,
    root: &std::path::Path,
    filename: &str,
) -> AssetId {
    fs::write(root.join("2024").join(filename), b"contenuto").expect("file su disco");

    let repo = AssetRepo::new(&server.db);
    let asset = repo
        .upsert_discovered(NewAsset {
            folder_id: folder,
            filename: AssetName::parse(filename).expect("nome"),
            size_bytes: 9,
            mtime: Utc.with_ymd_and_hms(2024, 6, 1, 12, 0, 0).unwrap(),
            inode: None,
            kind: AssetKind::Image,
        })
        .await
        .expect("asset")
        .unwrap()
        .id;
    repo.set_indexed(
        asset,
        Utc.with_ymd_and_hms(2024, 6, 1, 10, 0, 0).unwrap(),
        100,
        100,
    )
    .await
    .expect("indicizzazione");
    asset
}

#[allow(clippy::expect_used, clippy::unwrap_used)]
fn temp_root() -> PathBuf {
    let root = std::env::temp_dir().join(format!("keeppix-api-metadata-{}", uuid::Uuid::now_v7()));
    fs::create_dir_all(&root).expect("radice di test");
    root
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn a_partial_patch_only_touches_the_fields_it_names() {
    let server = TestServer::start().await;
    let admin = setup_admin(&server).await;
    let root = temp_root();
    let folder = ensure_folder(&server, admin, &root).await;
    let asset_id = seed_indexed_asset(&server, folder, &root, "foto.jpg").await;

    let response = server
        .client
        .get(server.url(&format!("/api/v1/assets/{asset_id}/metadata")))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let before: serde_json::Value = response.json().await.unwrap();
    assert_eq!(before["title"], serde_json::Value::Null);
    assert!(before["taken_at"].is_string(), "viene dall'exif");

    let response = server
        .client
        .patch(server.url(&format!("/api/v1/assets/{asset_id}/metadata")))
        .json(&json!({ "title": "Alba sul mare" }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 204);

    let after: serde_json::Value = server
        .client
        .get(server.url(&format!("/api/v1/assets/{asset_id}/metadata")))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(after["title"], "Alba sul mare");
    assert_eq!(
        after["taken_at"], before["taken_at"],
        "un campo non nominato nel patch non deve cambiare"
    );

    let _ = fs::remove_dir_all(&root);
}

/// Pin sulla distinzione fra "campo assente" e "campo `null`" (§3.2): un
/// client manda `null` per azzerare esplicitamente un campo già valorizzato,
/// non per lasciarlo stare. Senza `double_option`, `Option<Option<T>>`
/// tratterebbe i due casi allo stesso modo e questo test fallirebbe.
#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn an_explicit_null_erases_a_field_but_an_absent_key_does_not() {
    let server = TestServer::start().await;
    let admin = setup_admin(&server).await;
    let root = temp_root();
    let folder = ensure_folder(&server, admin, &root).await;
    let asset_id = seed_indexed_asset(&server, folder, &root, "foto.jpg").await;

    server
        .client
        .patch(server.url(&format!("/api/v1/assets/{asset_id}/metadata")))
        .json(&json!({ "title": "Temporaneo", "description": "Nota" }))
        .send()
        .await
        .unwrap();

    // Assente: non deve toccare description.
    server
        .client
        .patch(server.url(&format!("/api/v1/assets/{asset_id}/metadata")))
        .json(&json!({ "title": "Titolo definitivo" }))
        .send()
        .await
        .unwrap();
    let mid: serde_json::Value = server
        .client
        .get(server.url(&format!("/api/v1/assets/{asset_id}/metadata")))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        mid["description"], "Nota",
        "description assente non toccata"
    );

    // Presente con null: deve azzerare.
    server
        .client
        .patch(server.url(&format!("/api/v1/assets/{asset_id}/metadata")))
        .json(&json!({ "description": null }))
        .send()
        .await
        .unwrap();
    let after: serde_json::Value = server
        .client
        .get(server.url(&format!("/api/v1/assets/{asset_id}/metadata")))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        after["description"],
        serde_json::Value::Null,
        "null esplicito azzera il campo"
    );
    assert_eq!(
        after["title"], "Titolo definitivo",
        "il campo non nominato nell'ultima richiesta resta"
    );

    let _ = fs::remove_dir_all(&root);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn apply_batch_touches_every_asset_and_is_undoable() {
    let server = TestServer::start().await;
    let admin = setup_admin(&server).await;
    let root = temp_root();
    let folder = ensure_folder(&server, admin, &root).await;
    let a = seed_indexed_asset(&server, folder, &root, "a.jpg").await;
    let b = seed_indexed_asset(&server, folder, &root, "b.jpg").await;

    let response = server
        .client
        .post(server.url("/api/v1/metadata/batch"))
        .json(&json!({
            "asset_ids": [a.to_string(), b.to_string()],
            "patch": { "description": "Matrimonio Rossi" }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    let batch_id = body["batch_id"].as_str().expect("batch_id").to_owned();

    for asset in [a, b] {
        let effective: serde_json::Value = server
            .client
            .get(server.url(&format!("/api/v1/assets/{asset}/metadata")))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(effective["description"], "Matrimonio Rossi");
    }

    let response = server
        .client
        .post(server.url(&format!("/api/v1/metadata/batch/{batch_id}/undo")))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 204);

    let restored: serde_json::Value = server
        .client
        .get(server.url(&format!("/api/v1/assets/{a}/metadata")))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(restored["description"], serde_json::Value::Null);

    let _ = fs::remove_dir_all(&root);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn shift_taken_at_moves_the_effective_date_by_n_hours() {
    let server = TestServer::start().await;
    let admin = setup_admin(&server).await;
    let root = temp_root();
    let folder = ensure_folder(&server, admin, &root).await;
    let asset_id = seed_indexed_asset(&server, folder, &root, "foto.jpg").await;

    let before: serde_json::Value = server
        .client
        .get(server.url(&format!("/api/v1/assets/{asset_id}/metadata")))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let before_taken_at = before["taken_at"].as_str().unwrap().to_owned();

    let response = server
        .client
        .post(server.url("/api/v1/metadata/batch/shift-taken-at"))
        .json(&json!({ "asset_ids": [asset_id.to_string()], "hours": -3 }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    let after: serde_json::Value = server
        .client
        .get(server.url(&format!("/api/v1/assets/{asset_id}/metadata")))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_ne!(after["taken_at"], before_taken_at);

    let _ = fs::remove_dir_all(&root);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn undo_is_rejected_once_the_sidecar_has_been_written() {
    let server = TestServer::start().await;
    let admin = setup_admin(&server).await;
    let root = temp_root();
    let folder = ensure_folder(&server, admin, &root).await;
    let asset_id = seed_indexed_asset(&server, folder, &root, "foto.jpg").await;

    let response = server
        .client
        .post(server.url("/api/v1/metadata/batch"))
        .json(&json!({
            "asset_ids": [asset_id.to_string()],
            "patch": { "title": "Prima del viaggio" }
        }))
        .send()
        .await
        .unwrap();
    let body: serde_json::Value = response.json().await.unwrap();
    let batch_id = body["batch_id"].as_str().unwrap().to_owned();

    keeppix_db::OverrideRepo::new(&server.db)
        .mark_sidecar_written(asset_id)
        .await
        .unwrap();

    let response = server
        .client
        .post(server.url(&format!("/api/v1/metadata/batch/{batch_id}/undo")))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 409);
    let problem: serde_json::Value = response.json().await.unwrap();
    assert_eq!(problem["type"], "keeppix/conflict");

    let _ = fs::remove_dir_all(&root);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn probing_someone_elses_asset_metadata_is_forbidden_not_found() {
    let server = TestServer::start().await;
    let admin = setup_admin(&server).await;
    let root = temp_root();
    let folder = ensure_folder(&server, admin, &root).await;
    let asset_id = seed_indexed_asset(&server, folder, &root, "foto.jpg").await;

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
        .get(server.url(&format!("/api/v1/assets/{asset_id}/metadata")))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 403);

    let _ = fs::remove_dir_all(&root);
}
