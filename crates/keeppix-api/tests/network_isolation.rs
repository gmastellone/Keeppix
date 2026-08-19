//! Verifies that ingest → geocode → assign-location makes zero outbound
//! network connections. The test poisons HTTP/HTTPS proxy env vars so any
//! outbound `reqwest` or `hyper` call would fail through an unreachable
//! proxy, then runs the full flow and asserts success.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod harness;
mod journey;

use std::fs;
use std::time::{Duration, Instant};

use harness::TestServer;
use journey::{create_library, scan_and_wait, setup_admin, tiny_fixture_path};
use serde_json::{Value, json};

#[tokio::test]
async fn ingest_geocode_assign_makes_no_outbound_connections() {
    // Poison proxy env so any outbound HTTP attempt fails immediately.
    unsafe {
        std::env::set_var("HTTP_PROXY", "http://127.0.0.1:1");
        std::env::set_var("HTTPS_PROXY", "http://127.0.0.1:1");
    }

    let server = TestServer::start().await;
    setup_admin(&server).await;

    let root = server
        .photos_root
        .join(format!("net-iso-{}", uuid::Uuid::now_v7().simple()));
    fs::create_dir_all(&root).unwrap();
    fs::copy(tiny_fixture_path(), root.join("photo.jpg")).unwrap();

    // Ingest (scan)
    let library_id = create_library(&server, "NetIso", &root).await;
    let deadline = Instant::now() + Duration::from_secs(90);
    scan_and_wait(&server, &library_id, 1, deadline).await;

    // Find the asset via timeline
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

    // Assign location (geocode + pin) — purely local DB operation
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

    // Suggest places (local GeoNames, no outbound)
    let suggest = server
        .client
        .get(server.url("/api/v1/places/suggest?q=Roma&near_user=true"))
        .send()
        .await
        .unwrap();
    // 200 or empty is fine; the point is it didn't fail through the proxy.
    assert!(suggest.status().is_success());

    // Cleanup proxy env
    unsafe {
        std::env::remove_var("HTTP_PROXY");
        std::env::remove_var("HTTPS_PROXY");
    }
}
