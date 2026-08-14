mod harness;

use harness::TestServer;
use serde_json::json;

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn a_websocket_ticket_cannot_be_reused() {
    let server = TestServer::start().await;
    setup(&server).await;

    let issued = server
        .client
        .post(server.url("/api/v1/ws/ticket"))
        .send()
        .await
        .unwrap();
    assert_eq!(issued.status(), 200);
    let body: serde_json::Value = issued.json().await.unwrap();
    let ticket = body["ticket"].as_str().unwrap();
    assert_eq!(body["expires_in"], 30);

    let origin = server.base_url.clone();
    let first = handshake(&server, ticket, &origin).await;
    assert!(
        first == 101 || first == 400 || first == 426,
        "primo handshake deve consumare il ticket, got {first}"
    );

    let second = handshake(&server, ticket, &origin).await;
    assert_eq!(second, 403, "ticket già usato");
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn a_wrong_origin_is_forbidden() {
    let server = TestServer::start().await;
    setup(&server).await;
    let issued = server
        .client
        .post(server.url("/api/v1/ws/ticket"))
        .send()
        .await
        .unwrap();
    let ticket = issued.json::<serde_json::Value>().await.unwrap()["ticket"]
        .as_str()
        .unwrap()
        .to_owned();

    let status = handshake(&server, &ticket, "https://evil.example").await;
    assert_eq!(status, 403);
}

#[allow(clippy::unwrap_used)]
async fn handshake(server: &TestServer, ticket: &str, origin: &str) -> u16 {
    server
        .client
        .get(server.url("/api/v1/ws"))
        .header("Origin", origin)
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .header("Sec-WebSocket-Version", "13")
        .header("Sec-WebSocket-Key", "dGhlIHNhbXBsZSBub25jZQ==")
        .header(
            "Sec-WebSocket-Protocol",
            format!("keeppix.v1, ticket.{ticket}"),
        )
        .send()
        .await
        .unwrap()
        .status()
        .as_u16()
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
