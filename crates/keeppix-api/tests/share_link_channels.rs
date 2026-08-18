#![allow(clippy::unwrap_used, clippy::expect_used)]

mod harness;
mod journey;

use std::fs;
use std::time::{Duration, Instant};

use harness::TestServer;
use journey::{
    create_library, create_share_link, folder_id_by_name, scan_and_wait, setup_admin, share_client,
};
use serde_json::json;

#[tokio::test]
async fn a_share_link_is_confined_on_every_channel() {
    let server = TestServer::start().await;
    let deadline = Instant::now() + Duration::from_secs(90);

    setup_admin(&server).await;
    let root = server
        .photos_root
        .join(format!("share-confine-{}", uuid::Uuid::now_v7().simple()));
    fs::create_dir_all(root.join("inside")).unwrap();
    fs::create_dir_all(root.join("outside")).unwrap();
    fs::copy(journey::tiny_fixture_path(), root.join("inside/a.jpg")).unwrap();
    fs::copy(journey::tiny_fixture_path(), root.join("outside/b.jpg")).unwrap();

    let library_id = create_library(&server, "Confine", &root).await;
    scan_and_wait(&server, &library_id, 2, deadline).await;

    let inside_folder = folder_id_by_name(&server, "inside").await;
    let token = create_share_link(&server, "folder", &inside_folder, None).await;

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
    let assets = page["assets"].as_array().unwrap();
    let inside_id = assets
        .iter()
        .find(|a| a["filename"].as_str() == Some("a.jpg"))
        .and_then(|a| a["id"].as_str())
        .expect("inside asset");
    let outside_id = assets
        .iter()
        .find(|a| a["filename"].as_str() == Some("b.jpg"))
        .and_then(|a| a["id"].as_str())
        .expect("outside asset");
    let inside_hash = assets
        .iter()
        .find(|a| a["filename"].as_str() == Some("a.jpg"))
        .and_then(|a| a["content_hash"].as_str())
        .expect("inside hash");

    let guest = share_client(&token);

    assert_eq!(
        guest
            .get(server.url(&format!("/api/v1/share/{token}/assets")))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );

    assert_eq!(
        guest
            .get(server.url(&format!("/media/thumb/{inside_hash}")))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    assert_eq!(
        guest
            .get(server.url(&format!("/media/original/{outside_id}")))
            .send()
            .await
            .unwrap()
            .status(),
        403
    );

    let search = guest
        .post(server.url("/api/v1/search"))
        .json(&json!({"ast": {"op": "text", "value": "*"}}))
        .send()
        .await
        .unwrap();
    assert_eq!(search.status(), 403);

    assert_eq!(
        guest
            .get(server.url("/api/v1/timeline/buckets"))
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
}

/// A password on a share link must gate every subsequent request. Today
/// `public_auth` verifies the password and discards the result; this test
/// fails until ShareAuth requires an unlock proof.
#[tokio::test]
async fn a_password_protected_link_rejects_media_without_unlock() {
    let server = TestServer::start().await;
    let deadline = Instant::now() + Duration::from_secs(90);

    setup_admin(&server).await;
    let root = server
        .photos_root
        .join(format!("share-pw-{}", uuid::Uuid::now_v7().simple()));
    fs::create_dir_all(&root).unwrap();
    fs::copy(journey::tiny_fixture_path(), root.join("a.jpg")).unwrap();

    let library_id = create_library(&server, "PwShare", &root).await;
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
    let hash = page["assets"][0]["content_hash"]
        .as_str()
        .expect("indexed hash");
    let folder_id = page["assets"][0]["folder_id"].as_str().unwrap();

    let token = create_share_link(&server, "folder", folder_id, Some("guest-secret-ok")).await;
    let locked = share_client(&token);

    for path in [
        format!("/media/thumb/{hash}"),
        format!("/media/preview/{hash}"),
        format!("/media/full/{hash}"),
        format!("/api/v1/share/{token}/assets"),
    ] {
        assert_eq!(
            locked.get(server.url(&path)).send().await.unwrap().status(),
            403,
            "{path} must stay closed until /auth"
        );
    }

    let unlocked = share_client(&token);
    let auth = unlocked
        .post(server.url(&format!("/api/v1/share/{token}/auth")))
        .json(&json!({"password": "guest-secret-ok"}))
        .send()
        .await
        .unwrap();
    assert_eq!(auth.status(), 204);
    assert_eq!(
        unlocked
            .get(server.url(&format!("/media/thumb/{hash}")))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    assert_eq!(
        unlocked
            .get(server.url(&format!("/api/v1/share/{token}/assets")))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
}
