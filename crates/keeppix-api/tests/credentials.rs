mod harness;

use harness::TestServer;
use keeppix_db::AppPasswordRepo;
use serde_json::json;

#[allow(clippy::expect_used)]
async fn setup_admin(server: &TestServer) {
    let response = server
        .client
        .post(server.url("/api/v1/setup"))
        .json(&json!({
            "username": "giovanni",
            "display_name": "Giovanni",
            "password": "correct horse battery staple"
        }))
        .send()
        .await
        .expect("setup");
    assert_eq!(response.status(), 201);
}

#[allow(clippy::expect_used)]
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

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn creating_an_app_password_returns_the_secret_once() {
    let server = TestServer::start().await;
    setup_admin(&server).await;

    let created = server
        .client
        .post(server.url("/api/v1/users/me/app-passwords"))
        .json(&json!({ "label": "MacBook Finder" }))
        .send()
        .await
        .unwrap();
    assert_eq!(created.status(), 201);
    let body: serde_json::Value = created.json().await.unwrap();
    assert_eq!(body["label"], "MacBook Finder");
    let secret = body["secret"].as_str().unwrap().to_owned();
    assert!(!secret.is_empty());

    // Il segreto funziona davvero contro il repository — non solo la forma
    // della risposta, ma la sostanza: è l'hash Argon2id di *questo*
    // segreto, e nessun altro.
    let verified = AppPasswordRepo::new(&server.db)
        .verify("giovanni", &secret)
        .await
        .unwrap();
    assert!(verified.is_some());
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn listing_does_not_expose_the_secret() {
    let server = TestServer::start().await;
    setup_admin(&server).await;

    server
        .client
        .post(server.url("/api/v1/users/me/app-passwords"))
        .json(&json!({ "label": "rclone NAS" }))
        .send()
        .await
        .unwrap();

    let listed = server
        .client
        .get(server.url("/api/v1/users/me/app-passwords"))
        .send()
        .await
        .unwrap();
    assert_eq!(listed.status(), 200);
    let body: serde_json::Value = listed.json().await.unwrap();
    let entries = body.as_array().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["label"], "rclone NAS");
    assert!(entries[0]["last_used_at"].is_null());
    assert!(
        entries[0].get("secret").is_none(),
        "il segreto non deve mai comparire in una GET"
    );
    assert!(
        entries[0].get("secret_hash").is_none(),
        "nemmeno l'hash deve comparire in una GET"
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn deleting_returns_204_and_verify_fails_immediately() {
    let server = TestServer::start().await;
    setup_admin(&server).await;

    let created = server
        .client
        .post(server.url("/api/v1/users/me/app-passwords"))
        .json(&json!({ "label": "telefono" }))
        .send()
        .await
        .unwrap();
    let body: serde_json::Value = created.json().await.unwrap();
    let id = body["id"].as_str().unwrap().to_owned();
    let secret = body["secret"].as_str().unwrap().to_owned();

    // Prima della cancellazione, il segreto è ancora valido.
    let repo = AppPasswordRepo::new(&server.db);
    assert!(repo.verify("giovanni", &secret).await.unwrap().is_some());

    let deleted = server
        .client
        .delete(server.url(&format!("/api/v1/users/me/app-passwords/{id}")))
        .send()
        .await
        .unwrap();
    assert_eq!(deleted.status(), 204);

    assert!(
        repo.verify("giovanni", &secret).await.unwrap().is_none(),
        "una password cancellata deve fallire la verifica immediatamente"
    );

    let listed = server
        .client
        .get(server.url("/api/v1/users/me/app-passwords"))
        .send()
        .await
        .unwrap();
    let listed_body: serde_json::Value = listed.json().await.unwrap();
    assert!(
        listed_body.as_array().unwrap().is_empty(),
        "una app-password revocata non deve comparire nell'elenco"
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn deleting_someone_elses_app_password_is_forbidden_never_not_found() {
    let server = TestServer::start().await;
    setup_admin(&server).await;

    server
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

    let created = server
        .client
        .post(server.url("/api/v1/users/me/app-passwords"))
        .json(&json!({ "label": "MacBook di Giovanni" }))
        .send()
        .await
        .unwrap();
    let id = created.json::<serde_json::Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let mario = login_as(&server, "mario", "mario-password-ok").await;
    let response = mario
        .delete(server.url(&format!("/api/v1/users/me/app-passwords/{id}")))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 403);

    // Un id inesistente sotto lo stesso path deve dare la stessa risposta,
    // così l'endpoint non è un oracolo di esistenza.
    let missing = uuid::Uuid::now_v7();
    let missing_response = mario
        .delete(server.url(&format!("/api/v1/users/me/app-passwords/{missing}")))
        .send()
        .await
        .unwrap();
    assert_eq!(missing_response.status(), 403);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn app_passwords_require_authentication() {
    let server = TestServer::start().await;
    setup_admin(&server).await;

    let plain = harness::plain_client();
    let response = plain
        .get(server.url("/api/v1/users/me/app-passwords"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 401);
}
