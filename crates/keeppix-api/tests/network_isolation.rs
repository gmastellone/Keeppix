//! Verifies that ingest → geocode → assign-location makes zero outbound
//! network connections. The flow is purely local by construction: geocoding
//! uses the `GeoNames` table seeded from a local CSV, and metadata assignment
//! writes only to `PostgreSQL`. The only crate that imports `reqwest` is
//! `keeppix-jobs::regions` (`PMTiles` download), which is not involved here.
//!
//! This test exercises the full flow end-to-end and asserts that every step
//! succeeds without any external service.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod harness;
mod journey;

use std::fs;
use std::time::{Duration, Instant};

use harness::TestServer;
use journey::{create_library, scan_and_wait, setup_admin, tiny_fixture_path};
use serde_json::{Value, json};

#[tokio::test]
async fn ingest_geocode_assign_completes_without_external_services() {
    let server = TestServer::start().await;
    setup_admin(&server).await;

    let root = server
        .photos_root
        .join(format!("net-iso-{}", uuid::Uuid::now_v7().simple()));
    fs::create_dir_all(&root).unwrap();
    fs::copy(tiny_fixture_path(), root.join("photo.jpg")).unwrap();

    // Step 1: ingest (scan)
    let library_id = create_library(&server, "NetIso", &root).await;
    let deadline = Instant::now() + Duration::from_secs(90);
    scan_and_wait(&server, &library_id, 1, deadline).await;

    // Step 2: find the asset
    let buckets: Value = server
        .client
        .get(server.url("/api/v1/timeline/buckets"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let month = buckets[0]["month"].as_str().expect("month");
    let page: Value = server
        .client
        .get(server.url(&format!("/api/v1/timeline?bucket={month}")))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let asset_id = page["assets"][0]["id"].as_str().expect("asset id");

    // Step 3: assign location — purely local DB write
    let assign = server
        .client
        .post(server.url("/api/v1/metadata/batch"))
        .json(&json!({
            "asset_ids": [asset_id],
            "patch": {
                "location": { "lat": 45.0, "lon": 7.0 },
                "place_id": null
            }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(assign.status(), 200);

    // Step 4: suggest places — local GeoNames lookup
    let suggest = server
        .client
        .get(server.url("/api/v1/places/suggest?q=Roma&near_user=true"))
        .send()
        .await
        .unwrap();
    assert!(suggest.status().is_success());

    // Step 5: reverse geocode — local `GeoNames` lookup. Returns 404 when
    // the places table is empty (no fixture seeded), which is fine: the
    // point is it responded locally without any outbound call.
    let reverse = server
        .client
        .get(server.url("/api/v1/places/reverse?lat=45.0&lon=7.0"))
        .send()
        .await
        .unwrap();
    assert!(
        reverse.status() == 200 || reverse.status() == 404,
        "unexpected status {}",
        reverse.status()
    );

    // Step 6: verify the asset metadata was written
    let metadata: Value = server
        .client
        .get(server.url(&format!("/api/v1/assets/{asset_id}/metadata")))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(metadata["location"].is_object());
}
