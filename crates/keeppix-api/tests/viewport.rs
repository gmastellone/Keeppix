mod harness;

use harness::TestServer;
use keeppix_db::JobRepo;
use keeppix_domain::{JobKind, JobPriority};
use serde_json::json;
use std::sync::atomic::Ordering;

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn viewport_promotes_visible_derive_jobs() {
    let server = TestServer::start().await;
    setup(&server).await;
    let hex = "ab".repeat(32);
    JobRepo::new(&server.db)
        .enqueue(
            JobKind::DeriveAsset,
            json!({ "content_hash": hex }),
            JobPriority::Background,
            Some(&format!("derive:{hex}")),
        )
        .await
        .unwrap();

    let response = server
        .client
        .post(server.url("/api/v1/viewport"))
        .json(&json!({ "hashes": [hex] }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 204);

    let priority: i16 = sqlx::query_scalar("SELECT priority FROM jobs WHERE dedup_key = $1")
        .bind(format!("derive:{hex}"))
        .fetch_one(server.db.pool())
        .await
        .unwrap();
    assert!(priority <= 2, "visible is 2, got {priority}");
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn an_authenticated_request_notifies_the_activity_tracker() {
    let server = TestServer::start().await;
    setup(&server).await;
    assert_eq!(server.auth_pings.load(Ordering::Relaxed), 0);

    let me = server
        .client
        .get(server.url("/api/v1/auth/me"))
        .send()
        .await
        .unwrap();
    assert_eq!(me.status(), 200);
    assert!(
        server.auth_pings.load(Ordering::Relaxed) >= 1,
        "Auth must ping ActivityTracker so workers switch to Interactive"
    );
}

#[allow(clippy::unwrap_used)]
async fn setup(server: &TestServer) {
    server
        .client
        .post(server.url("/api/v1/setup"))
        .json(&json!({
            "username": "giovanni",
            "display_name": "Giovanni",
            "password": "correct horse battery staple"
        }))
        .send()
        .await
        .unwrap();
}
