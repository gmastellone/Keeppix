use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode, header};
use tower::ServiceExt as _;

/// `/health` non tocca il database, quindi il test non ha bisogno di Postgres.
fn app() -> axum::Router {
    keeppix_api::router_without_state()
}

/// Asserzioni condivise sui quattro header di sicurezza. Usata sia per una
/// rotta esistente (`/health`) sia per il fallback 404, così che i due
/// percorsi siano dimostrabilmente coperti dagli stessi controlli: se i
/// layer smettono di avvolgere una delle due rotte, uno dei due test fallisce.
#[allow(clippy::unwrap_used)]
fn assert_security_headers(headers: &HeaderMap) {
    assert_eq!(headers.get("x-content-type-options").unwrap(), "nosniff");
    assert_eq!(headers.get("referrer-policy").unwrap(), "no-referrer");
    assert!(headers.get("content-security-policy").is_some());
    assert_eq!(
        headers.get("permissions-policy").unwrap(),
        "camera=(), microphone=(), geolocation=()"
    );
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
