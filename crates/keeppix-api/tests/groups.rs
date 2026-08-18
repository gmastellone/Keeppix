#![allow(clippy::unwrap_used, clippy::expect_used)]

mod harness;

use harness::TestServer;
use serde_json::json;

async fn setup_admin(server: &TestServer) {
    let response = server
        .client
        .post(server.url("/api/v1/setup"))
        .json(&json!({
            "username": "admin",
            "display_name": "Admin",
            "password": "correct horse battery staple"
        }))
        .send()
        .await
        .expect("setup");
    assert_eq!(response.status(), 201);
}

async fn login_as(server: &TestServer, username: &str, password: &str) -> reqwest::Client {
    let client = reqwest::Client::builder()
        .cookie_store(true)
        .default_headers(harness::client_headers())
        .build()
        .expect("client");
    let response = client
        .post(server.url("/api/v1/auth/login"))
        .json(&json!({ "username": username, "password": password }))
        .send()
        .await
        .expect("login");
    assert_eq!(response.status(), 200);
    client
}

async fn create_plain_user(server: &TestServer) {
    let resp = server
        .client
        .post(server.url("/api/v1/users"))
        .json(&json!({
            "username": "plain",
            "display_name": "Plain User",
            "password": "correct horse battery staple",
            "role": "user"
        }))
        .send()
        .await
        .expect("create user");
    assert_eq!(resp.status(), 201);
}

#[tokio::test]
async fn a_plain_user_cannot_list_or_create_groups() {
    let server = TestServer::start().await;
    setup_admin(&server).await;
    create_plain_user(&server).await;

    let plain = login_as(&server, "plain", "correct horse battery staple").await;

    let resp = plain
        .get(server.url("/api/v1/groups"))
        .send()
        .await
        .expect("list groups");
    assert_eq!(resp.status(), 403);

    let resp = plain
        .post(server.url("/api/v1/groups"))
        .json(&json!({"name": "hackers"}))
        .send()
        .await
        .expect("create group");
    assert_eq!(resp.status(), 403);
}
