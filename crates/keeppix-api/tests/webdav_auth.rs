//! `WebDAV` scaffolding — router, app-password authentication, CSRF
//! exemption. No real `WebDAV` method yet: this only verifies that Basic
//! auth actually works (and that the login password doesn't count as an
//! app password), and that the CSRF exemption doesn't widen `/api/v1`'s
//! perimeter.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod harness;
mod journey;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use harness::TestServer;
use journey::{ADMIN_PASSWORD, setup_admin};
use serde_json::json;

fn basic_auth_header(username: &str, secret: &str) -> String {
    let raw = format!("{username}:{secret}");
    format!("Basic {}", STANDARD.encode(raw))
}

/// No `Authorization` → `401` with `WWW-Authenticate: Basic`, not a
/// redirect and not a 404: a WebDAV client (Finder, rclone) must be able
/// to tell "authentication is required here" apart from "this path
/// doesn't exist".
#[tokio::test]
async fn dav_without_authorization_returns_401_with_www_authenticate_header() {
    let server = TestServer::start().await;

    let response = server
        .client
        .get(server.url("/dav/anything"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 401);
    let header = response
        .headers()
        .get("www-authenticate")
        .expect("www-authenticate header")
        .to_str()
        .unwrap();
    assert_eq!(header, r#"Basic realm="Keeppix""#);
}

/// A valid app-password passes authentication: there's no real dispatch
/// yet at this point, so the response is `501`, but **not** `401`.
#[tokio::test]
async fn dav_with_valid_app_password_does_not_return_401() {
    let server = TestServer::start().await;
    setup_admin(&server).await;

    let created = server
        .client
        .post(server.url("/api/v1/users/me/app-passwords"))
        .json(&json!({ "label": "Finder" }))
        .send()
        .await
        .unwrap();
    assert_eq!(created.status(), 201);
    let body: serde_json::Value = created.json().await.unwrap();
    let secret = body["secret"].as_str().unwrap().to_owned();

    let response = server
        .client
        .get(server.url("/dav/anything"))
        .header("authorization", basic_auth_header("giovanni", &secret))
        .send()
        .await
        .unwrap();

    assert_ne!(response.status(), 401);
    assert_eq!(response.status(), 501);
}

/// The most important test: app passwords are not login passwords. Using
/// `username:login_password` as Basic Auth on `/dav/` must fail with
/// `401`, not pass "for convenience".
#[tokio::test]
async fn dav_with_login_password_returns_401() {
    let server = TestServer::start().await;
    setup_admin(&server).await;

    let response = server
        .client
        .get(server.url("/dav/anything"))
        .header(
            "authorization",
            basic_auth_header("giovanni", ADMIN_PASSWORD),
        )
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 401);
}

/// The CSRF exemption for `/dav/*` must not weaken `/api/v1`: a mutation
/// without `x-keeppix-client` stays blocked with `403`.
#[tokio::test]
async fn csrf_exemption_does_not_affect_api_v1() {
    let server = TestServer::start().await;
    setup_admin(&server).await;

    // Deliberately without `x-keeppix-client`: same "forged" client as
    // `auth.rs::a_mutation_without_the_client_header_is_rejected`.
    let forged = reqwest::Client::new();
    let response = forged
        .post(server.url("/api/v1/auth/logout"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 403);
}
