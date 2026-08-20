mod harness;

use harness::TestServer;
use serde_json::json;

#[allow(clippy::unwrap_used)]
async fn setup(server: &TestServer) {
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
        .unwrap();
    assert_eq!(response.status(), 201);
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn get_preferences_returns_defaults_for_a_new_user() {
    let server = TestServer::start().await;
    setup(&server).await;

    let response = server
        .client
        .get(server.url("/api/v1/users/me/preferences"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["theme"], "chiaro");
    assert_eq!(body["grid_density"]["desktop"], 4);
    assert_eq!(body["grid_density"]["mobile"], 3);
    assert_eq!(body["notifications"]["digest"], true);
    assert_eq!(body["notifications"]["condivisioni"], true);
    assert_eq!(body["notifications"]["problemi"], true);
    assert_eq!(body["language"], "it");
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn patch_merges_without_clearing_other_fields() {
    let server = TestServer::start().await;
    setup(&server).await;

    let seeded = server
        .client
        .patch(server.url("/api/v1/users/me/preferences"))
        .json(&json!({
            "theme": "scuro",
            "grid_density": { "desktop": 10 },
            "notifications": { "problemi": false },
            "language": "en"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(seeded.status(), 200);

    let patched = server
        .client
        .patch(server.url("/api/v1/users/me/preferences"))
        .json(&json!({ "theme": "sistema" }))
        .send()
        .await
        .unwrap();
    assert_eq!(patched.status(), 200);

    let body: serde_json::Value = patched.json().await.unwrap();
    assert_eq!(body["theme"], "sistema");
    assert_eq!(body["grid_density"]["desktop"], 10);
    assert_eq!(body["grid_density"]["mobile"], 3);
    assert_eq!(body["notifications"]["problemi"], false);
    assert_eq!(body["notifications"]["condivisioni"], true);
    assert_eq!(body["language"], "en");
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn an_unknown_top_level_field_is_rejected() {
    let server = TestServer::start().await;
    setup(&server).await;

    let response = server
        .client
        .patch(server.url("/api/v1/users/me/preferences"))
        .json(&json!({ "avatar_color": "#ff0000" }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 400);

    let problem: serde_json::Value = response.json().await.unwrap();
    assert_eq!(problem["type"], "keeppix/unknown-field");

    let unchanged = server
        .client
        .get(server.url("/api/v1/users/me/preferences"))
        .send()
        .await
        .unwrap();
    assert_eq!(unchanged.status(), 200);
    assert_eq!(
        unchanged.json::<serde_json::Value>().await.unwrap()["theme"],
        "chiaro"
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn preferences_require_authentication() {
    let server = TestServer::start().await;

    let get = server
        .client
        .get(server.url("/api/v1/users/me/preferences"))
        .send()
        .await
        .unwrap();
    assert_eq!(get.status(), 401);

    let patch = server
        .client
        .patch(server.url("/api/v1/users/me/preferences"))
        .json(&json!({ "theme": "scuro" }))
        .send()
        .await
        .unwrap();
    assert_eq!(patch.status(), 401);
}
