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

/// `AssetView` is shared, but `favorite` is per-caller: the search page
/// must resolve it against the caller's own set, like the timeline does.
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

/// Search's "Favorites" chip — the same `SearchNode::Favorite` already
/// covered at a lower level, reverified here at the HTTP layer against
/// the write endpoint.
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

/// Full round-trip: the API embeds the text query via `OpenClipXlmr`
/// (`keeppix-db` doesn't know about ort), passes it to the pgvector
/// subquery, and the result comes back on the search page. Deterministic
/// model: same text → same embedding for both the "fake asset" seeded
/// here and the query, so the asset ends up among the K nearest
/// neighbors.
#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn semantic_search_finds_the_asset_embedded_with_the_same_text() {
    let Some(model_dir) = keeppix_media::openclip_xlmr::first_complete_model_dir() else {
        eprintln!(
            "skipping: openclip-xlmr-it-en incomplete (run .github/workflows/export-openclip-xlmr.yml)"
        );
        return;
    };

    let server = TestServer::start_with_vector().await;
    let asset_id = seed_photo_returning_id(&server, "spiaggia.jpg").await;

    let query = "spiaggia al tramonto";
    let embedding = {
        let model_dir = model_dir.clone();
        let text = query.to_owned();
        tokio::task::spawn_blocking(move || {
            let mut clip =
                keeppix_media::openclip_xlmr::OpenClipXlmr::load(&model_dir).expect("load model");
            clip.embed_text(&text).expect("embed text")
        })
        .await
        .unwrap()
    };
    keeppix_db::EmbeddingRepo::new(&server.db)
        .upsert(asset_id, &embedding, keeppix_db::MODEL_VERSION)
        .await
        .unwrap();

    let response = server
        .client
        .post(server.url("/api/v1/search"))
        .json(&json!({
            "ast": { "op": "semantic", "query": query, "limit": 5 }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    let hits = body["assets"].as_array().unwrap();
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert_eq!(hits[0]["id"], asset_id.to_string());
}

/// `/search/suggest` returns typed objects, no longer flat strings: the
/// frontend must be able to tell `kind` apart without guessing it from
/// the value.
#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
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

/// Like [`seed_photo`], but returns the created asset's id: the
/// `Semantic` test needs it to write an embedding for that specific
/// asset.
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn seed_photo_returning_id(server: &TestServer, name: &str) -> keeppix_domain::AssetId {
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
                name: "Foto-semantic".to_owned(),
                owner_id: user.id,
                root_path: std::path::PathBuf::from("/mnt/foto-semantic"),
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
    a.id
}

/// Like [`seed_photo`], but for two files in the **same** library —
/// `libraries_root_path_key` is unique, so two calls to `seed_photo`
/// (which creates a library each time) can't share a folder.
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
