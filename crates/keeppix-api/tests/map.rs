mod harness;

use chrono::{TimeZone as _, Utc};
use harness::TestServer;
use keeppix_db::{FolderRepo, LibraryRepo, NewMapRegion, PlaceRepo, RegionRepo, UserRepo};
use keeppix_domain::{AuthContext, GeoPoint, NewLibrary, Place, SystemRole, UserId, Username};
use serde_json::json;

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn clusters_require_a_session() {
    let server = TestServer::start().await;
    let response = reqwest::Client::new()
        .get(server.url(
            "/api/v1/map/clusters?bbox=-180,-90,180,90&zoom=10&scope=library&scope_id=00000000-0000-0000-0000-000000000001",
        ))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 401);
    assert_eq!(
        response.headers()["content-type"],
        "application/problem+json"
    );
    assert_eq!(
        response.json::<serde_json::Value>().await.unwrap()["type"],
        "keeppix/unauthenticated"
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn clusters_return_the_scoped_map_payload() {
    let server = TestServer::start().await;
    let user = setup(&server).await;
    let seeded = seed_library_with_point(&server, user).await;
    let response = server
        .client
        .get(server.url(&format!(
            "/api/v1/map/clusters?bbox=10,40,15,45&zoom=15&scope=library&scope_id={}",
            seeded.library_id
        )))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    let rows = body.as_array().expect("array response");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["lat"], 41.9);
    assert_eq!(rows[0]["lon"], 12.5);
    assert_eq!(rows[0]["count"], 1);
    assert_eq!(rows[0]["clustered"], false);
    assert!(rows[0]["cover_asset_id"].as_str().is_some());
}

