mod harness;

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, TimeZone, Utc};
use harness::TestServer;
use keeppix_db::{AssetRepo, FolderRepo, LibraryRepo, UserRepo};
use keeppix_domain::{
    AssetId, AssetKind, AssetName, AuthContext, FolderId, GeoPoint, NewAsset, NewLibrary, NewUser,
    Password, SystemRole, UserId, Username, hash_password,
};
use serde_json::{Value, json};

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
    let body: Value = response.json().await.expect("setup JSON");
    body["user"]["id"]
        .as_str()
        .expect("user id")
        .parse()
        .expect("valid UUID")
}

#[allow(clippy::expect_used, clippy::unwrap_used)]
async fn create_user(server: &TestServer, admin: UserId, username: &str) -> UserId {
    UserRepo::new(&server.db)
        .create(
            &AuthContext::user(admin, SystemRole::Admin),
            NewUser {
                username: Username::parse(username).expect("username"),
                email: None,
                display_name: username.to_owned(),
                password_hash: hash_password(
                    &Password::parse("correct horse battery staple").expect("password"),
                )
                .expect("password hash")
                .as_str()
                .to_owned(),
                role: SystemRole::User,
            },
        )
        .await
        .expect("user")
        .id
}

#[allow(clippy::expect_used, clippy::unwrap_used)]
async fn login(server: &TestServer, username: &str) {
    let response = server
        .client
        .post(server.url("/api/v1/auth/login"))
        .json(&json!({
            "username": username,
            "password": "correct horse battery staple"
        }))
        .send()
        .await
        .expect("login request");
    assert_eq!(response.status(), 200);
}

#[allow(clippy::expect_used, clippy::unwrap_used)]
fn temp_root(label: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "keeppix-api-geotag-{label}-{}",
        uuid::Uuid::now_v7()
    ));
    fs::create_dir_all(&root).expect("test root");
    root
}

