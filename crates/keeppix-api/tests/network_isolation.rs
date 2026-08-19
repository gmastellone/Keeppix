//! Verifies that ingest → geocode → assign-location makes zero outbound
//! network connections by poisoning `HTTP_PROXY`/`HTTPS_PROXY` before
//! running the flow. `reqwest` honours proxy env vars, so any outbound
//! call would fail through the unreachable proxy at `127.0.0.1:1`.
//!
//! The test client is built *before* the env vars are set, so its
//! connections to the local test server are unaffected. Server-side code
//! that creates new `reqwest` clients (only `keeppix-jobs::regions`) would
//! pick up the poisoned proxy and fail.
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
    // Build server and client first — reqwest reads proxy env at client
    // build time, so the pre-built client is unaffected by the poison.
    let server = TestServer::start().await;
    setup_admin(&server).await;

    // Poison: any *new* outbound HTTP from the server process fails.
    unsafe {
        std::env::set_var("HTTP_PROXY", "http://127.0.0.1:1");
        std::env::set_var("HTTPS_PROXY", "http://127.0.0.1:1");
    }

    let root = server
        .photos_root
        .join(format!("net-iso-{}", uuid::Uuid::now_v7().simple()));
    fs::create_dir_all(&root).unwrap();
    fs::copy(tiny_fixture_path(), root.join("photo.jpg")).unwrap();

    let library_id = create_library(&server, "NetIso", &root).await;
    let deadline = Instant::now() + Duration::from_secs(90);
    scan_and_wait(&server, &library_id, 1, deadline).await;

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

    let suggest = server
        .client
        .get(server.url("/api/v1/places/suggest?q=Roma&near_user=true"))
        .send()
        .await
        .unwrap();
    assert!(suggest.status().is_success());

    // Cleanup
    unsafe {
        std::env::remove_var("HTTP_PROXY");
        std::env::remove_var("HTTPS_PROXY");
    }
}
