mod harness;

use chrono::{TimeZone, Timelike, Utc};
use harness::{TestServer, client_headers};
use keeppix_db::{AssetRepo, FolderRepo, LibraryRepo, StackRepo, UserRepo};
use keeppix_domain::{
    AssetKind, AssetName, AuthContext, FolderId, GeoPoint, NewAsset, NewLibrary, NewUser, Password,
    SystemRole, Username, hash_password,
};
use serde_json::json;

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn timeline_buckets_require_auth() {
    let server = TestServer::start().await;
    let response = server
        .client
        .get(server.url("/api/v1/timeline/buckets"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 401);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["type"], "keeppix/unauthenticated");
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn buckets_return_month_counts_for_indexed_assets() {
    let server = TestServer::start().await;
    let (folder, _) = seed_library(&server).await;
    index_photo(&server, folder, "a.jpg", 2024, 7, 2).await;
    index_photo(&server, folder, "b.jpg", 2024, 8, 3).await;

    let response = server
        .client
        .get(server.url("/api/v1/timeline/buckets"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body.as_array().unwrap().len(), 2);
    assert_eq!(body[0]["month"], "2024-08");
    assert_eq!(body[0]["count"], 1);
    assert_eq!(body[1]["month"], "2024-07");
    assert_eq!(body[1]["count"], 1);
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn timeline_page_uses_keyset_cursor() {
    let server = TestServer::start().await;
    let (folder, _) = seed_library(&server).await;
    index_photo(&server, folder, "c.jpg", 2024, 7, 10).await;
    index_photo(&server, folder, "d.jpg", 2024, 7, 9).await;
    index_photo(&server, folder, "e.jpg", 2024, 7, 8).await;

    let first = server
        .client
        .get(server.url("/api/v1/timeline?bucket=2024-07&limit=2"))
        .send()
        .await
        .unwrap();
    assert_eq!(first.status(), 200);
    let page: serde_json::Value = first.json().await.unwrap();
    let items = page["assets"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    let cursor = page["next_cursor"].as_str().unwrap();

    let second = server
        .client
        .get(server.url(&format!(
            "/api/v1/timeline?bucket=2024-07&limit=2&cursor={cursor}"
        )))
        .send()
        .await
        .unwrap();
    assert_eq!(second.status(), 200);
    let next: serde_json::Value = second.json().await.unwrap();
    let rest = next["assets"].as_array().unwrap();
    assert_eq!(rest.len(), 1);
    assert_ne!(rest[0]["id"], items[0]["id"]);
    assert_ne!(rest[0]["id"], items[1]["id"]);
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn asset_detail_returns_the_existing_public_view_without_coordinates() {
    let server = TestServer::start().await;
    let (folder, _) = seed_library(&server).await;
    index_photo(&server, folder, "map-point.jpg", 2024, 7, 10).await;
    let asset = AssetRepo::new(&server.db)
        .find_by_folder(&admin_ctx(&server).await, folder)
        .await
        .unwrap()
        .remove(0);

    let response = server
        .client
        .get(server.url(&format!("/api/v1/assets/{}", asset.id)))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["id"], asset.id.to_string());
    assert_eq!(body["filename"], "map-point.jpg");
    assert!(body.get("location").is_none());
    assert!(body.get("lat").is_none());
    assert!(body.get("lon").is_none());
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn timeline_bbox_filters_pages_and_bucket_counts() {
    let server = TestServer::start().await;
    let (folder, _) = seed_library(&server).await;
    index_photo(&server, folder, "rome.jpg", 2024, 7, 10).await;
    index_photo(&server, folder, "kyoto.jpg", 2024, 7, 9).await;
    let assets = AssetRepo::new(&server.db)
        .find_by_folder(&admin_ctx(&server).await, folder)
        .await
        .unwrap();
    let rome = assets
        .iter()
        .find(|asset| asset.filename.as_str() == "rome.jpg")
        .unwrap();
    let kyoto = assets
        .iter()
        .find(|asset| asset.filename.as_str() == "kyoto.jpg")
        .unwrap();
    AssetRepo::new(&server.db)
        .set_exif_location(
            rome.id,
            GeoPoint {
                lat: 41.9028,
                lon: 12.4964,
            },
        )
        .await
        .unwrap();
    AssetRepo::new(&server.db)
        .set_exif_location(
            kyoto.id,
            GeoPoint {
                lat: 35.0116,
                lon: 135.7681,
            },
        )
        .await
        .unwrap();

    let buckets = server
        .client
        .get(server.url("/api/v1/timeline/buckets?bbox=10,40,13,43"))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    assert_eq!(buckets[0]["count"], 1);

    let page = server
        .client
        .get(server.url("/api/v1/timeline?bucket=2024-07&bbox=10,40,13,43"))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    assert_eq!(page["assets"].as_array().unwrap().len(), 1);
    assert_eq!(page["assets"][0]["filename"], "rome.jpg");
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn timeline_keyset_keeps_assets_that_share_a_truncated_second() {
    let server = TestServer::start().await;
    let (folder, _) = seed_library(&server).await;
    let base = Utc.with_ymd_and_hms(2024, 7, 10, 12, 0, 0).unwrap();
    index_photo_at(
        &server,
        folder,
        "late.jpg",
        base.with_nanosecond(500_000_000).unwrap(),
    )
    .await;
    index_photo_at(
        &server,
        folder,
        "mid.jpg",
        base.with_nanosecond(400_000_000).unwrap(),
    )
    .await;
    index_photo_at(
        &server,
        folder,
        "early.jpg",
        base.with_nanosecond(300_000_000).unwrap(),
    )
    .await;

    let first = server
        .client
        .get(server.url("/api/v1/timeline?bucket=2024-07&limit=2"))
        .send()
        .await
        .unwrap();
    assert_eq!(first.status(), 200);
    let page: serde_json::Value = first.json().await.unwrap();
    let first_ids: Vec<&str> = page["assets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["id"].as_str().unwrap())
        .collect();
    assert_eq!(first_ids.len(), 2);
    let cursor = page["next_cursor"].as_str().unwrap();

    let second = server
        .client
        .get(server.url(&format!(
            "/api/v1/timeline?bucket=2024-07&limit=2&cursor={cursor}"
        )))
        .send()
        .await
        .unwrap();
    assert_eq!(second.status(), 200);
    let next: serde_json::Value = second.json().await.unwrap();
    let rest: Vec<&str> = next["assets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        rest.len(),
        1,
        "sub-second timestamps must not vanish on page 2"
    );
    assert!(!first_ids.contains(&rest[0]));
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn folder_tree_lists_visible_folders() {
    let server = TestServer::start().await;
    let (root, _) = seed_library(&server).await;
    let child = FolderRepo::new(&server.db)
        .ensure_path(
            LibraryRepo::new(&server.db)
                .list(&admin_ctx(&server).await)
                .await
                .unwrap()[0]
                .id,
            &["Vacanze"],
        )
        .await
        .unwrap();

    let response = server
        .client
        .get(server.url("/api/v1/folders/tree"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let tree: serde_json::Value = response.json().await.unwrap();
    let folders = tree.as_array().unwrap();
    assert_eq!(folders.len(), 2);
    let ids: Vec<&str> = folders.iter().map(|f| f["id"].as_str().unwrap()).collect();
    assert!(ids.contains(&root.to_string().as_str()));
    assert!(ids.contains(&child.id.to_string().as_str()));
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn folder_children_include_direct_folders_and_assets() {
    let server = TestServer::start().await;
    let (root, library) = seed_library(&server).await;
    FolderRepo::new(&server.db)
        .ensure_path(library, &["Album"])
        .await
        .unwrap();
    index_photo(&server, root, "cover.jpg", 2024, 7, 1).await;

    let response = server
        .client
        .get(server.url(&format!("/api/v1/folders/{root}/children")))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["folders"].as_array().unwrap().len(), 1);
    assert_eq!(body["folders"][0]["name"], "Album");
    assert_eq!(body["assets"].as_array().unwrap().len(), 1);
    assert_eq!(body["assets"][0]["filename"], "cover.jpg");
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn probing_someone_elses_folder_is_forbidden() {
    let server = TestServer::start().await;
    let (folder, _) = seed_library(&server).await;
    seed_user(&server, "luca").await;

    let client = reqwest::Client::builder()
        .cookie_store(true)
        .default_headers(client_headers())
        .build()
        .unwrap();
    let login = client
        .post(server.url("/api/v1/auth/login"))
        .json(&json!({
            "username": "luca",
            "password": "correct horse battery staple"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(login.status(), 200);

    let response = client
        .get(server.url(&format!("/api/v1/folders/{folder}/children")))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 403);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["type"], "keeppix/forbidden");
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn folder_tree_roots_omits_descendants() {
    let server = TestServer::start().await;
    let (root, library) = seed_library(&server).await;
    FolderRepo::new(&server.db)
        .ensure_path(library, &["Vacanze"])
        .await
        .unwrap();

    let full = server
        .client
        .get(server.url("/api/v1/folders/tree"))
        .send()
        .await
        .unwrap();
    assert_eq!(full.status(), 200);
    assert_eq!(
        full.json::<serde_json::Value>()
            .await
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        2
    );

    let roots = server
        .client
        .get(server.url("/api/v1/folders/tree?roots=true"))
        .send()
        .await
        .unwrap();
    assert_eq!(roots.status(), 200);
    let body: serde_json::Value = roots.json().await.unwrap();
    let folders = body.as_array().unwrap();
    assert_eq!(folders.len(), 1);
    assert_eq!(folders[0]["id"], root.to_string());
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn moving_a_folder_does_not_rewrite_asset_rows() {
    let server = TestServer::start().await;
    let (_root, library) = seed_library(&server).await;
    let folders = FolderRepo::new(&server.db);
    let ctx = admin_ctx(&server).await;
    folders
        .ensure_path(library, &["2024", "Grecia"])
        .await
        .unwrap();
    let archive = folders.ensure_path(library, &["Archivio"]).await.unwrap();
    let root_folder = folders.ensure_root(library).await.unwrap();
    let y2024 = folders.ensure_child(&root_folder, "2024").await.unwrap();
    let greece = folders.ensure_child(&y2024, "Grecia").await.unwrap();
    index_photo(&server, greece.id, "DSC_0042.ARW", 2024, 7, 1).await;

    let before = AssetRepo::new(&server.db)
        .find_by_folder(&ctx, greece.id)
        .await
        .unwrap();
    assert_eq!(before.len(), 1);
    let asset_id = before[0].id;

    let moved = server
        .client
        .patch(server.url(&format!("/api/v1/folders/{}", greece.id)))
        .json(&json!({ "parent_id": archive.id.to_string() }))
        .send()
        .await
        .unwrap();
    assert_eq!(moved.status(), 204);

    let after = AssetRepo::new(&server.db)
        .find_by_id(&ctx, asset_id)
        .await
        .unwrap();
    assert_eq!(after.folder_id, greece.id);
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn probing_someone_elses_library_buckets_is_forbidden() {
    let server = TestServer::start().await;
    let (_, library) = seed_library(&server).await;
    seed_user(&server, "luca").await;

    let client = reqwest::Client::builder()
        .cookie_store(true)
        .default_headers(client_headers())
        .build()
        .unwrap();
    let login = client
        .post(server.url("/api/v1/auth/login"))
        .json(&json!({
            "username": "luca",
            "password": "correct horse battery staple"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(login.status(), 200);

    let response = client
        .get(server.url(&format!("/api/v1/timeline/buckets?library={library}")))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 403);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["type"], "keeppix/forbidden");
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn timeline_geometry_requires_auth() {
    let server = TestServer::start().await;
    let response = server
        .client
        .get(server.url("/api/v1/timeline/geometry"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 401);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["type"], "keeppix/unauthenticated");
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn timeline_geometry_returns_ordered_binary_records() {
    let server = TestServer::start().await;
    let (folder, _) = seed_library(&server).await;
    // Più vecchio -> più recente: la geometria deve uscire come la timeline,
    // taken_at DESC, id DESC.
    index_photo_sized(&server, folder, "old.jpg", 2024, 7, 1, 100, 50).await;
    index_photo_sized(&server, folder, "new.jpg", 2024, 8, 1, 6000, 4000).await;

    let response = server
        .client
        .get(server.url("/api/v1/timeline/geometry"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(
        response.headers().get("content-type").unwrap(),
        "application/octet-stream"
    );
    assert!(response.headers().get("etag").is_some());
    let body = response.bytes().await.unwrap();
    let records = decode_geometry(&body);
    assert_eq!(records.len(), 2);
    // record 0 = new.jpg (più recente)
    assert_eq!(records[0], (6000, 4000, 2024 * 12 + 8));
    assert_eq!(records[1], (100, 50, 2024 * 12 + 7));
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn timeline_geometry_encodes_missing_dimensions_as_zero() {
    let server = TestServer::start().await;
    let (folder, _) = seed_library(&server).await;
    let assets = AssetRepo::new(&server.db);
    let unsized_asset = assets
        .upsert_discovered(keeppix_domain::NewAsset {
            folder_id: folder,
            filename: keeppix_domain::AssetName::parse("unsized.jpg").unwrap(),
            size_bytes: 10,
            mtime: Utc.with_ymd_and_hms(2024, 7, 1, 12, 0, 0).unwrap(),
            inode: Some(1),
            kind: keeppix_domain::AssetKind::Image,
        })
        .await
        .unwrap()
        .unwrap();
    sqlx::query(
        "UPDATE assets SET status = 'indexed', taken_at_utc = $2, width = NULL, height = NULL \
         WHERE id = $1",
    )
    .bind(unsized_asset.id.as_uuid())
    .bind(Utc.with_ymd_and_hms(2024, 7, 1, 12, 0, 0).unwrap())
    .execute(server.db.pool())
    .await
    .unwrap();

    let response = server
        .client
        .get(server.url("/api/v1/timeline/geometry"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = response.bytes().await.unwrap();
    let records = decode_geometry(&body);
    assert_eq!(records.len(), 1);
    assert_eq!(
        records[0],
        (0, 0, 2024 * 12 + 7),
        "un asset non ancora dimensionato entra con w=0,h=0, non viene escluso"
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn timeline_geometry_returns_304_on_matching_if_none_match() {
    let server = TestServer::start().await;
    let (folder, _) = seed_library(&server).await;
    index_photo_sized(&server, folder, "a.jpg", 2024, 7, 1, 100, 50).await;

    let first = server
        .client
        .get(server.url("/api/v1/timeline/geometry"))
        .send()
        .await
        .unwrap();
    assert_eq!(first.status(), 200);
    let etag = first
        .headers()
        .get("etag")
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();

    let second = server
        .client
        .get(server.url("/api/v1/timeline/geometry"))
        .header("If-None-Match", etag)
        .send()
        .await
        .unwrap();
    assert_eq!(second.status(), 304);
    let body = second.bytes().await.unwrap();
    assert!(body.is_empty());
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn timeline_geometry_count_matches_bucket_counts() {
    let server = TestServer::start().await;
    let (folder, _) = seed_library(&server).await;
    index_photo_sized(&server, folder, "a.jpg", 2024, 7, 2, 100, 50).await;
    index_photo_sized(&server, folder, "b.jpg", 2024, 8, 3, 100, 50).await;
    index_photo_sized(&server, folder, "c.jpg", 2024, 8, 4, 100, 50).await;

    let buckets: serde_json::Value = server
        .client
        .get(server.url("/api/v1/timeline/buckets"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let bucket_total: i64 = buckets
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["count"].as_i64().unwrap())
        .sum();

    let geometry_response = server
        .client
        .get(server.url("/api/v1/timeline/geometry"))
        .send()
        .await
        .unwrap();
    let body = geometry_response.bytes().await.unwrap();
    let records = decode_geometry(&body);
    assert_eq!(records.len(), usize::try_from(bucket_total).unwrap());
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn timeline_collapses_a_raw_jpeg_stack_into_one_tile_with_a_badge() {
    let server = TestServer::start().await;
    let (folder, _) = seed_library(&server).await;
    let taken = Utc.with_ymd_and_hms(2024, 6, 1, 12, 0, 0).unwrap();
    let assets = AssetRepo::new(&server.db);
    let raw = assets
        .upsert_discovered(NewAsset {
            folder_id: folder,
            filename: AssetName::parse("DSC_0042.ARW").unwrap(),
            size_bytes: 1000,
            mtime: taken,
            inode: Some(1),
            kind: AssetKind::RawImage,
        })
        .await
        .unwrap()
        .unwrap();
    assets.set_indexed(raw.id, taken, 6000, 4000).await.unwrap();
    let jpeg = assets
        .upsert_discovered(NewAsset {
            folder_id: folder,
            filename: AssetName::parse("DSC_0042.JPG").unwrap(),
            size_bytes: 500,
            mtime: taken,
            inode: Some(2),
            kind: AssetKind::Image,
        })
        .await
        .unwrap()
        .unwrap();
    assets
        .set_indexed(jpeg.id, taken, 6000, 4000)
        .await
        .unwrap();
    StackRepo::new(&server.db)
        .regroup_folder(folder)
        .await
        .unwrap();

    let page = server
        .client
        .get(server.url("/api/v1/timeline?bucket=2024-06"))
        .send()
        .await
        .unwrap();
    assert_eq!(page.status(), 200);
    let body: serde_json::Value = page.json().await.unwrap();
    let items = body["assets"].as_array().unwrap();
    assert_eq!(items.len(), 1, "raw+jpeg stack must collapse to one tile");
    assert_eq!(items[0]["id"], raw.id.to_string(), "raw is the primary");
    assert_eq!(items[0]["stack_size"], 2);
    assert_eq!(items[0]["raw_kind"], "raw+jpeg");

    let buckets: serde_json::Value = server
        .client
        .get(server.url("/api/v1/timeline/buckets"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        buckets[0]["count"], 1,
        "buckets must count stacks, not files"
    );

    let geometry_response = server
        .client
        .get(server.url("/api/v1/timeline/geometry"))
        .send()
        .await
        .unwrap();
    let geo_body = geometry_response.bytes().await.unwrap();
    let records = decode_geometry(&geo_body);
    assert_eq!(records.len(), 1, "geometry must also collapse the stack");
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn timeline_reports_stack_size_one_for_an_unstacked_asset() {
    let server = TestServer::start().await;
    let (folder, _) = seed_library(&server).await;
    index_photo(&server, folder, "lone.jpg", 2024, 6, 1).await;

    let page = server
        .client
        .get(server.url("/api/v1/timeline?bucket=2024-06"))
        .send()
        .await
        .unwrap();
    assert_eq!(page.status(), 200);
    let body: serde_json::Value = page.json().await.unwrap();
    let items = body["assets"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["stack_size"], 1);
    assert_eq!(items[0]["raw_kind"], "jpeg");
    assert_eq!(
        items[0]["favorite"], false,
        "un asset mai votato non è preferito"
    );
}

/// `AssetView` è condiviso, ma `favorite` è per chiamante (Task 10 fase-10):
/// la timeline deve risolverlo dal set del chiamante, non lasciarlo sempre
/// `false`.
#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn timeline_page_resolves_the_callers_favorite_on_each_tile() {
    let server = TestServer::start().await;
    let (folder, _) = seed_library(&server).await;
    index_photo(&server, folder, "loved.jpg", 2024, 6, 1).await;
    index_photo(&server, folder, "plain.jpg", 2024, 6, 2).await;

    let before = server
        .client
        .get(server.url("/api/v1/timeline?bucket=2024-06"))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    let items = before["assets"].as_array().unwrap();
    let loved_id = items.iter().find(|a| a["filename"] == "loved.jpg").unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(items.iter().all(|a| a["favorite"] == false));

    server
        .client
        .put(server.url(&format!("/api/v1/assets/{loved_id}/flags")))
        .json(&json!({ "rating": null, "pick": "none", "color_label": null, "favorite": true }))
        .send()
        .await
        .unwrap();

    let after = server
        .client
        .get(server.url("/api/v1/timeline?bucket=2024-06"))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    let after_items = after["assets"].as_array().unwrap();
    for item in after_items {
        let expected = item["id"] == loved_id;
        assert_eq!(
            item["favorite"], expected,
            "solo l'asset marcato preferito deve tornare favorite=true"
        );
    }
}

/// `GET /assets/{id}` (pannello informazioni del lightbox, spec fase-10
/// §7bis.1) deve anch'esso risolvere `favorite` del chiamante, non solo la
/// pagina della timeline.
#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn get_single_asset_resolves_the_callers_favorite() {
    let server = TestServer::start().await;
    let (folder, _) = seed_library(&server).await;
    index_photo(&server, folder, "solo.jpg", 2024, 6, 1).await;
    let page = server
        .client
        .get(server.url("/api/v1/timeline?bucket=2024-06"))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    let asset_id = page["assets"][0]["id"].as_str().unwrap().to_owned();

    let before = server
        .client
        .get(server.url(&format!("/api/v1/assets/{asset_id}")))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    assert_eq!(before["favorite"], false);

    server
        .client
        .put(server.url(&format!("/api/v1/assets/{asset_id}/flags")))
        .json(&json!({ "rating": null, "pick": "none", "color_label": null, "favorite": true }))
        .send()
        .await
        .unwrap();

    let after = server
        .client
        .get(server.url(&format!("/api/v1/assets/{asset_id}")))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    assert_eq!(after["favorite"], true);
}

/// Fase 11 Task 8 (§19.2 campi 6-9, sezione "SCATTO" del lightbox):
/// `full_exif` è additivo su `GET /assets/{id}` soltanto — mai su
/// `/timeline`, un giro di query in più per riga che nessuna pagina della
/// timeline legge.
#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn get_single_asset_includes_full_exif_but_timeline_page_never_does() {
    let server = TestServer::start().await;
    let (folder, _) = seed_library(&server).await;
    index_photo(&server, folder, "con-exif.jpg", 2024, 6, 1).await;
    let page = server
        .client
        .get(server.url("/api/v1/timeline?bucket=2024-06"))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    let asset_id = page["assets"][0]["id"].as_str().unwrap().to_owned();
    assert!(
        page["assets"][0].get("full_exif").is_none(),
        "full_exif must never appear on a timeline page"
    );

    let before = server
        .client
        .get(server.url(&format!("/api/v1/assets/{asset_id}")))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    assert!(before.get("full_exif").is_none(), "no asset_exif row yet");

    sqlx::query(
        "INSERT INTO asset_exif (asset_id, raw, camera_make, camera_model, lens, iso, \
                                  f_number, exposure, focal_length) \
         VALUES ($1, '{}', 'Sony', 'Sony A7 IV', 'FE 24-70mm f/2.8', 400, 3.5, '1/250', 70.0)",
    )
    .bind(uuid::Uuid::parse_str(&asset_id).unwrap())
    .execute(server.db.pool())
    .await
    .unwrap();

    let after = server
        .client
        .get(server.url(&format!("/api/v1/assets/{asset_id}")))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    assert_eq!(after["full_exif"]["camera_make"], "Sony");
    assert_eq!(after["full_exif"]["lens"], "FE 24-70mm f/2.8");
    assert_eq!(after["full_exif"]["iso"], 400);
    assert_eq!(after["full_exif"]["f_number"], 3.5);
    assert_eq!(after["full_exif"]["exposure"], "1/250");
    assert_eq!(after["full_exif"]["focal_length"], 70.0);
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn probing_someone_elses_library_geometry_is_forbidden() {
    let server = TestServer::start().await;
    let (_, library) = seed_library(&server).await;
    seed_user(&server, "luca").await;

    let client = reqwest::Client::builder()
        .cookie_store(true)
        .default_headers(client_headers())
        .build()
        .unwrap();
    let login = client
        .post(server.url("/api/v1/auth/login"))
        .json(&json!({
            "username": "luca",
            "password": "correct horse battery staple"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(login.status(), 200);

    let response = client
        .get(server.url(&format!("/api/v1/timeline/geometry?library={library}")))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 403);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["type"], "keeppix/forbidden");
}

/// Decodifica il formato binario di `/timeline/geometry`: intestazione da 8
/// byte (versione, conteggio) poi record da 6 byte (w, h, month), tutto LE.
#[allow(clippy::unwrap_used)]
fn decode_geometry(body: &[u8]) -> Vec<(u16, u16, u16)> {
    assert!(body.len() >= 8, "intestazione da 8 byte assente");
    let version = u32::from_le_bytes(body[0..4].try_into().unwrap());
    assert_eq!(version, 1, "versione del formato binario");
    let count = u32::from_le_bytes(body[4..8].try_into().unwrap()) as usize;
    assert_eq!(body.len(), 8 + count * 6);
    (0..count)
        .map(|i| {
            let base = 8 + i * 6;
            let w = u16::from_le_bytes(body[base..base + 2].try_into().unwrap());
            let h = u16::from_le_bytes(body[base + 2..base + 4].try_into().unwrap());
            let m = u16::from_le_bytes(body[base + 4..base + 6].try_into().unwrap());
            (w, h, m)
        })
        .collect()
}

#[allow(clippy::unwrap_used, clippy::too_many_arguments)]
async fn index_photo_sized(
    server: &TestServer,
    folder: FolderId,
    name: &str,
    y: i32,
    m: u32,
    d: u32,
    width: i32,
    height: i32,
) {
    let assets = AssetRepo::new(&server.db);
    let taken = Utc.with_ymd_and_hms(y, m, d, 12, 0, 0).unwrap();
    let a = assets
        .upsert_discovered(NewAsset {
            folder_id: folder,
            filename: AssetName::parse(name).unwrap(),
            size_bytes: 10,
            mtime: taken,
            inode: Some(1),
            kind: AssetKind::Image,
        })
        .await
        .unwrap()
        .unwrap();
    assets
        .set_indexed(a.id, taken, width, height)
        .await
        .unwrap();
}

#[allow(clippy::unwrap_used, clippy::expect_used)]
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
async fn admin_ctx(server: &TestServer) -> AuthContext {
    let username = Username::parse("giovanni").unwrap();
    let (user, _) = UserRepo::new(&server.db)
        .find_by_username(&username)
        .await
        .unwrap()
        .expect("admin");
    AuthContext::user(user.id, SystemRole::Admin)
}

#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn seed_library(server: &TestServer) -> (FolderId, keeppix_domain::LibraryId) {
    setup(server).await;
    let ctx = admin_ctx(server).await;
    let library = LibraryRepo::new(&server.db)
        .create(
            &ctx,
            NewLibrary {
                name: "Foto".to_owned(),
                owner_id: ctx.user_id().unwrap(),
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
    (folder.id, library.id)
}

#[allow(clippy::unwrap_used)]
async fn index_photo(server: &TestServer, folder: FolderId, name: &str, y: i32, m: u32, d: u32) {
    index_photo_at(
        server,
        folder,
        name,
        Utc.with_ymd_and_hms(y, m, d, 12, 0, 0).unwrap(),
    )
    .await;
}

#[allow(clippy::unwrap_used)]
async fn index_photo_at(
    server: &TestServer,
    folder: FolderId,
    name: &str,
    taken: chrono::DateTime<Utc>,
) {
    let assets = AssetRepo::new(&server.db);
    let a = assets
        .upsert_discovered(NewAsset {
            folder_id: folder,
            filename: AssetName::parse(name).unwrap(),
            size_bytes: 10,
            mtime: taken,
            inode: Some(1),
            kind: AssetKind::Image,
        })
        .await
        .unwrap()
        .unwrap();
    assets.set_indexed(a.id, taken, 1, 1).await.unwrap();
}

#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn seed_user(server: &TestServer, username: &str) {
    let password = Password::parse("correct horse battery staple").unwrap();
    UserRepo::new(&server.db)
        .create(
            &admin_ctx(server).await,
            NewUser {
                username: Username::parse(username).unwrap(),
                email: None,
                display_name: username.to_owned(),
                password_hash: hash_password(&password).unwrap().as_str().to_owned(),
                role: SystemRole::User,
            },
        )
        .await
        .unwrap();
}

/// Fase 11 Task 7 (SP-3 §11, dimensione "Fotocamera") — campo additivo su
/// `AssetView`, condiviso da `enrich_views` fra `/timeline` e `/search`.
/// Server senza pgvector di proposito: dimostra che `tags`/`faces` restano
/// `[]` per grazia (nessun errore) quando lo schema AI non esiste affatto —
/// `camera_model` non dipende da pgvector, `asset_exif` è schema core.
#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn timeline_page_includes_camera_model_and_empty_tags_faces_without_vector() {
    let server = TestServer::start().await;
    let (folder, _) = seed_library(&server).await;
    index_photo(&server, folder, "with-camera.jpg", 2024, 6, 1).await;
    index_photo(&server, folder, "no-exif.jpg", 2024, 6, 2).await;

    let before = server
        .client
        .get(server.url("/api/v1/timeline?bucket=2024-06"))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    let items = before["assets"].as_array().unwrap();
    let with_camera_id = items
        .iter()
        .find(|a| a["filename"] == "with-camera.jpg")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(
        items
            .iter()
            .all(|a| a["tags"].as_array().unwrap().is_empty()
                && a["faces"].as_array().unwrap().is_empty()),
        "tags/faces are always present arrays, empty here — no pgvector, no AI schema"
    );
    assert!(items.iter().all(|a| a.get("camera_model").is_none()));

    sqlx::query("INSERT INTO asset_exif (asset_id, camera_model) VALUES ($1, $2)")
        .bind(uuid::Uuid::parse_str(&with_camera_id).unwrap())
        .bind("FUJIFILM X-T5")
        .execute(server.db.pool())
        .await
        .unwrap();

    let after = server
        .client
        .get(server.url("/api/v1/timeline?bucket=2024-06"))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    let after_items = after["assets"].as_array().unwrap();
    let with_camera = after_items
        .iter()
        .find(|a| a["id"] == with_camera_id)
        .unwrap();
    assert_eq!(with_camera["camera_model"], "FUJIFILM X-T5");
    let no_exif = after_items
        .iter()
        .find(|a| a["filename"] == "no-exif.jpg")
        .unwrap();
    assert!(no_exif.get("camera_model").is_none());
}

/// Fase 11 Task 7 (SP-3 §11, dimensioni "Tag"/"Categorie"/"Persone") — la
/// stessa `enrich_views` che il test sopra dimostra graziosa senza
/// pgvector deve popolare per davvero quando lo schema AI c'è.
#[tokio::test]
#[allow(clippy::unwrap_used, clippy::too_many_lines)]
async fn timeline_page_includes_confirmed_tags_and_faces_with_vector() {
    use keeppix_db::{AssetTagRepo, FaceRepo, NewDetectedFace, PersonRepo};
    use keeppix_domain::{FaceBBox, TagKind};

    let server = TestServer::start_with_vector().await;
    let (folder, _) = seed_library(&server).await;
    index_photo(&server, folder, "tagged.jpg", 2024, 6, 1).await;

    let before = server
        .client
        .get(server.url("/api/v1/timeline?bucket=2024-06"))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    let asset_id: keeppix_domain::AssetId =
        before["assets"][0]["id"].as_str().unwrap().parse().unwrap();

    let ctx = admin_ctx(&server).await;
    let category = keeppix_db::TagRepo::new(&server.db)
        .create(
            &ctx,
            keeppix_db::NewTag {
                name: "Viaggi".to_owned(),
                kind: TagKind::Category,
                parent_id: None,
                prompt: None,
                color: None,
                threshold: None,
                embedding: None,
                model_version: None,
            },
        )
        .await
        .unwrap()
        .id;
    let tag = keeppix_db::TagRepo::new(&server.db)
        .create(
            &ctx,
            keeppix_db::NewTag {
                name: "Montagna".to_owned(),
                kind: TagKind::Tag,
                parent_id: Some(category),
                prompt: None,
                color: Some("#336699".to_owned()),
                threshold: None,
                embedding: None,
                model_version: None,
            },
        )
        .await
        .unwrap()
        .id;
    AssetTagRepo::new(&server.db)
        .assign(&ctx, tag, asset_id)
        .await
        .unwrap();

    let person = PersonRepo::new(&server.db)
        .create(Some(keeppix_domain::PersonName::parse("Marta").unwrap()))
        .await
        .unwrap();
    let face = FaceRepo::new(&server.db)
        .insert_detected(NewDetectedFace {
            asset_id,
            bbox: FaceBBox {
                x: 0.1,
                y: 0.1,
                w: 0.2,
                h: 0.2,
            },
            landmarks: None,
            embedding: None,
            detect_score: 0.95,
            quality: Some(0.8),
            model_version: "scrfd-500mf+arcface".to_owned(),
        })
        .await
        .unwrap();
    FaceRepo::new(&server.db)
        .assign(&ctx, face.id, person.id)
        .await
        .unwrap();

    let after = server
        .client
        .get(server.url("/api/v1/timeline?bucket=2024-06"))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    let item = &after["assets"][0];

    let tags = item["tags"].as_array().unwrap();
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0]["name"], "Montagna");
    assert_eq!(tags[0]["color"], "#336699");
    assert_eq!(tags[0]["category_id"], category.to_string());

    let faces = item["faces"].as_array().unwrap();
    assert_eq!(faces.len(), 1);
    assert_eq!(faces[0]["person_id"], person.id.to_string());
    assert_eq!(faces[0]["person_name"], "Marta");
}
