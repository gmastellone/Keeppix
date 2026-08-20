mod harness;

use chrono::{TimeZone, Utc};
use harness::TestServer;
use keeppix_db::{AssetRepo, FolderRepo, LibraryRepo, UserRepo};
use keeppix_domain::{
    AssetKind, AssetName, AuthContext, ExifData, NewAsset, NewLibrary, SystemRole, Username,
};
use serde_json::json;

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn search_from_ast_does_not_run_user_sql() {
    let server = TestServer::start().await;
    seed_photo(&server, "grecia.jpg").await;

    let response = server
        .client
        .post(server.url("/api/v1/search"))
        .json(&json!({
            "ast": { "op": "text", "value": "grecia" }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["assets"].as_array().unwrap().len(), 1);

    let injected = server
        .client
        .post(server.url("/api/v1/search"))
        .json(&json!({
            "ast": { "op": "text", "value": "grecia'; drop table assets; --" }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(injected.status(), 200);
    let empty: serde_json::Value = injected.json().await.unwrap();
    assert!(empty["assets"].as_array().unwrap().is_empty());
}

/// `AssetView` è condiviso, ma `favorite` è per chiamante (Task 10
/// fase-10): la pagina di ricerca deve risolverlo con il set del chiamante,
/// come la timeline.
#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn search_page_resolves_the_callers_favorite_on_each_tile() {
    let server = TestServer::start().await;
    seed_photo(&server, "grecia.jpg").await;

    let before: serde_json::Value = server
        .client
        .post(server.url("/api/v1/search"))
        .json(&json!({ "ast": { "op": "text", "value": "grecia" } }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let items = before["assets"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["favorite"], false);
    let asset_id = items[0]["id"].as_str().unwrap().to_owned();

    server
        .client
        .put(server.url(&format!("/api/v1/assets/{asset_id}/flags")))
        .json(&json!({ "rating": null, "pick": "none", "color_label": null, "favorite": true }))
        .send()
        .await
        .unwrap();

    let after: serde_json::Value = server
        .client
        .post(server.url("/api/v1/search"))
        .json(&json!({ "ast": { "op": "text", "value": "grecia" } }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(after["assets"][0]["favorite"], true);
}

/// Il chip "Preferiti" di Cerca (spec fase-10 §7bis.1) — stesso
/// `SearchNode::Favorite` già coperto dal Task 6, riverificato qui a
/// livello HTTP con l'endpoint di scrittura di questo task.
#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn the_favorite_search_chip_finds_only_the_callers_favorites() {
    let server = TestServer::start().await;
    seed_two_photos_in_the_same_library(&server, "loved.jpg", "plain.jpg").await;

    let all: serde_json::Value = server
        .client
        .post(server.url("/api/v1/search"))
        .json(&json!({ "ast": { "op": "text", "value": "" } }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let loved_id = all["assets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["filename"] == "loved.jpg")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();

    server
        .client
        .put(server.url(&format!("/api/v1/assets/{loved_id}/flags")))
        .json(&json!({ "rating": null, "pick": "none", "color_label": null, "favorite": true }))
        .send()
        .await
        .unwrap();

    let favorites: serde_json::Value = server
        .client
        .post(server.url("/api/v1/search"))
        .json(&json!({ "ast": { "op": "favorite" } }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let found = favorites["assets"].as_array().unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0]["id"], loved_id);
}

/// `/search/suggest` restituisce oggetti tipizzati (spec fase-10 §23), non
/// più stringhe piatte: il frontend deve poter distinguere `kind` senza
/// indovinarlo dal valore.
#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn suggest_returns_typed_pills_not_bare_strings() {
    let server = TestServer::start().await;
    seed_photo(&server, "cascata.jpg").await;
    let username = Username::parse("giovanni").unwrap();
    let (user, _) = UserRepo::new(&server.db)
        .find_by_username(&username)
        .await
        .unwrap()
        .expect("admin");
    let ctx = AuthContext::user(user.id, SystemRole::Admin);
    let library = LibraryRepo::new(&server.db)
        .create(
            &ctx,
            NewLibrary {
                name: "Fotocamera".to_owned(),
                owner_id: user.id,
                root_path: std::path::PathBuf::from("/mnt/fotocamera"),
                exclude_patterns: vec![],
            },
        )
        .await
        .unwrap();
    let folder = FolderRepo::new(&server.db)
        .ensure_path(library.id, &[])
        .await
        .unwrap();
    let assets = AssetRepo::new(&server.db);
    let asset = assets
        .upsert_discovered(NewAsset {
            folder_id: folder.id,
            filename: AssetName::parse("con-exif.jpg").unwrap(),
            size_bytes: 10,
            mtime: Utc.with_ymd_and_hms(2024, 7, 1, 0, 0, 0).unwrap(),
            inode: Some(2),
            kind: AssetKind::Image,
        })
        .await
        .unwrap()
        .unwrap();
    assets
        .set_indexed(
            asset.id,
            Utc.with_ymd_and_hms(2024, 7, 1, 12, 0, 0).unwrap(),
            1,
            1,
        )
        .await
        .unwrap();
    assets
        .insert_exif(
            asset.id,
            &ExifData {
                raw: serde_json::json!({}),
                taken_at_utc: Utc.with_ymd_and_hms(2024, 7, 1, 12, 0, 0).unwrap(),
                tz_offset_minutes: 0,
                tz_assumed: true,
                width: None,
                height: None,
                camera_make: None,
                camera_model: Some("Fotocamera Suprema".to_owned()),
                lens: None,
                iso: None,
                f_number: None,
                exposure: None,
                focal_length: None,
                gps: None,
            },
        )
        .await
        .unwrap();

    let response = server
        .client
        .get(server.url("/api/v1/search/suggest?q=Fotocamera"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    let suggestions = body["suggestions"].as_array().unwrap();
    assert_eq!(suggestions.len(), 1, "{suggestions:?}");
    assert_eq!(suggestions[0]["kind"], "camera");
    assert_eq!(suggestions[0]["value"], "Fotocamera Suprema");
    assert_eq!(suggestions[0]["label"], "Fotocamera Suprema");
    assert!(suggestions[0].get("color").is_none());
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn saved_searches_round_trip() {
    let server = TestServer::start().await;
    setup(&server).await;
    let created = server
        .client
        .post(server.url("/api/v1/saved-searches"))
        .json(&json!({ "name": "Grecia", "query_text": "grecia" }))
        .send()
        .await
        .unwrap();
    assert_eq!(created.status(), 201);

    let list = server
        .client
        .get(server.url("/api/v1/saved-searches"))
        .send()
        .await
        .unwrap();
    assert_eq!(list.status(), 200);
    let body: serde_json::Value = list.json().await.unwrap();
    assert_eq!(body[0]["name"], "Grecia");
}

#[allow(clippy::unwrap_used)]
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
async fn seed_photo(server: &TestServer, name: &str) {
    setup(server).await;
    let username = Username::parse("giovanni").unwrap();
    let (user, _) = UserRepo::new(&server.db)
        .find_by_username(&username)
        .await
        .unwrap()
        .expect("admin");
    let ctx = AuthContext::user(user.id, SystemRole::Admin);
    let library = LibraryRepo::new(&server.db)
        .create(
            &ctx,
            NewLibrary {
                name: "Foto".to_owned(),
                owner_id: user.id,
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
    let a = AssetRepo::new(&server.db)
        .upsert_discovered(NewAsset {
            folder_id: folder.id,
            filename: AssetName::parse(name).unwrap(),
            size_bytes: 10,
            mtime: Utc.with_ymd_and_hms(2024, 7, 1, 0, 0, 0).unwrap(),
            inode: Some(1),
            kind: AssetKind::Image,
        })
        .await
        .unwrap()
        .unwrap();
    AssetRepo::new(&server.db)
        .set_indexed(
            a.id,
            Utc.with_ymd_and_hms(2024, 7, 1, 12, 0, 0).unwrap(),
            1,
            1,
        )
        .await
        .unwrap();
}

/// Come [`seed_photo`], ma per due file nella **stessa** libreria —
/// `libraries_root_path_key` è unico, quindi due chiamate a `seed_photo`
/// (che crea una libreria a ogni volta) non possono condividere una
/// cartella.
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn seed_two_photos_in_the_same_library(server: &TestServer, first: &str, second: &str) {
    setup(server).await;
    let username = Username::parse("giovanni").unwrap();
    let (user, _) = UserRepo::new(&server.db)
        .find_by_username(&username)
        .await
        .unwrap()
        .expect("admin");
    let ctx = AuthContext::user(user.id, SystemRole::Admin);
    let library = LibraryRepo::new(&server.db)
        .create(
            &ctx,
            NewLibrary {
                name: "Foto".to_owned(),
                owner_id: user.id,
                root_path: std::path::PathBuf::from("/mnt/foto-due"),
                exclude_patterns: vec![],
            },
        )
        .await
        .unwrap();
    let folder = FolderRepo::new(&server.db)
        .ensure_path(library.id, &[])
        .await
        .unwrap();
    for name in [first, second] {
        let a = AssetRepo::new(&server.db)
            .upsert_discovered(NewAsset {
                folder_id: folder.id,
                filename: AssetName::parse(name).unwrap(),
                size_bytes: 10,
                mtime: Utc.with_ymd_and_hms(2024, 7, 1, 0, 0, 0).unwrap(),
                inode: Some(1),
                kind: AssetKind::Image,
            })
            .await
            .unwrap()
            .unwrap();
        AssetRepo::new(&server.db)
            .set_indexed(
                a.id,
                Utc.with_ymd_and_hms(2024, 7, 1, 12, 0, 0).unwrap(),
                1,
                1,
            )
            .await
            .unwrap();
    }
}
