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
        "the same Idempotency-Key must return the already-saved payload"
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
        "the same mutation must not re-execute the album creation"
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
        "the conflicting body must not create a new album"
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

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn refresh_replay_with_the_consumed_cookie_returns_the_cached_set_cookie() {
    use harness::plain_client;

    let server = TestServer::start().await;
    let setup = server
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
    assert_eq!(setup.status(), 201);
    let old_cookie = session_cookie_value(&setup);

    let first = plain_client()
        .post(server.url("/api/v1/auth/refresh"))
        .header("cookie", format!("__Host-kpx_session={old_cookie}"))
        .header("idempotency-key", "refresh-retry-1")
        .send()
        .await
        .unwrap();
    assert_eq!(first.status(), 204, "first refresh must succeed");
    let new_cookie = session_cookie_value(&first);
    assert_ne!(old_cookie, new_cookie);

    // Real clients may retry with the pre-rotation cookie after a lost
    // response. Auth fails, but the Idempotency-Key cache must still replay.
    let replay = plain_client()
        .post(server.url("/api/v1/auth/refresh"))
        .header("cookie", format!("__Host-kpx_session={old_cookie}"))
        .header("idempotency-key", "refresh-retry-1")
        .send()
        .await
        .unwrap();
    assert_eq!(
        replay.status(),
        204,
        "replay with the consumed cookie must hit the idempotency cache, not 401"
    );
    let replayed = session_cookie_value(&replay);
    assert_eq!(
        replayed, new_cookie,
        "cached Set-Cookie from the successful refresh must be replayed"
    );
}

#[allow(clippy::unwrap_used, clippy::expect_used)]
fn session_cookie_value(response: &reqwest::Response) -> String {
    response
        .headers()
        .get("set-cookie")
        .expect("set-cookie present")
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .trim_start_matches("__Host-kpx_session=")
        .to_owned()
}
