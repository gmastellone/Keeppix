mod harness;
mod journey;

use std::fs;
use std::time::{Duration, Instant};

use harness::TestServer;
use journey::{
    build_fixture_archive, create_library, create_share_link, create_share_link_from, create_user,
    drain_workers, folder_id_by_name, grant_folder_viewer, login_as, scan_and_wait, setup_admin,
    share_client, start_scan,
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

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn v5_shared_folder_is_visible_only_to_the_grantee() {
    let server = TestServer::start().await;
    let deadline = Instant::now() + Duration::from_secs(90);
    setup_admin(&server).await;
    let archive = build_fixture_archive(&server);
    let library_id = create_library(&server, "SharedLib", &archive.root).await;
    scan_and_wait(&server, &library_id, archive.photo_count, deadline).await;

    let mario_id = create_user(&server, "mario-v5", "mario-password-ok").await;
    let folder_a = folder_id_by_name(&server, "album-a").await;
    grant_folder_viewer(&server, &mario_id, &folder_a).await;

    let mario = login_as(&server, "mario-v5", "mario-password-ok").await;
    let buckets = mario
        .get(server.url("/api/v1/timeline/buckets"))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    let count: i64 = buckets
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["count"].as_i64().unwrap_or(0))
        .sum();
    assert_eq!(count, 3);

    let probe = mario
        .get(server.url(&format!("/api/v1/libraries/{library_id}")))
        .send()
        .await
        .unwrap();
    assert_eq!(probe.status(), 403);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn v6_group_members_inherit_folder_shares() {
    let server = TestServer::start().await;
    let deadline = Instant::now() + Duration::from_secs(90);
    setup_admin(&server).await;
    let archive = build_fixture_archive(&server);
    let library_id = create_library(&server, "FamilyLib", &archive.root).await;
    scan_and_wait(&server, &library_id, archive.photo_count, deadline).await;

    let group = server
        .client
        .post(server.url("/api/v1/groups"))
        .json(&json!({"name": "Famiglia"}))
        .send()
        .await
        .unwrap();
    assert_eq!(group.status(), 201);
    let group_id = group.json::<serde_json::Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();

    for user in ["anna", "luigi", "paola"] {
        let uid = create_user(&server, user, "user-password-ok").await;
        let add = server
            .client
            .post(server.url(&format!("/api/v1/groups/{group_id}/members/{uid}")))
            .send()
            .await
            .unwrap();
        assert_eq!(add.status(), 204);
    }

    let folder_a = folder_id_by_name(&server, "album-a").await;
    let grant = server
        .client
        .post(server.url("/api/v1/permissions"))
        .json(&json!({
            "subject_type": "group",
            "subject_id": group_id,
            "object_type": "folder",
            "object_id": folder_a,
            "role": "viewer",
            "inherit": true
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(grant.status(), 201);

    let marco_id = create_user(&server, "marco", "user-password-ok").await;
    let add_marco = server
        .client
        .post(server.url(&format!("/api/v1/groups/{group_id}/members/{marco_id}")))
        .send()
        .await
        .unwrap();
    assert_eq!(add_marco.status(), 204);

    let marco = login_as(&server, "marco", "user-password-ok").await;
    let buckets = marco
        .get(server.url("/api/v1/timeline/buckets"))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    let count: i64 = buckets
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["count"].as_i64().unwrap_or(0))
        .sum();
    assert_eq!(count, 3);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn v7_password_protected_share_link_opens_for_guests() {
    let server = TestServer::start().await;
    let deadline = Instant::now() + Duration::from_secs(90);
    setup_admin(&server).await;
    let archive = build_fixture_archive(&server);
    let library_id = create_library(&server, "AlbumLib", &archive.root).await;
    scan_and_wait(&server, &library_id, archive.photo_count, deadline).await;

    let album = server
        .client
        .post(server.url("/api/v1/albums"))
        .json(&json!({"name": "Vacanze"}))
        .send()
        .await
        .unwrap();
    assert_eq!(album.status(), 201);
    let album_id = album.json::<serde_json::Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();

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
    let add = server
        .client
        .post(server.url(&format!("/api/v1/albums/{album_id}/assets/{asset_id}")))
        .send()
        .await
        .unwrap();
    assert_eq!(add.status(), 204);

    let token = create_share_link(&server, "album", &album_id, Some("guest-secret-ok")).await;
    let guest = share_client(&token);
    assert_eq!(
        guest
            .get(server.url(&format!("/api/v1/share/{token}")))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    let auth = guest
        .post(server.url(&format!("/api/v1/share/{token}/auth")))
        .json(&json!({"password": "guest-secret-ok"}))
        .send()
        .await
        .unwrap();
    assert_eq!(auth.status(), 204);
    let content = guest
        .get(server.url(&format!("/api/v1/share/{token}/assets")))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    assert_eq!(content["assets"].as_array().unwrap().len(), 1);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn v8_revoking_a_link_locks_out_whoever_holds_it() {
    let server = TestServer::start().await;
    let deadline = Instant::now() + Duration::from_secs(90);
    setup_admin(&server).await;
    let archive = build_fixture_archive(&server);
    let library_id = create_library(&server, "RevokeLib", &archive.root).await;
    scan_and_wait(&server, &library_id, archive.photo_count, deadline).await;

    let folder_a = folder_id_by_name(&server, "album-a").await;
    let created = server
        .client
        .post(server.url("/api/v1/share/links"))
        .json(&json!({
            "object_type": "folder",
            "object_id": folder_a,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(created.status(), 201);
    let body = created.json::<serde_json::Value>().await.unwrap();
    let link_id = body["id"].as_str().unwrap();
    let token = body["token"].as_str().unwrap();

    let guest = share_client(token);
    assert_eq!(
        guest
            .get(server.url(&format!("/api/v1/share/{token}/assets")))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );

    let revoke = server
        .client
        .delete(server.url(&format!("/api/v1/share/links/{link_id}")))
        .send()
        .await
        .unwrap();
    assert_eq!(revoke.status(), 204);

    assert_eq!(
        guest
            .get(server.url(&format!("/api/v1/share/{token}/assets")))
            .send()
            .await
            .unwrap()
            .status(),
        403
    );

    let audit = server
        .client
        .get(server.url("/api/v1/audit?limit=10"))
        .send()
        .await
        .unwrap();
    assert_eq!(audit.status(), 200);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn v9_guest_uploads_stay_hidden_until_approved() {
    let server = TestServer::start().await;
    let deadline = Instant::now() + Duration::from_secs(60);
    setup_admin(&server).await;
    let root = server
        .photos_root
        .join(format!("guest-{}", uuid::Uuid::now_v7().simple()));
    fs::create_dir_all(&root).unwrap();
    fs::copy(journey::tiny_fixture_path(), root.join("host.jpg")).unwrap();
    let library_id = create_library(&server, "GuestLib", &root).await;
    scan_and_wait(&server, &library_id, 1, deadline).await;

    let root_folder: (uuid::Uuid,) =
        sqlx::query_as("SELECT id FROM folders WHERE library_id = $1 AND parent_id IS NULL")
            .bind(uuid::Uuid::parse_str(&library_id).unwrap())
            .fetch_one(server.db.pool())
            .await
            .unwrap();
    let folder_id = root_folder.0.to_string();
    let token = create_share_link_from(
        &server,
        json!({
            "object_type": "folder",
            "object_id": folder_id,
            "allow_upload": true,
        }),
    )
    .await;

    let guest = share_client(&token);
    let uploaded = guest
        .post(server.url(&format!("/api/v1/share/{token}/uploads?filename=guest.jpg")))
        .header("content-type", "application/octet-stream")
        .body(fs::read(journey::tiny_fixture_path()).unwrap())
        .send()
        .await
        .unwrap();
    assert_eq!(uploaded.status(), 201);
    let upload_id = uploaded.json::<serde_json::Value>().await.unwrap()["id"]
        .as_str()
        .expect("upload id")
        .to_owned();

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
        "pending guest files must stay out of the shared listing"
    );

    let timeline_after = server
        .client
        .get(server.url("/api/v1/timeline/buckets"))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    let count_before: i64 = timeline_after
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["count"].as_i64().unwrap_or(0))
        .sum();
    assert_eq!(count_before, 1);

    let approve = server
        .client
        .post(server.url(&format!("/api/v1/guest-uploads/{upload_id}/approve")))
        .send()
        .await
        .unwrap();
    assert_eq!(approve.status(), 204);

    drain_workers(&server, deadline).await;

    let timeline_approved = server
        .client
        .get(server.url("/api/v1/timeline/buckets"))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    let count_after: i64 = timeline_approved
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["count"].as_i64().unwrap_or(0))
        .sum();
    assert_eq!(count_after, 2);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn v10_admin_can_disable_a_user_and_kill_sessions() {
    let server = TestServer::start().await;
    setup_admin(&server).await;
    let uid = create_user(&server, "tempo", "tempo-password-ok").await;
    let tempo = login_as(&server, "tempo", "tempo-password-ok").await;
    assert_eq!(
        tempo
            .get(server.url("/api/v1/auth/me"))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );

    let disable = server
        .client
        .post(server.url(&format!("/api/v1/users/{uid}/disable")))
        .send()
        .await
        .unwrap();
    assert_eq!(disable.status(), 204);

    assert_eq!(
        tempo
            .get(server.url("/api/v1/auth/me"))
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn v11_batch_date_shift_can_be_undone() {
    let server = TestServer::start().await;
    let deadline = Instant::now() + Duration::from_secs(60);
    setup_admin(&server).await;
    let root = server
        .photos_root
        .join(format!("batch-{}", uuid::Uuid::now_v7().simple()));
    fs::create_dir_all(&root).unwrap();
    fs::copy(journey::tiny_fixture_path(), root.join("batch.jpg")).unwrap();
    let library_id = create_library(&server, "BatchLib", &root).await;
    scan_and_wait(&server, &library_id, 1, deadline).await;

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
    let before = page["assets"][0]["taken_at_utc"]
        .as_str()
        .unwrap()
        .to_owned();

    let shift = server
        .client
        .post(server.url("/api/v1/metadata/batch/shift-taken-at"))
        .json(&json!({
            "asset_ids": [asset_id],
            "hours": 1
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(shift.status(), 200);
    let batch_id = shift.json::<serde_json::Value>().await.unwrap()["batch_id"]
        .as_str()
        .unwrap()
        .to_owned();

    let meta = server
        .client
        .get(server.url(&format!("/api/v1/assets/{asset_id}/metadata")))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    assert_ne!(meta["taken_at"].as_str().unwrap(), before);

    let undo = server
        .client
        .post(server.url(&format!("/api/v1/metadata/batch/{batch_id}/undo")))
        .send()
        .await
        .unwrap();
    assert_eq!(undo.status(), 204);

    let restored = server
        .client
        .get(server.url(&format!("/api/v1/assets/{asset_id}/metadata")))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    assert_eq!(restored["taken_at"].as_str().unwrap(), before);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn v12_trash_restore_returns_photo_to_timeline() {
    let server = TestServer::start().await;
    let deadline = Instant::now() + Duration::from_secs(60);
    setup_admin(&server).await;
    let root = server
        .photos_root
        .join(format!("v12-{}", uuid::Uuid::now_v7().simple()));
    fs::create_dir_all(&root).unwrap();
    fs::copy(journey::tiny_fixture_path(), root.join("restore-me.jpg")).unwrap();
    let library_id = create_library(&server, "V12", &root).await;
    scan_and_wait(&server, &library_id, 1, deadline).await;

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

    assert_eq!(
        server
            .client
            .delete(server.url(&format!("/api/v1/assets/{asset_id}")))
            .json(&json!({"disk_action": "moved_to_trash"}))
            .send()
            .await
            .unwrap()
            .status(),
        204
    );

    let trash = server
        .client
        .get(server.url("/api/v1/trash"))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    assert_eq!(trash["items"].as_array().unwrap().len(), 1);

    assert_eq!(
        server
            .client
            .post(server.url(&format!("/api/v1/assets/{asset_id}/restore")))
            .send()
            .await
            .unwrap()
            .status(),
        204
    );

    let buckets_after = server
        .client
        .get(server.url("/api/v1/timeline/buckets"))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    let count: i64 = buckets_after
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["count"].as_i64().unwrap_or(0))
        .sum();
    assert_eq!(count, 1);
}

fn v13_basic_auth_header(username: &str, secret: &str) -> String {
    use base64::Engine as _;
    let raw = format!("{username}:{secret}");
    format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(raw)
    )
}

#[allow(clippy::expect_used)]
async fn v13_create_app_password(server: &TestServer, label: &str) -> String {
    let created = server
        .client
        .post(server.url("/api/v1/users/me/app-passwords"))
        .json(&json!({ "label": label }))
        .send()
        .await
        .expect("create app password");
    assert_eq!(created.status(), 201);
    let body: serde_json::Value = created.json().await.expect("app password json");
    body["secret"].as_str().expect("secret").to_owned()
}

#[allow(clippy::expect_used, clippy::unwrap_used)]
async fn v13_asset_id_by_name(server: &TestServer, folder_id: &str, filename: &str) -> String {
    let row: (uuid::Uuid,) =
        sqlx::query_as("SELECT id FROM assets WHERE folder_id = $1 AND filename = $2")
            .bind(uuid::Uuid::parse_str(folder_id).unwrap())
            .bind(filename)
            .fetch_one(server.db.pool())
            .await
            .expect("asset by name");
    row.0.to_string()
}

#[allow(clippy::expect_used, clippy::unwrap_used)]
async fn v13_library_root_folder_id(server: &TestServer, library_id: &str) -> String {
    let row: (uuid::Uuid,) =
        sqlx::query_as("SELECT id FROM folders WHERE library_id = $1 AND parent_id IS NULL")
            .bind(uuid::Uuid::parse_str(library_id).unwrap())
            .fetch_one(server.db.pool())
            .await
            .expect("library root folder");
    row.0.to_string()
}

#[allow(clippy::expect_used)]
async fn v13_set_flags(server: &TestServer, asset_id: &str, body: serde_json::Value) {
    let response = server
        .client
        .put(server.url(&format!("/api/v1/assets/{asset_id}/flags")))
        .json(&body)
        .send()
        .await
        .expect("set flags");
    assert_eq!(response.status(), 204);
}

#[allow(clippy::expect_used)]
async fn v13_get_flags(server: &TestServer, asset_id: &str) -> serde_json::Value {
    server
        .client
        .get(server.url(&format!("/api/v1/assets/{asset_id}/flags")))
        .send()
        .await
        .expect("get flags")
        .json()
        .await
        .expect("flags json")
}

#[allow(clippy::expect_used)]
async fn v13_create_album(server: &TestServer, name: &str) -> String {
    let response = server
        .client
        .post(server.url("/api/v1/albums"))
        .json(&json!({ "name": name }))
        .send()
        .await
        .expect("create album");
    assert_eq!(response.status(), 201);
    let body: serde_json::Value = response.json().await.expect("album json");
    body["id"].as_str().expect("album id").to_owned()
}

#[allow(clippy::expect_used)]
async fn v13_add_to_album(server: &TestServer, album_id: &str, asset_id: &str) {
    let response = server
        .client
        .post(server.url(&format!("/api/v1/albums/{album_id}/assets/{asset_id}")))
        .send()
        .await
        .expect("add to album");
    assert_eq!(response.status(), 204);
}

#[allow(clippy::expect_used)]
async fn v13_album_asset_ids(server: &TestServer, album_id: &str) -> Vec<String> {
    let response = server
        .client
        .get(server.url(&format!("/api/v1/albums/{album_id}/assets")))
        .send()
        .await
        .expect("list album assets");
    let body: serde_json::Value = response.json().await.expect("album assets json");
    body.as_array()
        .expect("array")
        .iter()
        .map(|item| item["id"].as_str().expect("asset id").to_owned())
        .collect()
}

/// This journey is done when a real trip (import across multiple days,
/// culling, renaming, downloading via `WebDAV`, external editing, deleting
/// RAW files) completes without touching the filesystem by hand, and
/// without any photo losing its rating, tags, or album membership along
/// the way. Every step below goes through a real API (HTTP or `WebDAV`),
/// never a direct disk write outside of those — the only exception is the
/// initial creation of the source archive, which in real life is the
/// camera's memory card, not an action inside Keeppix.
///
/// **Declared scope**: "culling" covers both the per-user vote (`pick`/
/// `reject`/`rating`, `PUT .../flags`) and the physical move into the
/// `_taken`/`_skipped` lots (`CullingRepo::set_pick`/`list_lots`/
/// `empty_skipped`). A fifth asset, outside the scope of the
/// renaming/WebDAV/trash already exercised by the other four, closes out
/// the test with the designated root via `PATCH .../culling-root`, the
/// lot listing, the physical move of a rejected shot, and the final
/// emptying — until this test was added, this step was only exercised by
/// the `keeppix-db` suite calling the repository directly, never by a
/// real HTTP route.
/// Same story for tags: in the app they are only proposed by the AI and
/// confirmed, not assignable by hand via HTTP — they're out of scope for
/// this test. "Deleting RAW files" is exercised with `DELETE
/// /dav/asset/…` on any asset: the deletion path is the same regardless
/// of file type (verified by reading `dav::write`/`TrashRepo::choose` —
/// no branch specific to `AssetKind::RawImage`), so a second JPEG
/// exercises the same mechanism as a real `.arw` without requiring an
/// actual RAW decoder in the test environment.
#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::too_many_lines)]
async fn v13_a_real_trip_survives_culling_rename_webdav_and_raw_cleanup() {
    let server = TestServer::start().await;
    let deadline = Instant::now() + Duration::from_secs(90);
    setup_admin(&server).await;
    let secret = v13_create_app_password(&server, "Finder").await;

    // Import across multiple days: two day-folders, two JPEGs each — the
    // camera's "memory card", the only disk write in this test that
    // doesn't go through a Keeppix API.
    let root = server
        .photos_root
        .join(format!("trip-{}", uuid::Uuid::now_v7().simple()));
    let bytes = fs::read(journey::tiny_fixture_path()).unwrap();
    for day in ["2026-08-14", "2026-08-15"] {
        let dir = root.join(day);
        fs::create_dir_all(&dir).unwrap();
        for i in 0..2 {
            fs::write(dir.join(format!("IMG_{i}.jpg")), &bytes).unwrap();
        }
    }
    // Fifth asset, dedicated to the batch culling step at the end of the
    // test (see "Declared scope" above): outside the scope of the
    // renaming/WebDAV/trash already exercised by the other four, so
    // adding it doesn't touch any of the existing assertions.
    fs::write(root.join("2026-08-15").join("IMG_2.jpg"), &bytes).unwrap();
    let library_id = create_library(&server, "Viaggio", &root).await;
    scan_and_wait(&server, &library_id, 5, deadline).await;

    let day1 = folder_id_by_name(&server, "2026-08-14").await;
    let day2 = folder_id_by_name(&server, "2026-08-15").await;
    let keeper = v13_asset_id_by_name(&server, &day1, "IMG_0.jpg").await;
    let rejected = v13_asset_id_by_name(&server, &day1, "IMG_1.jpg").await;
    let other1 = v13_asset_id_by_name(&server, &day2, "IMG_0.jpg").await;
    let other2 = v13_asset_id_by_name(&server, &day2, "IMG_1.jpg").await;
    let culling_target = v13_asset_id_by_name(&server, &day2, "IMG_2.jpg").await;

    // Culling: per-user vote via the real API, before renaming.
    v13_set_flags(&server, &keeper, json!({"rating": 5, "pick": "pick"})).await;
    v13_set_flags(&server, &rejected, json!({"pick": "reject"})).await;
    v13_set_flags(&server, &other1, json!({"favorite": true})).await;

    // Album membership: only the two "picked" ones go into the album.
    let album_id = v13_create_album(&server, "Migliori del viaggio").await;
    v13_add_to_album(&server, &album_id, &keeper).await;
    v13_add_to_album(&server, &album_id, &other1).await;

    // Batch rename across the whole scope (the real route, not
    // `RenameRepo` directly).
    let asset_ids = vec![
        keeper.clone(),
        rejected.clone(),
        other1.clone(),
        other2.clone(),
    ];
    let preview: serde_json::Value = server
        .client
        .post(server.url("/api/v1/assets/batch/rename/preview"))
        .json(&json!({ "asset_ids": asset_ids, "schema": "Viaggio_{n:2}" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let preview_items = preview.as_array().unwrap();
    assert_eq!(preview_items.len(), 4);
    assert!(
        preview_items.iter().all(|item| item["collides"] == false),
        "the preview must not report collisions on a scope with no duplicates: {preview:?}"
    );
    let expected_names: std::collections::HashMap<String, String> = preview_items
        .iter()
        .map(|item| {
            (
                item["asset_id"].as_str().unwrap().to_owned(),
                item["new_name"].as_str().unwrap().to_owned(),
            )
        })
        .collect();

    // The rename runs in the background (JobKind::BulkRename): the
    // request responds 202 with just an operation_id, the outcome has to
    // be read after draining the workers — the same reason `scan_and_wait`
    // already exists in this file for scanning.
    let applied = server
        .client
        .post(server.url("/api/v1/assets/batch/rename"))
        .json(&json!({ "asset_ids": asset_ids, "schema": "Viaggio_{n:2}" }))
        .send()
        .await
        .unwrap();
    assert_eq!(applied.status(), 202);
    let applied: serde_json::Value = applied.json().await.unwrap();
    let operation_id = applied["operation_id"].as_str().unwrap().to_owned();

    drain_workers(&server, deadline).await;

    // The tracked operation reached `Done`, not left `running` — the same
    // source of truth the `WebSocket` reads.
    let (op_status, op_done, op_total): (String, i64, Option<i64>) = sqlx::query_as(
        "SELECT status, done, total FROM operations WHERE id = $1 AND kind = 'bulk_rename'",
    )
    .bind(uuid::Uuid::parse_str(&operation_id).unwrap())
    .fetch_one(server.db.pool())
    .await
    .unwrap();
    assert_eq!(op_status, "done");
    assert_eq!(op_done, 4);
    assert_eq!(op_total, Some(4));

    // The files are actually renamed on disk, with the names from the
    // preview — not just the database row.
    assert!(
        root.join("2026-08-14")
            .join(&expected_names[&keeper])
            .is_file()
    );
    assert!(
        root.join("2026-08-15")
            .join(&expected_names[&other1])
            .is_file()
    );
    assert!(!root.join("2026-08-14").join("IMG_0.jpg").exists());

    // No photo lost its vote or album membership by going through the
    // rename — identity (`asset_id`) doesn't change with `move_asset`.
    let keeper_flags = v13_get_flags(&server, &keeper).await;
    assert_eq!(keeper_flags["rating"], 5);
    assert_eq!(keeper_flags["pick"], "pick");
    let rejected_flags = v13_get_flags(&server, &rejected).await;
    assert_eq!(rejected_flags["pick"], "reject");
    let other1_flags = v13_get_flags(&server, &other1).await;
    assert_eq!(other1_flags["favorite"], true);
    let mut album_members = v13_album_asset_ids(&server, &album_id).await;
    album_members.sort();
    let mut expected_members = vec![keeper.clone(), other1.clone()];
    expected_members.sort();
    assert_eq!(album_members, expected_members);

    // Download via WebDAV: the same asset, downloaded by id (never by
    // name — the name just changed) returns exactly the original bytes.
    let auth = v13_basic_auth_header("giovanni", &secret);
    let download = server
        .client
        .get(server.url(&format!("/dav/asset/{keeper}")))
        .header("authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(download.status(), 200);
    assert_eq!(download.bytes().await.unwrap().as_ref(), bytes.as_slice());

    // External editing: a `WebDAV` client (Lightroom, Finder, ...) drops a
    // modified file next to the renamed originals.
    let mut edited_bytes = bytes.clone();
    edited_bytes.push(0);
    let put_response = server
        .client
        .put(server.url(&format!("/dav/folder/{day1}/sviluppo-esterno.jpg")))
        .header("authorization", &auth)
        .body(edited_bytes.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(put_response.status(), 201);
    assert_eq!(
        fs::read(root.join("2026-08-14").join("sviluppo-esterno.jpg")).unwrap(),
        edited_bytes
    );
    drain_workers(&server, deadline).await;
    let developed_id = v13_asset_id_by_name(&server, &day1, "sviluppo-esterno.jpg").await;
    assert_ne!(
        developed_id, keeper,
        "the deposited file is a new asset, not a duplicate"
    );

    // Deletion: `DELETE` via `WebDAV` moves to trash, it doesn't silently
    // delete from the filesystem (the same path as `dav::write` for any
    // file type, RAW included — see the note above the function).
    let delete_response = server
        .client
        .delete(server.url(&format!("/dav/asset/{other2}")))
        .header("authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(delete_response.status(), 204);
    let (disk_action, trash_path): (String, Option<String>) =
        sqlx::query_as("SELECT disk_action, trash_path FROM trash_entries WHERE asset_id = $1")
            .bind(uuid::Uuid::parse_str(&other2).unwrap())
            .fetch_one(server.db.pool())
            .await
            .unwrap();
    assert_eq!(disk_action, "moved_to_trash");
    assert!(std::path::Path::new(&trash_path.unwrap()).exists());

    // No side effects on the other photos: the vote, the favorite, and
    // the album for the rest of the trip stay the same as before the
    // deletion — deleting one didn't touch the others.
    let keeper_flags_after = v13_get_flags(&server, &keeper).await;
    assert_eq!(keeper_flags_after, keeper_flags);
    let mut album_members_after = v13_album_asset_ids(&server, &album_id).await;
    album_members_after.sort();
    assert_eq!(album_members_after, expected_members);

    // Batch culling: the designated root makes the day-folders themselves
    // count as lots, without restructuring the library. The fifth asset
    // hasn't been touched by any of the steps above — it's dedicated to
    // this.
    let root_folder_id = v13_library_root_folder_id(&server, &library_id).await;
    let updated_library: serde_json::Value = server
        .client
        .patch(server.url(&format!("/api/v1/libraries/{library_id}/culling-root")))
        .json(&json!({ "folder_id": root_folder_id }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        updated_library["culling_root_folder_id"],
        root_folder_id.clone()
    );

    let lots: serde_json::Value = server
        .client
        .get(server.url(&format!("/api/v1/libraries/{library_id}/culling/lots")))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let lots = lots.as_array().unwrap();
    assert_eq!(
        lots.len(),
        2,
        "2026-08-14 and 2026-08-15 count as lots under the designated root: {lots:?}"
    );
    let day2_lot = lots
        .iter()
        .find(|lot| lot["folder_id"] == day2)
        .expect("day2 is a lot");
    assert_eq!(
        day2_lot["pending"], 2,
        "other1 and the fifth asset are still pending in day2: {day2_lot:?}"
    );

    // Rejecting inside a lot physically moves the file to `_skipped`, not
    // just the flag — this part previously had no HTTP caller.
    let picked: serde_json::Value = server
        .client
        .post(server.url(&format!("/api/v1/assets/{culling_target}/pick")))
        .json(&json!({ "pick": "reject" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let new_folder_id = picked["folder_id"].as_str().unwrap().to_owned();
    assert_ne!(
        new_folder_id, day2,
        "an asset rejected inside a lot moves to _skipped, it doesn't stay in the lot's folder"
    );
    assert!(
        root.join("2026-08-15")
            .join("_skipped")
            .join("IMG_2.jpg")
            .is_file(),
        "the file is actually on disk inside _skipped"
    );
    let target_flags = v13_get_flags(&server, &culling_target).await;
    assert_eq!(target_flags["pick"], "reject");

    // "Empty skipped": permanent deletion, partial success (`BulkOutcome`)
    // instead of all-or-nothing.
    let emptied: serde_json::Value = server
        .client
        .post(server.url(&format!("/api/v1/culling/lots/{day2}/empty-skipped")))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let succeeded = emptied["succeeded"].as_array().unwrap();
    assert_eq!(succeeded.len(), 1, "{emptied:?}");
    assert_eq!(succeeded[0], culling_target);
    assert!(emptied["failed"].as_array().unwrap().is_empty());
    assert!(
        !root
            .join("2026-08-15")
            .join("_skipped")
            .join("IMG_2.jpg")
            .exists(),
        "emptying skipped actually deletes the file from disk"
    );

    let _ = fs::remove_dir_all(&root);
}