#[allow(clippy::expect_used, clippy::unwrap_used)]
async fn ensure_folder(server: &TestServer, actor: UserId, owner: UserId, root: &Path) -> FolderId {
    let library = LibraryRepo::new(&server.db)
        .create(
            &AuthContext::user(actor, SystemRole::Admin),
            NewLibrary {
                name: format!("Photos {owner}"),
                owner_id: owner,
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
async fn seed_asset(
    server: &TestServer,
    folder: FolderId,
    root: &Path,
    filename: &str,
    taken_at: DateTime<Utc>,
) -> AssetId {
    fs::write(root.join("2024").join(filename), b"contents").expect("asset file");
    let repo = AssetRepo::new(&server.db);
    let asset = repo
        .upsert_discovered(NewAsset {
            folder_id: folder,
            filename: AssetName::parse(filename).expect("filename"),
            size_bytes: 8,
            mtime: taken_at,
            inode: None,
            kind: AssetKind::Image,
        })
        .await
        .expect("asset insert")
        .expect("new asset")
        .id;
    repo.set_indexed(asset, taken_at, 100, 100)
        .await
        .expect("asset indexing");
    asset
}

#[allow(clippy::expect_used, clippy::unwrap_used)]
async fn location_source(server: &TestServer, asset: AssetId) -> Option<String> {
    sqlx::query_scalar("SELECT location_source FROM assets WHERE id = $1")
        .bind(asset.as_uuid())
        .fetch_one(server.db.pool())
        .await
        .expect("location source")
}

#[allow(clippy::expect_used, clippy::unwrap_used)]
async fn effective(server: &TestServer, asset: AssetId) -> Value {
    server
        .client
        .get(server.url(&format!("/api/v1/assets/{asset}/metadata")))
        .send()
        .await
        .expect("effective request")
        .json()
        .await
        .expect("effective JSON")
}

#[tokio::test]
#[allow(clippy::expect_used, clippy::unwrap_used)]
async fn search_assignment_overwrites_exif_and_undo_restores_its_source() {
    let server = TestServer::start().await;
    let admin = setup_admin(&server).await;
    let root = temp_root("search");
    let folder = ensure_folder(&server, admin, admin, &root).await;
    let asset = seed_asset(
        &server,
        folder,
        &root,
        "search.jpg",
        Utc.with_ymd_and_hms(2026, 8, 18, 10, 0, 0).unwrap(),
    )
    .await;
    AssetRepo::new(&server.db)
        .set_exif_location(
            asset,
            GeoPoint {
                lat: 40.0,
                lon: 8.0,
            },
        )
        .await
        .unwrap();

    let response = server
        .client
        .post(server.url("/api/v1/metadata/batch"))
        .json(&json!({
            "asset_ids": [asset.to_string()],
            "patch": {
                "location": { "lat": 45.4642, "lon": 9.19 },
                "place_id": 3_173_435
            }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body: Value = response.json().await.unwrap();
    let batch_id = body["batch_id"].as_str().expect("batch id");

    let assigned = effective(&server, asset).await;
    assert_eq!(assigned["place_id"], 3_173_435);
    assert_eq!(assigned["location"]["lat"], 45.4642);
    assert_eq!(
        location_source(&server, asset).await.as_deref(),
        Some("user")
    );

    let response = server
        .client
        .post(server.url(&format!("/api/v1/metadata/batch/{batch_id}/undo")))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 204);
    let restored = effective(&server, asset).await;
    assert_eq!(restored["location"]["lat"], 40.0);
    assert_eq!(restored["place_id"], Value::Null);
    assert_eq!(
        location_source(&server, asset).await.as_deref(),
        Some("exif")
    );

    let _ = fs::remove_dir_all(root);
}

#[tokio::test]
#[allow(clippy::expect_used, clippy::unwrap_used)]
async fn map_pin_assignment_clears_place_and_records_map_pin_source() {
    let server = TestServer::start().await;
    let admin = setup_admin(&server).await;
    let root = temp_root("pin");
    let folder = ensure_folder(&server, admin, admin, &root).await;
    let asset = seed_asset(
        &server,
        folder,
        &root,
        "pin.jpg",
        Utc.with_ymd_and_hms(2026, 8, 18, 10, 0, 0).unwrap(),
    )
    .await;

    let response = server
        .client
        .post(server.url("/api/v1/metadata/batch"))
        .json(&json!({
            "asset_ids": [asset.to_string()],
            "patch": {
                "location": { "lat": 41.9028, "lon": 12.4964 },
                "place_id": null
            }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    let assigned = effective(&server, asset).await;
    assert_eq!(assigned["location"]["lon"], 12.4964);
    assert_eq!(assigned["place_id"], Value::Null);
    assert_eq!(
        location_source(&server, asset).await.as_deref(),
        Some("map_pin")
    );

    let _ = fs::remove_dir_all(root);
}

#[tokio::test]
#[allow(clippy::expect_used, clippy::unwrap_used)]
async fn copy_assignment_uses_effective_source_location_and_records_copied_source() {
    let server = TestServer::start().await;
    let admin = setup_admin(&server).await;
    let root = temp_root("copy");
    let folder = ensure_folder(&server, admin, admin, &root).await;
    let taken_at = Utc.with_ymd_and_hms(2026, 8, 18, 10, 0, 0).unwrap();
    let source = seed_asset(&server, folder, &root, "source.jpg", taken_at).await;
    let target = seed_asset(&server, folder, &root, "target.jpg", taken_at).await;
    AssetRepo::new(&server.db)
        .set_exif_location(
            source,
            GeoPoint {
                lat: -33.8688,
                lon: 151.2093,
            },
        )
        .await
        .unwrap();

    // Nessun setup di PMTiles: copiare coordinate è indipendente dallo sfondo.
    let response = server
        .client
        .post(server.url("/api/v1/metadata/batch/copy-location"))
        .json(&json!({
            "source_asset_id": source.to_string(),
            "target_asset_ids": [target.to_string()]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    let copied = effective(&server, target).await;
    assert_eq!(copied["location"]["lat"], -33.8688);
    assert_eq!(copied["location"]["lon"], 151.2093);
    assert_eq!(
        location_source(&server, target).await.as_deref(),
        Some("copied")
    );

    let _ = fs::remove_dir_all(root);
}

#[tokio::test]
#[allow(clippy::expect_used, clippy::unwrap_used)]
async fn copying_from_a_foreign_source_is_forbidden_not_not_found() {
    let server = TestServer::start().await;
    let admin = setup_admin(&server).await;
    let mario = create_user(&server, admin, "mario").await;
    let admin_root = temp_root("foreign-admin");
    let mario_root = temp_root("foreign-mario");
    let admin_folder = ensure_folder(&server, admin, admin, &admin_root).await;
    let mario_folder = ensure_folder(&server, admin, mario, &mario_root).await;
    let taken_at = Utc.with_ymd_and_hms(2026, 8, 18, 10, 0, 0).unwrap();
    let source = seed_asset(&server, admin_folder, &admin_root, "private.jpg", taken_at).await;
    let target = seed_asset(&server, mario_folder, &mario_root, "mine.jpg", taken_at).await;
    AssetRepo::new(&server.db)
        .set_exif_location(
            source,
            GeoPoint {
                lat: 45.0,
                lon: 9.0,
            },
        )
        .await
        .unwrap();
    login(&server, "mario").await;

    let response = server
        .client
        .post(server.url("/api/v1/metadata/batch/copy-location"))
        .json(&json!({
            "source_asset_id": source.to_string(),
            "target_asset_ids": [target.to_string()]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 403);
    let problem: Value = response.json().await.unwrap();
    assert_eq!(problem["type"], "keeppix/forbidden");

    let _ = fs::remove_dir_all(admin_root);
    let _ = fs::remove_dir_all(mario_root);
}

#[tokio::test]
#[allow(clippy::expect_used, clippy::unwrap_used)]
async fn gpx_import_interpolates_and_uses_the_default_five_minute_tolerance() {
    let server = TestServer::start().await;
    let admin = setup_admin(&server).await;
    let root = temp_root("gpx");
    let folder = ensure_folder(&server, admin, admin, &root).await;
    let before = seed_asset(
        &server,
        folder,
        &root,
        "before.jpg",
        Utc.with_ymd_and_hms(2026, 8, 18, 9, 58, 0).unwrap(),
    )
    .await;
    let middle = seed_asset(
        &server,
        folder,
        &root,
        "middle.jpg",
        Utc.with_ymd_and_hms(2026, 8, 18, 10, 5, 0).unwrap(),
    )
    .await;
    let after = seed_asset(
        &server,
        folder,
        &root,
        "after.jpg",
        Utc.with_ymd_and_hms(2026, 8, 18, 10, 12, 0).unwrap(),
    )
    .await;
    let too_late = seed_asset(
        &server,
        folder,
        &root,
        "too-late.jpg",
        Utc.with_ymd_and_hms(2026, 8, 18, 10, 30, 0).unwrap(),
    )
    .await;
    let gpx = r#"<?xml version="1.0"?>
        <gpx version="1.1" creator="keeppix-test">
          <trk><trkseg>
            <trkpt lat="40.0" lon="8.0"><time>2026-08-18T10:00:00Z</time></trkpt>
            <trkpt lat="50.0" lon="18.0"><time>2026-08-18T10:10:00Z</time></trkpt>
          </trkseg></trk>
        </gpx>"#;

    // Nessuna regione PMTiles viene montata: l'import deve funzionare lo stesso.
    let response = server
        .client
        .post(server.url("/api/v1/metadata/batch/import-gpx"))
        .json(&json!({
            "asset_ids": [
                before.to_string(),
                middle.to_string(),
                after.to_string(),
                too_late.to_string()
            ],
            "gpx": gpx
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    let before_metadata = effective(&server, before).await;
    assert_eq!(before_metadata["location"]["lat"], 40.0);
    assert_eq!(before_metadata["location"]["lon"], 8.0);
    assert_eq!(
        location_source(&server, before).await.as_deref(),
        Some("gpx")
    );

    let middle_metadata = effective(&server, middle).await;
    assert_eq!(middle_metadata["location"]["lat"], 45.0);
    assert_eq!(middle_metadata["location"]["lon"], 13.0);

    let after_metadata = effective(&server, after).await;
    assert_eq!(after_metadata["location"]["lat"], 50.0);
    assert_eq!(after_metadata["location"]["lon"], 18.0);

    assert_eq!(effective(&server, too_late).await["location"], Value::Null);
    assert_eq!(location_source(&server, too_late).await, None);

    let _ = fs::remove_dir_all(root);
}