/// The cluster popover must be able to navigate to the folder and show
/// the place label without a second request: `folder_id` and
/// `place_label` are additive on `MapClusterView`.
#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn cluster_carries_enough_data_to_open_its_folder_without_a_second_request() {
    let server = TestServer::start().await;
    let user = setup(&server).await;
    let seeded = seed_library_with_point(&server, user).await;
    let response = server
        .client
        .get(server.url(&format!(
            "/api/v1/map/clusters?bbox=10,40,15,45&zoom=15&scope=library&scope_id={}",
            seeded.library_id
        )))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    let rows = body.as_array().expect("array response");
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0]["folder_id"].as_str().unwrap(),
        seeded.folder_id.to_string(),
        "destination id to open the folder from the popover"
    );
    assert_eq!(
        rows[0]["place_label"], "Roma",
        "human-readable place label, from reverse geocoding"
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn invalid_map_queries_are_problem_json_400s() {
    let server = TestServer::start().await;
    let user = setup(&server).await;
    let library = seed_library_with_point(&server, user).await.library_id;
    let cases = [
        (
            format!(
                "/api/v1/map/clusters?bbox=10,50,15,45&zoom=10&scope=library&scope_id={library}"
            ),
            "keeppix/invalid-bbox",
        ),
        (
            format!(
                "/api/v1/map/clusters?bbox=10,40,15,45&zoom=31&scope=library&scope_id={library}"
            ),
            "keeppix/invalid-zoom",
        ),
        (
            format!(
                "/api/v1/map/clusters?bbox=10,40,15,45&zoom=10&scope=timeline&scope_id={library}"
            ),
            "keeppix/invalid-map-scope",
        ),
    ];

    for (path, expected_type) in cases {
        let response = server.client.get(server.url(&path)).send().await.unwrap();
        assert_eq!(response.status(), 400, "{path}");
        assert_eq!(
            response.headers()["content-type"],
            "application/problem+json"
        );
        let body: serde_json::Value = response.json().await.unwrap();
        assert_eq!(body["type"], expected_type, "{path}");
    }
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn a_foreign_scope_id_is_forbidden_not_not_found() {
    let server = TestServer::start().await;
    setup(&server).await;
    let response = server
        .client
        .get(server.url(
            "/api/v1/map/clusters?bbox=-180,-90,180,90&zoom=10&scope=library&scope_id=00000000-0000-0000-0000-000000000001",
        ))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 403);
    assert_eq!(
        response.headers()["content-type"],
        "application/problem+json"
    );
    assert_eq!(
        response.json::<serde_json::Value>().await.unwrap()["type"],
        "keeppix/forbidden"
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn deleted_region_returns_clean_404_after_range_serving() {
    let server = TestServer::start().await;
    let user = setup(&server).await;
    let ctx = AuthContext::user(user, SystemRole::Admin);
    let region = RegionRepo::new(&server.db)
        .begin_download(
            &ctx,
            NewMapRegion {
                id: "IT".to_owned(),
                label: "Italia".to_owned(),
                size_bytes: 10,
                version: "2026-08".to_owned(),
                source_url: "https://build.protomaps.com/IT.pmtiles".to_owned(),
                checksum_sha256: "ab".repeat(32),
            },
        )
        .await
        .unwrap();
    RegionRepo::new(&server.db)
        .mark_available("IT", region.download_generation)
        .await
        .unwrap();
    tokio::fs::create_dir_all(server.data_dir.join("maps"))
        .await
        .unwrap();
    tokio::fs::write(server.data_dir.join(&region.file_path), b"0123456789")
        .await
        .unwrap();

    let ranged = server
        .client
        .get(server.url("/api/v1/map/tiles/IT/0/0/0"))
        .header(reqwest::header::RANGE, "bytes=2-5")
        .send()
        .await
        .unwrap();
    assert_eq!(ranged.status(), 206);
    assert_eq!(ranged.headers()["content-range"], "bytes 2-5/10");
    assert_eq!(ranged.bytes().await.unwrap().as_ref(), b"2345");

    let deleted = server
        .client
        .delete(server.url("/api/v1/map/regions/IT"))
        .send()
        .await
        .unwrap();
    assert_eq!(deleted.status(), 204);

    let missing = server
        .client
        .get(server.url("/api/v1/map/tiles/IT/0/0/0"))
        .send()
        .await
        .unwrap();
    assert_eq!(missing.status(), 404);
    assert_eq!(
        missing.headers()["content-type"],
        "application/problem+json"
    );
    assert_eq!(
        missing.json::<serde_json::Value>().await.unwrap()["type"],
        "keeppix/not-found"
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn region_enqueue_rejects_a_non_allowlisted_source() {
    let server = TestServer::start().await;
    setup(&server).await;

    let response = server
        .client
        .post(server.url("/api/v1/map/regions"))
        .json(&json!({
            "id": "IT",
            "label": "Italia",
            "size_bytes": 10,
            "version": "2026-08",
            "source_url": "https://127.0.0.1/private",
            "checksum_sha256": "ab".repeat(32)
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 422);
    assert_eq!(
        response.json::<serde_json::Value>().await.unwrap()["type"],
        "keeppix/region-source-not-allowed"
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn cancelling_a_region_removes_its_partial_file() {
    let server = TestServer::start().await;
    let user = setup(&server).await;
    let ctx = AuthContext::user(user, SystemRole::Admin);
    let region = RegionRepo::new(&server.db)
        .begin_download(
            &ctx,
            NewMapRegion {
                id: "IT".to_owned(),
                label: "Italia".to_owned(),
                size_bytes: 10,
                version: "2026-08".to_owned(),
                source_url: "https://build.protomaps.com/IT.pmtiles".to_owned(),
                checksum_sha256: "ab".repeat(32),
            },
        )
        .await
        .unwrap();
    tokio::fs::create_dir_all(server.data_dir.join("maps"))
        .await
        .unwrap();
    let partial = server.data_dir.join(format!("{}.part", region.file_path));
    tokio::fs::write(&partial, b"partial").await.unwrap();

    let response = server
        .client
        .post(server.url("/api/v1/map/regions/IT/cancel"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 204);
    assert!(!partial.exists());
    let region = RegionRepo::new(&server.db).find(&ctx, "IT").await.unwrap();
    assert_eq!(region.status, keeppix_db::RegionStatus::Error);
    assert_eq!(region.last_error.as_deref(), Some("Download cancelled"));
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn failed_cancel_cleanup_can_be_retried() {
    let server = TestServer::start().await;
    let user = setup(&server).await;
    let ctx = AuthContext::user(user, SystemRole::Admin);
    let region = RegionRepo::new(&server.db)
        .begin_download(
            &ctx,
            NewMapRegion {
                id: "IT".to_owned(),
                label: "Italia".to_owned(),
                size_bytes: 10,
                version: "2026-08".to_owned(),
                source_url: "https://build.protomaps.com/IT.pmtiles".to_owned(),
                checksum_sha256: "ab".repeat(32),
            },
        )
        .await
        .unwrap();
    let partial = server.data_dir.join(format!("{}.part", region.file_path));
    tokio::fs::create_dir_all(&partial).await.unwrap();

    let failed = server
        .client
        .post(server.url("/api/v1/map/regions/IT/cancel"))
        .send()
        .await
        .unwrap();

    assert_eq!(failed.status(), 500);
    let region = RegionRepo::new(&server.db).find(&ctx, "IT").await.unwrap();
    assert_eq!(region.status, keeppix_db::RegionStatus::Downloading);

    tokio::fs::remove_dir(&partial).await.unwrap();
    let retried = server
        .client
        .post(server.url("/api/v1/map/regions/IT/cancel"))
        .send()
        .await
        .unwrap();

    assert_eq!(retried.status(), 204);
    let region = RegionRepo::new(&server.db).find(&ctx, "IT").await.unwrap();
    assert_eq!(region.status, keeppix_db::RegionStatus::Error);
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn catalog_requires_a_session_and_then_lists_thirty_five_countries() {
    let server = TestServer::start().await;

    let anonymous = server
        .client
        .get(server.url("/api/v1/map/regions/catalog"))
        .send()
        .await
        .unwrap();
    assert_eq!(anonymous.status(), 401);

    setup(&server).await;
    let response = server
        .client
        .get(server.url("/api/v1/map/regions/catalog"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    let entries = body.as_array().unwrap();
    assert_eq!(entries.len(), 35);
    assert!(
        entries
            .iter()
            .any(|entry| entry["id"] == "FR" && entry["label"] == "Francia")
    );
    assert!(entries.iter().all(|entry| entry["approx_size_bytes"]
        .as_i64()
        .is_some_and(|size| size > 0)));
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn downloading_an_unknown_catalog_id_is_a_clean_404() {
    let server = TestServer::start().await;
    setup(&server).await;

    let response = server
        .client
        .post(server.url("/api/v1/map/regions/catalog/atlantis"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 404);
    assert_eq!(
        response.json::<serde_json::Value>().await.unwrap()["type"],
        "keeppix/unknown-region-catalog-id"
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn downloading_a_catalog_entry_queues_an_extraction() {
    let server = TestServer::start().await;
    setup(&server).await;

    let response = server
        .client
        .post(server.url("/api/v1/map/regions/catalog/FR"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 202);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["id"], "FR");
    assert_eq!(body["status"], "downloading");

    let kind: String = sqlx::query_scalar("SELECT kind FROM jobs WHERE payload->>'region_id' = 'FR'")
        .fetch_one(server.db.pool())
        .await
        .unwrap();
    assert_eq!(kind, "extract_map_region");
}

/// Regression test for the acquisition-aware dedup key fix in
/// `retire_region_job`: cancelling a catalog-triggered (extraction) region
/// must retire the `map-region-extract:*` job, not silently miss it by
/// guessing the manual-download flow's `map-region:*` key.
#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn cancelling_a_catalog_download_retires_the_extraction_job_not_a_download_job() {
    let server = TestServer::start().await;
    setup(&server).await;
    server
        .client
        .post(server.url("/api/v1/map/regions/catalog/FR"))
        .send()
        .await
        .unwrap();

    let response = server
        .client
        .post(server.url("/api/v1/map/regions/FR/cancel"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 204);

    let status: String = sqlx::query_scalar("SELECT status FROM jobs WHERE payload->>'region_id' = 'FR'")
        .fetch_one(server.db.pool())
        .await
        .unwrap();
    assert_eq!(status, "failed");
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn malformed_region_paths_are_problem_json_400s() {
    let server = TestServer::start().await;
    setup(&server).await;

    for (method, path) in [
        (reqwest::Method::POST, "/api/v1/map/regions/%FF/cancel"),
        (reqwest::Method::DELETE, "/api/v1/map/regions/%FF"),
    ] {
        let response = server
            .client
            .request(method, server.url(path))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 400, "{path}");
        assert_eq!(
            response.headers()["content-type"],
            "application/problem+json",
            "{path}"
        );
        assert_eq!(
            response.json::<serde_json::Value>().await.unwrap()["type"],
            "keeppix/invalid-region-path",
            "{path}"
        );
    }
}

#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn setup(server: &TestServer) -> UserId {
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
    UserRepo::new(&server.db)
        .find_by_username(&Username::parse("giovanni").unwrap())
        .await
        .unwrap()
        .expect("admin")
        .0
        .id
}

struct SeededMapLibrary {
    library_id: uuid::Uuid,
    folder_id: uuid::Uuid,
}

#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn seed_library_with_point(server: &TestServer, user: UserId) -> SeededMapLibrary {
    let library = LibraryRepo::new(&server.db)
        .create(
            &AuthContext::user(user, SystemRole::Admin),
            NewLibrary {
                name: "Map".to_owned(),
                owner_id: user,
                root_path: server.photos_root.join("map"),
                exclude_patterns: Vec::new(),
            },
        )
        .await
        .unwrap();
    let folder = FolderRepo::new(&server.db)
        .ensure_path(library.id, &["italy"])
        .await
        .unwrap();
    let place = Place {
        id: 3_169_070,
        name: "Roma".to_owned(),
        ascii_name: "Roma".to_owned(),
        country_code: Some("IT".to_owned()),
        admin1: None,
        admin2: None,
        location: GeoPoint {
            lat: 41.9,
            lon: 12.5,
        },
        population: 2_318_895,
    };
    PlaceRepo::new(&server.db).upsert(&place).await.unwrap();
    sqlx::query(
        "INSERT INTO assets \
             (id, folder_id, filename, size_bytes, mtime, kind, status, taken_at_utc, \
              location, place_id) \
         VALUES ($1, $2, 'rome.jpg', 1, $3, 'image', 'indexed', $3, \
                 ST_SetSRID(ST_MakePoint(12.5, 41.9), 4326)::geography, $4)",
    )
    .bind(uuid::Uuid::now_v7())
    .bind(folder.id.as_uuid())
    .bind(Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap())
    .bind(place.id)
    .execute(server.db.pool())
    .await
    .unwrap();
    SeededMapLibrary {
        library_id: library.id.as_uuid(),
        folder_id: folder.id.as_uuid(),
    }
}
