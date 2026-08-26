mod harness;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use harness::TestServer;
use keeppix_test_support::assert_security_headers;
use tower::ServiceExt as _;

/// La variante senza stato di `/health` non tocca il database (usata qui per
/// non richiedere Postgres) — il router *con* stato lo controlla per davvero
/// (`Db::ping`, debito chiuso il 26 agosto: verificato sotto in
/// `health_reports_the_real_database_status`).
fn app() -> axum::Router {
    keeppix_api::router_without_state()
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn health_returns_ok() {
    let response = app()
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = http_body_util::BodyExt::collect(response.into_body())
        .await
        .unwrap()
        .to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["database"], "not checked");
}

/// Il router *con* stato (quello reale, montato in produzione) deve
/// controllare per davvero il database, non limitarsi a rispondere vivo —
/// altrimenti un umano che fa `curl /health` per capire perché Keeppix non
/// funziona vede "ok" anche con Postgres giù. `Db::ping` esisteva dalla Fase
/// 0 senza consumatore (`scripts/wired-exceptions.txt`, chiuso il 26
/// agosto).
#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn health_reports_the_real_database_status() {
    let server = TestServer::start().await;

    let response = server
        .client
        .get(server.url("/health"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let json: serde_json::Value = response.json().await.unwrap();
    assert_eq!(json["database"], "ok");
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn security_headers_are_present() {
    let response = app()
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_security_headers(response.headers());
}

/// Anche il router senza stato deve avere il `method_not_allowed_fallback`:
/// è quello che montano i test, e la coppia di router va tenuta allineata.
#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn wrong_method_returns_problem_json() {
    let response = app()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/problem+json"
    );
    assert_security_headers(response.headers());

    let body = http_body_util::BodyExt::collect(response.into_body())
        .await
        .unwrap()
        .to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["type"], "keeppix/method-not-allowed");
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn unknown_api_path_returns_problem_json() {
    let response = app()
        .oneshot(
            Request::builder()
                .uri("/api/v1/does-not-exist")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/problem+json"
    );
    // Regression coverage: the fallback route must be wrapped by the same
    // security-header layers as matched routes (see the comment on
    // `.fallback(not_found)` in `lib.rs`).
    assert_security_headers(response.headers());

    let body = http_body_util::BodyExt::collect(response.into_body())
        .await
        .unwrap()
        .to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["type"], "keeppix/not-found");
    assert_eq!(json["status"], 404);
}
