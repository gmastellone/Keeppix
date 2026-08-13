use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode};
use tower::ServiceExt as _;

/// Il documento non tocca il database: nasce dalle annotazioni sui tipi.
fn app() -> axum::Router {
    keeppix_api::router_without_state()
}

/// Copia locale delle asserzioni di `tests/health.rs`: ogni file di test è un
/// binario a sé, quindi l'helper non è condivisibile senza un modulo comune.
/// I quattro header sono gli stessi applicati da `common_layers`.
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
async fn openapi_document_is_served_and_complete() {
    let response = app()
        .oneshot(
            Request::builder()
                .uri("/api/openapi.json")
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
    let doc: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(doc["openapi"].as_str().unwrap(), "3.1.0");
    assert_eq!(doc["info"]["title"], "Keeppix API");

    for path in [
        "/api/v1/setup/status",
        "/api/v1/setup",
        "/api/v1/auth/login",
        "/api/v1/auth/refresh",
        "/api/v1/auth/logout",
        "/api/v1/auth/me",
    ] {
        assert!(doc["paths"][path].is_object(), "manca il percorso {path}");
    }

    assert!(doc["components"]["schemas"]["UserView"].is_object());
}

/// Pin sul punto in cui è facile sbagliare: la rotta va montata **dentro**
/// l'argomento di `common_layers`, non concatenata dopo la sua chiamata. Nel
/// secondo caso i `.layer(...)` non la avvolgerebbero e il documento uscirebbe
/// senza CSP, nosniff, referrer-policy e permissions-policy — lo stesso difetto
/// che il fallback 404 ha già pinnato in `tests/health.rs`.
#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn openapi_document_carries_the_security_headers() {
    let response = app()
        .oneshot(
            Request::builder()
                .uri("/api/openapi.json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_security_headers(response.headers());
}

/// Blocca la specifica su disco. Se cambia, il test fallisce e mostra il diff:
/// aggiornare `docs/api/openapi.json` è una scelta consapevole, non un effetto
/// collaterale di un refactoring.
#[test]
#[allow(clippy::unwrap_used)]
fn openapi_snapshot_matches_the_committed_file() {
    let generated =
        serde_json::to_string_pretty(&<keeppix_api::openapi::ApiDoc as utoipa::OpenApi>::openapi())
            .unwrap();

    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/api/openapi.json");

    if !path.exists() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &generated).unwrap();
        return;
    }

    let committed = std::fs::read_to_string(&path).unwrap();
    assert_eq!(
        committed.trim(),
        generated.trim(),
        "la specifica è cambiata: rigenerare con `rm docs/api/openapi.json && cargo test`"
    );
}
