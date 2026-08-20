mod harness;

use harness::TestServer;
use serde_json::json;

const CHROME_MACOS: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 \
    (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36";
const FIREFOX_WINDOWS: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:128.0) Gecko/20100101 Firefox/128.0";

#[allow(clippy::unwrap_used)]
async fn setup(server: &TestServer) {
    let response = server
        .client
        .post(server.url("/api/v1/setup"))
        .header(reqwest::header::USER_AGENT, CHROME_MACOS)
        .json(&json!({
            "username": "giovanni",
            "display_name": "Giovanni",
            "password": "correct horse battery staple"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 201);
}

/// Un secondo "dispositivo": client con proprio cookie-jar, proprio
/// `User-Agent`, loggato sullo stesso utente creato da `setup`.
#[allow(clippy::unwrap_used)]
async fn login_second_device(server: &TestServer, user_agent: &str) -> reqwest::Client {
    let device = reqwest::Client::builder()
        .cookie_store(true)
        .default_headers(harness::client_headers())
        .build()
        .unwrap();

    let response = device
        .post(server.url("/api/v1/auth/login"))
        .header(reqwest::header::USER_AGENT, user_agent)
        .json(&json!({ "username": "giovanni", "password": "correct horse battery staple" }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    device
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn a_lone_session_lists_itself_as_current_with_a_device_label() {
    let server = TestServer::start().await;
    setup(&server).await;

    let response = server
        .client
        .get(server.url("/api/v1/users/me/sessions"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    let body: serde_json::Value = response.json().await.unwrap();
    let sessions = body.as_array().unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0]["current"], true);
    assert_eq!(sessions[0]["device_label"], "Chrome on macOS");
    assert!(sessions[0]["id"].is_string());
    assert!(sessions[0]["last_seen_at"].is_string());
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn a_second_device_shows_up_and_only_the_caller_is_marked_current() {
    let server = TestServer::start().await;
    setup(&server).await;
    let device_b = login_second_device(&server, FIREFOX_WINDOWS).await;

    let from_a: serde_json::Value = server
        .client
        .get(server.url("/api/v1/users/me/sessions"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let sessions_a = from_a.as_array().unwrap();
    assert_eq!(sessions_a.len(), 2);
    assert_eq!(
        sessions_a.iter().filter(|s| s["current"] == true).count(),
        1
    );

    let from_b: serde_json::Value = device_b
        .get(server.url("/api/v1/users/me/sessions"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let sessions_b = from_b.as_array().unwrap();
    assert_eq!(sessions_b.len(), 2);

    let current_id_from_a = sessions_a.iter().find(|s| s["current"] == true).unwrap()["id"]
        .as_str()
        .unwrap();
    let current_id_from_b = sessions_b.iter().find(|s| s["current"] == true).unwrap()["id"]
        .as_str()
        .unwrap();
    assert_ne!(
        current_id_from_a, current_id_from_b,
        "ognuno vede sé stesso come corrente, non l'altro dispositivo"
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn revoking_another_session_logs_it_out_without_touching_the_caller() {
    let server = TestServer::start().await;
    setup(&server).await;
    let device_b = login_second_device(&server, FIREFOX_WINDOWS).await;

    let sessions: serde_json::Value = server
        .client
        .get(server.url("/api/v1/users/me/sessions"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let b_id = sessions
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["current"] == false)
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let revoke = server
        .client
        .delete(server.url(&format!("/api/v1/users/me/sessions/{b_id}")))
        .send()
        .await
        .unwrap();
    assert_eq!(revoke.status(), 204);

    let b_me = device_b
        .get(server.url("/api/v1/auth/me"))
        .send()
        .await
        .unwrap();
    assert_eq!(
        b_me.status(),
        401,
        "il token del dispositivo revocato non deve più autenticare"
    );

    let a_me = server
        .client
        .get(server.url("/api/v1/auth/me"))
        .send()
        .await
        .unwrap();
    assert_eq!(a_me.status(), 200, "la sessione chiamante resta valida");
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn the_current_session_cannot_be_revoked_via_delete() {
    let server = TestServer::start().await;
    setup(&server).await;

    let sessions: serde_json::Value = server
        .client
        .get(server.url("/api/v1/users/me/sessions"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let own_id = sessions.as_array().unwrap()[0]["id"].as_str().unwrap();

    let response = server
        .client
        .delete(server.url(&format!("/api/v1/users/me/sessions/{own_id}")))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 400);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["type"], "keeppix/session-is-current");

    // Non è stata revocata: la sessione chiamante deve ancora funzionare.
    let me = server
        .client
        .get(server.url("/api/v1/auth/me"))
        .send()
        .await
        .unwrap();
    assert_eq!(me.status(), 200);
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn revoking_a_session_that_belongs_to_someone_else_is_forbidden_not_not_found() {
    let server = TestServer::start().await;
    setup(&server).await;

    // Un secondo utente amministra sé stesso creando il proprio account con
    // `/setup` non è possibile due volte; si crea un secondo utente da admin.
    server
        .client
        .post(server.url("/api/v1/users"))
        .json(&json!({
            "username": "mario",
            "display_name": "Mario",
            "password": "correct horse battery staple"
        }))
        .send()
        .await
        .unwrap();

    let mario = reqwest::Client::builder()
        .cookie_store(true)
        .default_headers(harness::client_headers())
        .build()
        .unwrap();
    let login = mario
        .post(server.url("/api/v1/auth/login"))
        .json(&json!({ "username": "mario", "password": "correct horse battery staple" }))
        .send()
        .await
        .unwrap();
    assert_eq!(login.status(), 200);

    let admins_sessions: serde_json::Value = server
        .client
        .get(server.url("/api/v1/users/me/sessions"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let admins_id = admins_sessions.as_array().unwrap()[0]["id"]
        .as_str()
        .unwrap();

    let response = mario
        .delete(server.url(&format!("/api/v1/users/me/sessions/{admins_id}")))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 403);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["type"], "keeppix/forbidden");

    // La sessione dell'admin non deve essere stata toccata dal tentativo.
    let me = server
        .client
        .get(server.url("/api/v1/auth/me"))
        .send()
        .await
        .unwrap();
    assert_eq!(me.status(), 200);
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn revoke_others_logs_out_every_other_device_but_keeps_the_caller() {
    let server = TestServer::start().await;
    setup(&server).await;
    let device_b = login_second_device(&server, FIREFOX_WINDOWS).await;
    let device_c = login_second_device(&server, "curl/8.7.1").await;

    let response = server
        .client
        .post(server.url("/api/v1/users/me/sessions/revoke-others"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 204);

    for other in [&device_b, &device_c] {
        let me = other
            .get(server.url("/api/v1/auth/me"))
            .send()
            .await
            .unwrap();
        assert_eq!(
            me.status(),
            401,
            "i dispositivi non chiamanti devono cadere"
        );
    }

    let a_me = server
        .client
        .get(server.url("/api/v1/auth/me"))
        .send()
        .await
        .unwrap();
    assert_eq!(a_me.status(), 200, "il chiamante resta autenticato");

    let sessions: serde_json::Value = server
        .client
        .get(server.url("/api/v1/users/me/sessions"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(sessions.as_array().unwrap().len(), 1);
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn sessions_endpoints_require_authentication() {
    let server = TestServer::start().await;
    let anonymous = harness::plain_client();

    let list = anonymous
        .get(server.url("/api/v1/users/me/sessions"))
        .send()
        .await
        .unwrap();
    assert_eq!(list.status(), 401);

    let revoke_others = anonymous
        .post(server.url("/api/v1/users/me/sessions/revoke-others"))
        .send()
        .await
        .unwrap();
    assert_eq!(revoke_others.status(), 401);
}
