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

/// Un solo asset dentro una cartella già preparata da [`ensure_folder`].
#[allow(clippy::expect_used, clippy::unwrap_used)]
async fn seed_asset(
    server: &TestServer,
    folder: FolderId,
    root: &std::path::Path,
    filename: &str,
) -> AssetId {
    fs::write(root.join("2024").join(filename), b"contenuto").expect("file su disco");

    AssetRepo::new(&server.db)
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
        .id
}

#[allow(clippy::expect_used, clippy::unwrap_used)]
fn temp_root() -> PathBuf {
    let root = std::env::temp_dir().join(format!("keeppix-api-flags-{}", uuid::Uuid::now_v7()));
    fs::create_dir_all(&root).expect("radice di test");
    root
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn setting_and_reading_back_the_callers_flags_round_trips() {
    let server = TestServer::start().await;
    let admin = setup_admin(&server).await;
    let root = temp_root();
    let folder = ensure_folder(&server, admin, &root).await;
    let asset_id = seed_asset(&server, folder, &root, "foto.jpg").await;

    let unvoted: serde_json::Value = server
        .client
        .get(server.url(&format!("/api/v1/assets/{asset_id}/flags")))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(unvoted["rating"], serde_json::Value::Null);
    assert_eq!(unvoted["pick"], "none");

    let response = server
        .client
        .put(server.url(&format!("/api/v1/assets/{asset_id}/flags")))
        .json(&json!({ "rating": 4, "pick": "pick", "color_label": "rosso" }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 204);

    let after: serde_json::Value = server
        .client
        .get(server.url(&format!("/api/v1/assets/{asset_id}/flags")))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(after["rating"], 4);
    assert_eq!(after["pick"], "pick");
    assert_eq!(after["color_label"], "rosso");

    let _ = fs::remove_dir_all(&root);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn a_rating_above_five_is_a_bad_request() {
    let server = TestServer::start().await;
    let admin = setup_admin(&server).await;
    let root = temp_root();
    let folder = ensure_folder(&server, admin, &root).await;
    let asset_id = seed_asset(&server, folder, &root, "foto.jpg").await;

    let response = server
        .client
        .put(server.url(&format!("/api/v1/assets/{asset_id}/flags")))
        .json(&json!({ "rating": 9, "pick": "none", "color_label": null }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 400);
    let problem: serde_json::Value = response.json().await.unwrap();
    assert_eq!(problem["type"], "keeppix/invalid-rating");

    let _ = fs::remove_dir_all(&root);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn two_users_voting_on_the_same_asset_do_not_overwrite_each_other() {
    let server = TestServer::start().await;
    let admin = setup_admin(&server).await;
    let root = temp_root();
    let folder = ensure_folder(&server, admin, &root).await;
    let asset_id = seed_asset(&server, folder, &root, "foto.jpg").await;

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

    server
        .client
        .put(server.url(&format!("/api/v1/assets/{asset_id}/flags")))
        .json(&json!({ "rating": 5, "pick": "none", "color_label": null }))
        .send()
        .await
        .unwrap();

    let mario_client = reqwest::Client::builder()
        .cookie_store(true)
        .default_headers(harness::client_headers())
        .build()
        .unwrap();
    let login = mario_client
        .post(server.url("/api/v1/auth/login"))
        .json(&json!({ "username": "mario", "password": "correct horse battery staple" }))
        .send()
        .await
        .unwrap();
    assert_eq!(login.status(), 200);

    mario_client
        .put(server.url(&format!("/api/v1/assets/{asset_id}/flags")))
        .json(&json!({ "rating": 1, "pick": "reject", "color_label": null }))
        .send()
        .await
        .unwrap();

    let admin_flags: serde_json::Value = server
        .client
        .get(server.url(&format!("/api/v1/assets/{asset_id}/flags")))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        admin_flags["rating"], 5,
        "il voto di mario non deve sovrascrivere quello dell'admin"
    );

    let _ = fs::remove_dir_all(&root);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn batch_set_applies_the_same_flags_to_every_asset() {
    let server = TestServer::start().await;
    let admin = setup_admin(&server).await;
    let root = temp_root();
    let folder = ensure_folder(&server, admin, &root).await;
    let a = seed_asset(&server, folder, &root, "a.jpg").await;
    let b = seed_asset(&server, folder, &root, "b.jpg").await;

    let response = server
        .client
        .post(server.url("/api/v1/flags/batch"))
        .json(&json!({
            "asset_ids": [a.to_string(), b.to_string()],
            "rating": 3,
            "pick": "reject",
            "color_label": null
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["succeeded"].as_array().unwrap().len(), 2);
    assert!(body["failed"].as_array().unwrap().is_empty());
    assert!(body["batch_id"].is_null());

    for asset in [a, b] {
        let flags: serde_json::Value = server
            .client
            .get(server.url(&format!("/api/v1/assets/{asset}/flags")))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(flags["rating"], 3);
        assert_eq!(flags["pick"], "reject");
    }

    let _ = fs::remove_dir_all(&root);
}

/// Spec §3: un lotto misto non è tutto-o-niente — l'asset visibile riesce,
/// quello altrui finisce in `failed` con `permission-denied`, HTTP 200.
#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn flags_batch_reports_partial_success_when_one_asset_is_foreign() {
    let server = TestServer::start().await;
    let admin = setup_admin(&server).await;
    let admin_root = temp_root();
    let admin_folder = ensure_folder(&server, admin, &admin_root).await;
    let foreign = seed_asset(&server, admin_folder, &admin_root, "private.jpg").await;

    let mario = keeppix_db::UserRepo::new(&server.db)
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
        .unwrap()
        .id;
    let mario_root =
        std::env::temp_dir().join(format!("keeppix-api-flags-mario-{}", uuid::Uuid::now_v7()));
    fs::create_dir_all(&mario_root).unwrap();
    let mario_folder = {
        let ctx = AuthContext::user(admin, SystemRole::Admin);
        let library = LibraryRepo::new(&server.db)
            .create(
                &ctx,
                NewLibrary {
                    name: "Mario".to_owned(),
                    owner_id: mario,
                    root_path: mario_root.clone(),
                    exclude_patterns: vec![],
                },
            )
            .await
            .unwrap()
            .id;
        fs::create_dir_all(mario_root.join("2024")).unwrap();
        FolderRepo::new(&server.db)
            .ensure_path(library, &["2024"])
            .await
            .unwrap()
            .id
    };
    let mine = seed_asset(&server, mario_folder, &mario_root, "mine.jpg").await;

    let mario_client = reqwest::Client::builder()
        .cookie_store(true)
        .default_headers(harness::client_headers())
        .build()
        .unwrap();
    let login = mario_client
        .post(server.url("/api/v1/auth/login"))
        .json(&json!({ "username": "mario", "password": "correct horse battery staple" }))
        .send()
        .await
        .unwrap();
    assert_eq!(login.status(), 200);

    let response = mario_client
        .post(server.url("/api/v1/flags/batch"))
        .json(&json!({
            "asset_ids": [mine.to_string(), foreign.to_string()],
            "rating": 5,
            "pick": "pick",
            "color_label": null
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200, "partial success is still HTTP 200");
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(
        body["succeeded"],
        json!([mine.to_string()]),
        "only the visible asset must succeed"
    );
    let failed = body["failed"].as_array().expect("failed array");
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0]["id"], foreign.to_string());
    assert_eq!(failed[0]["reason"], "permission-denied");
    assert!(body["batch_id"].is_null());

    let mine_flags: serde_json::Value = mario_client
        .get(server.url(&format!("/api/v1/assets/{mine}/flags")))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(mine_flags["rating"], 5);
    assert_eq!(mine_flags["pick"], "pick");

    let _ = fs::remove_dir_all(&admin_root);
    let _ = fs::remove_dir_all(&mario_root);
}
