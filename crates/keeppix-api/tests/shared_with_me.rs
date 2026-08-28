//! `GET /api/v1/shared-with-me`.

mod harness;
mod journey;

use std::time::{Duration, Instant};

use harness::TestServer;
use journey::{
    build_fixture_archive, create_library, create_user, folder_id_by_name, grant_folder_viewer,
    login_as, scan_and_wait, setup_admin,
};

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn a_folder_shared_with_a_user_shows_up_in_their_shared_with_me_list() {
    let server = TestServer::start().await;
    let deadline = Instant::now() + Duration::from_secs(90);
    setup_admin(&server).await;
    let archive = build_fixture_archive(&server);
    let library_id = create_library(&server, "SharedLib", &archive.root).await;
    scan_and_wait(&server, &library_id, archive.photo_count, deadline).await;

    let mario_id = create_user(&server, "mario-shared", "mario-password-ok").await;
    let folder = folder_id_by_name(&server, "album-a").await;
    grant_folder_viewer(&server, &mario_id, &folder).await;

    let mario = login_as(&server, "mario-shared", "mario-password-ok").await;
    let response = mario
        .get(server.url("/api/v1/shared-with-me"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let items = response.json::<serde_json::Value>().await.unwrap();
    let items = items.as_array().unwrap();
    assert_eq!(items.len(), 1, "exactly the one folder mario was granted");
    assert_eq!(items[0]["object_type"], "folder");
    assert_eq!(items[0]["object_id"], folder);
    assert_eq!(items[0]["role"], "viewer");
    assert!(
        items[0]["via_group"].is_null(),
        "a direct grant must not report a group origin"
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn a_user_with_no_grants_sees_an_empty_shared_with_me_list() {
    let server = TestServer::start().await;
    setup_admin(&server).await;
    create_user(&server, "nobody-shared", "nobody-password-ok").await;

    let nobody = login_as(&server, "nobody-shared", "nobody-password-ok").await;
    let response = nobody
        .get(server.url("/api/v1/shared-with-me"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let items = response.json::<serde_json::Value>().await.unwrap();
    assert_eq!(items.as_array().unwrap().len(), 0);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn shared_with_me_requires_authentication() {
    let server = TestServer::start().await;
    setup_admin(&server).await;

    let anonymous = reqwest::Client::builder()
        .cookie_store(true)
        .default_headers(harness::client_headers())
        .build()
        .unwrap();
    let response = anonymous
        .get(server.url("/api/v1/shared-with-me"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 401);
}
