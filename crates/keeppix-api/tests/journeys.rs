mod harness;
mod journey;

use std::fs;
use std::time::{Duration, Instant};

use harness::TestServer;
use journey::{
    build_fixture_archive, create_library, drain_workers, login_as, scan_and_wait, setup_admin,
    start_scan,
};
use serde_json::json;

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn v1_from_empty_instance_to_photos_in_the_timeline() {
    let server = TestServer::start().await;
    let journey_deadline = Instant::now() + Duration::from_secs(60);
    let archive = build_fixture_archive(&server);

    setup_admin(&server).await;

    let library_id = create_library(&server, "Foto", &archive.root).await;
    let scan_elapsed =
        scan_and_wait(&server, &library_id, archive.photo_count, journey_deadline).await;

    let buckets = server
        .client
        .get(server.url("/api/v1/timeline/buckets"))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    let bucket_sum: i64 = buckets
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["count"].as_i64().unwrap_or(0))
        .sum();
    assert_eq!(bucket_sum, archive.photo_count);

    let month = buckets[0]["month"].as_str().unwrap();
    let timeline = server
        .client
        .get(server.url(&format!("/api/v1/timeline?bucket={month}")))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    let asset = &timeline["assets"][0];
    let content_hash = asset["content_hash"]
        .as_str()
        .expect("indexed asset has content_hash");
    assert!(!content_hash.is_empty());

    let thumb = server
        .client
        .get(server.url(&format!("/media/thumb/{content_hash}")))
        .send()
        .await
        .unwrap();
    assert_eq!(thumb.status(), 200);
    assert_eq!(thumb.headers().get("content-type").unwrap(), "image/webp");

    assert!(
        Instant::now() < journey_deadline,
        "full V1 journey exceeded 60s (scan took {scan_elapsed:?})"
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn v2_a_second_user_sees_only_what_it_should() {
    let server = TestServer::start().await;
    let journey_deadline = Instant::now() + Duration::from_secs(60);

    setup_admin(&server).await;
    let admin_root = server
        .photos_root
        .join(format!("admin-only-{}", uuid::Uuid::now_v7().simple()));
    fs::create_dir_all(&admin_root).unwrap();
    fs::copy(journey::tiny_fixture_path(), admin_root.join("secret.jpg")).unwrap();
    let admin_library_id = create_library(&server, "AdminLib", &admin_root).await;
    scan_and_wait(&server, &admin_library_id, 1, journey_deadline).await;

    let created = server
        .client
        .post(server.url("/api/v1/users"))
        .json(&json!({
            "username": "mario",
            "display_name": "Mario",
            "password": "mario-password-ok",
            "role": "user"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(created.status(), 201);

    let mario = login_as(&server, "mario", "mario-password-ok").await;
    let listed = mario
        .get(server.url("/api/v1/libraries"))
        .send()
        .await
        .unwrap();
    assert_eq!(listed.status(), 200);
    let list: serde_json::Value = listed.json().await.unwrap();
    assert!(
        list.as_array().unwrap().is_empty(),
        "mario has no libraries of its own"
    );

    let probe = mario
        .get(server.url(&format!("/api/v1/libraries/{admin_library_id}")))
        .send()
        .await
        .unwrap();
    assert_eq!(probe.status(), 403);
    let problem: serde_json::Value = probe.json().await.unwrap();
    assert_eq!(problem["type"], "keeppix/forbidden");

    assert!(Instant::now() < journey_deadline);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn v3_trash_and_restore_round_trip() {
    let server = TestServer::start().await;
    let journey_deadline = Instant::now() + Duration::from_secs(60);

    let root = server
        .photos_root
        .join(format!("trash-trip-{}", uuid::Uuid::now_v7().simple()));
    fs::create_dir_all(&root).unwrap();
    fs::copy(journey::tiny_fixture_path(), root.join("kept.jpg")).unwrap();
    setup_admin(&server).await;
    let library_id = create_library(&server, "TrashTrip", &root).await;
    scan_and_wait(&server, &library_id, 1, journey_deadline).await;

    let buckets = server
        .client
        .get(server.url("/api/v1/timeline/buckets"))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    let month = buckets[0]["month"].as_str().unwrap();
    let page = server
        .client
        .get(server.url(&format!("/api/v1/timeline?bucket={month}")))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    let asset_id = page["assets"][0]["id"].as_str().unwrap();
    let original = root.join("kept.jpg");

    let delete = server
        .client
        .delete(server.url(&format!("/api/v1/assets/{asset_id}")))
        .json(&json!({ "disk_action": "moved_to_trash" }))
        .send()
        .await
        .unwrap();
    assert_eq!(delete.status(), 204);
    assert!(!original.exists(), "file moved to server trash");

    let trash = server
        .client
        .get(server.url("/api/v1/trash"))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    let items = trash["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["asset_id"].as_str().unwrap(), asset_id);

    let restore = server
        .client
        .post(server.url(&format!("/api/v1/assets/{asset_id}/restore")))
        .send()
        .await
        .unwrap();
    assert_eq!(restore.status(), 204);
    assert!(original.is_file(), "file restored to original path");

    assert!(Instant::now() < journey_deadline);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn v4_an_unreachable_library_never_loses_data() {
    let server = TestServer::start().await;
    let journey_deadline = Instant::now() + Duration::from_secs(60);

    setup_admin(&server).await;
    let root = server
        .photos_root
        .join(format!("gone-soon-{}", uuid::Uuid::now_v7().simple()));
    fs::create_dir_all(&root).unwrap();
    fs::copy(journey::tiny_fixture_path(), root.join("survivor.jpg")).unwrap();
    let library_id = create_library(&server, "GoneSoon", &root).await;
    scan_and_wait(&server, &library_id, 1, journey_deadline).await;

    let before: i64 = sqlx::query_scalar("SELECT count(*) FROM assets")
        .fetch_one(server.db.pool())
        .await
        .unwrap();
    assert!(before >= 1);

    fs::remove_dir_all(&root).unwrap();

    start_scan(&server, &library_id).await;
    drain_workers(&server, journey_deadline).await;

    let status = server
        .client
        .get(server.url(&format!("/api/v1/libraries/{library_id}/scan")))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    assert_eq!(status["library_status"], "offline");
    assert_eq!(status["phase"], "offline");

    let library = server
        .client
        .get(server.url(&format!("/api/v1/libraries/{library_id}")))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    assert_eq!(library["status"], "offline");

    let after: i64 = sqlx::query_scalar("SELECT count(*) FROM assets")
        .fetch_one(server.db.pool())
        .await
        .unwrap();
    assert_eq!(before, after);

    assert!(Instant::now() < journey_deadline);
}
