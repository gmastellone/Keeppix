//! Guest upload through a public share link.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod harness;
mod journey;

use std::fs;
use std::time::{Duration, Instant};

use harness::TestServer;
use journey::{
    create_library, create_share_link, create_share_link_from, scan_and_wait, setup_admin,
    share_client, tiny_fixture_path,
};
use serde_json::json;

async fn folder_share(server: &TestServer) -> (String, String) {
    let deadline = Instant::now() + Duration::from_secs(60);
    setup_admin(server).await;
    let root = server
        .photos_root
        .join(format!("guest-up-{}", uuid::Uuid::now_v7().simple()));
    fs::create_dir_all(&root).unwrap();
    fs::copy(tiny_fixture_path(), root.join("host.jpg")).unwrap();
    let library_id = create_library(server, "GuestUp", &root).await;
    scan_and_wait(server, &library_id, 1, deadline).await;
    let folder_id: (uuid::Uuid,) =
        sqlx::query_as("SELECT id FROM folders WHERE library_id = $1 AND parent_id IS NULL")
            .bind(uuid::Uuid::parse_str(&library_id).unwrap())
            .fetch_one(server.db.pool())
            .await
            .unwrap();
    (library_id, folder_id.0.to_string())
}

#[tokio::test]
async fn a_guest_can_upload_a_file_through_a_share_link() {
    let server = TestServer::start().await;
    let (_library_id, folder_id) = folder_share(&server).await;
    let token = create_share_link_from(
        &server,
        json!({
            "object_type": "folder",
            "object_id": folder_id,
            "allow_upload": true,
        }),
    )
    .await;

    let bytes = fs::read(tiny_fixture_path()).unwrap();
    let guest = share_client(&token);
    let uploaded = guest
        .post(server.url(&format!("/api/v1/share/{token}/uploads?filename=guest.jpg")))
        .header("content-type", "application/octet-stream")
        .body(bytes.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(uploaded.status(), 201);
    let body: serde_json::Value = uploaded.json().await.unwrap();
    let upload_id = body["id"].as_str().expect("upload id");

    let queued: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM guest_upload_queue WHERE id = $1 AND filename = 'guest.jpg'",
    )
    .bind(uuid::Uuid::parse_str(upload_id).unwrap())
    .fetch_one(server.db.pool())
    .await
    .unwrap();
    assert_eq!(queued, 1);

    let guests: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM assets WHERE filename = 'guest.jpg' AND uploaded_by_guest",
    )
    .fetch_one(server.db.pool())
    .await
    .unwrap();
    assert_eq!(guests, 1);

    let on_disk = fs::read_dir(&server.photos_root)
        .unwrap()
        .filter_map(Result::ok)
        .any(|entry| entry.path().join("guest.jpg").is_file());
    assert!(
        on_disk,
        "the uploaded bytes must land in the destination folder"
    );

    let listed = guest
        .get(server.url(&format!("/api/v1/share/{token}/assets")))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    let names: Vec<&str> = listed["assets"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|a| a["filename"].as_str())
        .collect();
    assert!(
        !names.contains(&"guest.jpg"),
        "pending guest files must stay out of the public listing"
    );
}

#[tokio::test]
async fn a_share_link_without_upload_rejects_guest_files() {
    let server = TestServer::start().await;
    let (_library_id, folder_id) = folder_share(&server).await;
    let token = create_share_link(&server, "folder", &folder_id, None).await;
    let guest = share_client(&token);
    let uploaded = guest
        .post(server.url(&format!("/api/v1/share/{token}/uploads?filename=guest.jpg")))
        .header("content-type", "application/octet-stream")
        .body(fs::read(tiny_fixture_path()).unwrap())
        .send()
        .await
        .unwrap();
    assert_eq!(uploaded.status(), 403);
}

/// The quota is applied WHILE reading, not after: a 100-byte body on a
/// 10-byte quota must stop before the file is accepted.
#[tokio::test]
async fn the_quota_is_enforced_while_uploading_not_after() {
    let server = TestServer::start().await;
    let (_library_id, folder_id) = folder_share(&server).await;
    let token = create_share_link_from(
        &server,
        json!({
            "object_type": "folder",
            "object_id": folder_id,
            "allow_upload": true,
            "upload_quota_bytes": 10,
        }),
    )
    .await;

    let guest = share_client(&token);
    let uploaded = guest
        .post(server.url(&format!(
            "/api/v1/share/{token}/uploads?filename=too-big.jpg"
        )))
        .header("content-type", "application/octet-stream")
        .body(vec![0_u8; 100])
        .send()
        .await
        .unwrap();
    assert_eq!(uploaded.status(), 413);

    let queued: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM guest_upload_queue WHERE filename = 'too-big.jpg'",
    )
    .fetch_one(server.db.pool())
    .await
    .unwrap();
    assert_eq!(queued, 0);

    let leftovers: i64 =
        sqlx::query_scalar("SELECT count(*) FROM assets WHERE filename = 'too-big.jpg'")
            .fetch_one(server.db.pool())
            .await
            .unwrap();
    assert_eq!(leftovers, 0);
}
