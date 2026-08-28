mod harness;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use harness::TestServer;
use keeppix_test_support::assert_security_headers;
use tower::ServiceExt as _;

/// The stateless variant of `/health` does not touch the database (used
/// here so this doesn't require Postgres) — the router *with* state
/// actually checks it (`Db::ping`, verified below in
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

/// The router *with* state (the real one, mounted in production) must
/// actually check the database, not just answer that it's alive —
/// otherwise a human running `curl /health` to figure out why Keeppix
/// isn't working sees "ok" even with Postgres down. `Db::ping` existed
/// for a while with no consumer before this test wired it up.
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

/// The stateless router must also have the `method_not_allowed_fallback`:
/// it's the one the tests mount, and the pair of routers must be kept in
/// sync.
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
