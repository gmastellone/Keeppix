mod harness;

use harness::TestServer;
use serde_json::json;

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn replaying_the_same_mutation_key_and_body_returns_the_cached_response() {
    let server = TestServer::start().await;
    setup(&server).await;

    let first = create_album(&server, "album-vacanze", "Vacanze", "Estate 2026").await;
    assert_eq!(first.status(), 201);
    let first_body = first.json::<serde_json::Value>().await.unwrap();

    let replay = create_album(&server, "album-vacanze", "Vacanze", "Estate 2026").await;
    assert_eq!(replay.status(), 201);
    let replay_body = replay.json::<serde_json::Value>().await.unwrap();

    assert_eq!(
        replay_body, first_body,
        "lo stesso Idempotency-Key deve restituire il payload già salvato"
    );

    let albums = server
        .client
        .get(server.url("/api/v1/albums"))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    let list = albums.as_array().unwrap();
    assert_eq!(
        list.len(),
        1,
        "la stessa mutazione non deve rieseguire la creazione dell'album"
    );
    assert_eq!(list[0]["id"], first_body["id"]);
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn reusing_the_same_mutation_key_with_a_different_body_is_a_conflict() {
    let server = TestServer::start().await;
    setup(&server).await;

    let first = create_album(&server, "album-vacanze", "Vacanze", "Estate 2026").await;
    assert_eq!(first.status(), 201);

    let conflict = create_album(&server, "album-vacanze", "Lavoro", "Cliente ACME").await;
    assert_eq!(conflict.status(), 409);
    let body = conflict.json::<serde_json::Value>().await.unwrap();
    assert_eq!(body["type"], "keeppix/idempotency-key-conflict");

    let albums = server
        .client
        .get(server.url("/api/v1/albums"))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    let list = albums.as_array().unwrap();
    assert_eq!(
        list.len(),
        1,
        "il body conflittuale non deve creare un nuovo album"
    );
    assert_eq!(list[0]["name"], "Vacanze");
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

#[allow(clippy::unwrap_used)]
async fn create_album(
    server: &TestServer,
    key: &str,
    name: &str,
    description: &str,
) -> reqwest::Response {
    server
        .client
        .post(server.url("/api/v1/albums"))
        .header("idempotency-key", key)
        .json(&json!({
            "name": name,
            "description": description
        }))
        .send()
        .await
        .unwrap()
}
