//! Viewer vs editor on metadata and trash: a viewer may see; an editor may
//! edit metadata and move to trash.

mod harness;
mod journey;

use std::time::{Duration, Instant};

use harness::TestServer;
use journey::{
    build_fixture_archive, create_library, create_user, folder_id_by_name, grant_folder_editor,
    grant_folder_viewer, login_as, scan_and_wait, setup_admin,
};
use serde_json::json;

#[allow(clippy::expect_used, clippy::unwrap_used)]
async fn first_visible_asset(client: &reqwest::Client, server: &TestServer) -> String {
    let buckets = client
        .get(server.url("/api/v1/timeline/buckets"))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    let month = buckets.as_array().unwrap()[0]["month"]
        .as_str()
        .expect("a month bucket");
    let timeline = client
        .get(server.url(&format!("/api/v1/timeline?bucket={month}")))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    timeline["assets"][0]["id"]
        .as_str()
        .expect("an asset id")
        .to_owned()
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn a_folder_viewer_cannot_edit_metadata_or_trash() {
    let server = TestServer::start().await;
    let deadline = Instant::now() + Duration::from_secs(90);
    setup_admin(&server).await;
    let archive = build_fixture_archive(&server);
    let library_id = create_library(&server, "SharedLib", &archive.root).await;
    scan_and_wait(&server, &library_id, archive.photo_count, deadline).await;

    let mario_id = create_user(&server, "mario-viewer", "mario-password-ok").await;
    let folder = folder_id_by_name(&server, "album-a").await;
    grant_folder_viewer(&server, &mario_id, &folder).await;

    let mario = login_as(&server, "mario-viewer", "mario-password-ok").await;
    let asset_id = first_visible_asset(&mario, &server).await;

    let patch = mario
        .patch(server.url(&format!("/api/v1/assets/{asset_id}/metadata")))
        .json(&json!({ "title": "nope" }))
        .send()
        .await
        .unwrap();
    assert_eq!(patch.status(), 403, "viewer must not patch metadata");

    let trash = mario
        .delete(server.url(&format!("/api/v1/assets/{asset_id}")))
        .json(&json!({ "disk_action": "moved_to_trash" }))
        .send()
        .await
        .unwrap();
    assert_eq!(trash.status(), 403, "viewer must not trash");

    let kept = mario
        .delete(server.url(&format!("/api/v1/assets/{asset_id}")))
        .json(&json!({ "disk_action": "kept" }))
        .send()
        .await
        .unwrap();
    assert_eq!(kept.status(), 403, "viewer must not keep-delete");
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn a_folder_editor_can_edit_metadata_and_trash() {
    let server = TestServer::start().await;
    let deadline = Instant::now() + Duration::from_secs(90);
    setup_admin(&server).await;
    let archive = build_fixture_archive(&server);
    let library_id = create_library(&server, "SharedLib", &archive.root).await;
    scan_and_wait(&server, &library_id, archive.photo_count, deadline).await;

    let luigi_id = create_user(&server, "luigi-editor", "luigi-password-ok").await;
    let folder = folder_id_by_name(&server, "album-a").await;
    grant_folder_editor(&server, &luigi_id, &folder).await;

    let luigi = login_as(&server, "luigi-editor", "luigi-password-ok").await;
    let asset_id = first_visible_asset(&luigi, &server).await;

    let patch = luigi
        .patch(server.url(&format!("/api/v1/assets/{asset_id}/metadata")))
        .json(&json!({ "title": "ok" }))
        .send()
        .await
        .unwrap();
    assert_eq!(patch.status(), 204, "editor may patch metadata");

    let trash = luigi
        .delete(server.url(&format!("/api/v1/assets/{asset_id}")))
        .json(&json!({ "disk_action": "moved_to_trash" }))
        .send()
        .await
        .unwrap();
    assert_eq!(trash.status(), 204, "editor may trash");
}
