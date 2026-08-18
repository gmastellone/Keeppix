mod harness;
mod journey;

use std::fs;
use std::time::{Duration, Instant};

use harness::TestServer;
use journey::{
    build_fixture_archive, create_library, create_share_link, create_user, drain_workers,
    folder_id_by_name, grant_folder_viewer, login_as, scan_and_wait, setup_admin, share_client,
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
    let _token = create_share_link(&server, "folder", &folder_id, None).await;
    let link_row: (uuid::Uuid,) =
        sqlx::query_as("SELECT id FROM share_links ORDER BY created_at DESC LIMIT 1")
            .fetch_one(server.db.pool())
            .await
            .unwrap();

    let guest_asset = uuid::Uuid::now_v7();
    sqlx::query(
        "INSERT INTO assets (id, folder_id, filename, size_bytes, mtime, kind, status, uploaded_by_guest) \
         VALUES ($1, $2, 'guest.jpg', 10, now(), 'image', 'indexed', true)",
    )
    .bind(guest_asset)
    .bind(uuid::Uuid::parse_str(&folder_id).unwrap())
    .execute(server.db.pool())
    .await
    .unwrap();
    let upload_id = uuid::Uuid::now_v7();
    sqlx::query(
        "INSERT INTO guest_upload_queue (id, asset_id, share_link_id, filename, size_bytes) \
         VALUES ($1, $2, $3, 'guest.jpg', 10)",
    )
    .bind(upload_id)
    .bind(guest_asset)
    .bind(link_row.0)
    .execute(server.db.pool())
    .await
    .unwrap();

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
