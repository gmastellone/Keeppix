//! Task 5 (Fase 5): scaffolding `WebDAV` — router, autenticazione via
//! app-password, deroga CSRF. Nessun metodo `WebDAV` reale ancora: qui si
//! verifica solo che l'autenticazione Basic funzioni davvero (e che la
//! password di login non valga come app-password), e che la deroga CSRF
//! non allarghi il perimetro di `/api/v1`.
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

/// Nessun `Authorization` → `401` con `WWW-Authenticate: Basic`, non un
/// redirect e non un 404: un client WebDAV (Finder, rclone) deve poter
/// distinguere "qui serve autenticarsi" da "questo percorso non esiste".
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

/// Un'app-password valida supera l'autenticazione: in questa fase non c'è
/// ancora un dispatch reale, quindi la risposta è `501`, ma **non** `401`.
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

/// Il test più importante: le app-password non sono le password di login.
/// Usare `username:password_di_login` come Basic Auth su `/dav/` deve
/// fallire con `401`, non passare "per comodità".
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

/// La deroga CSRF per `/dav/*` non deve indebolire `/api/v1`: una mutazione
/// senza `x-keeppix-client` resta bloccata con `403`.
#[tokio::test]
async fn csrf_exemption_does_not_affect_api_v1() {
    let server = TestServer::start().await;
    setup_admin(&server).await;

    // Deliberatamente senza `x-keeppix-client`: stesso client "forgiato" di
    // `auth.rs::a_mutation_without_the_client_header_is_rejected`.
    let forged = reqwest::Client::new();
    let response = forged
        .post(server.url("/api/v1/auth/logout"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 403);
}
