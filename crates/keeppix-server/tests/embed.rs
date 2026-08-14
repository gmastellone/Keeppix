use axum::body::Body;
use axum::http::{Request, StatusCode};
use keeppix_test_support::assert_security_headers;
use tower::ServiceExt as _;

/// Il test gira solo quando il frontend è stato compilato: in CI la build del
/// frontend precede quella del backend.
fn frontend_built() -> bool {
    std::path::Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../frontend/dist/index.html"
    ))
    .exists()
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn index_is_served_at_root() {
    if !frontend_built() {
        eprintln!("frontend/dist assente: test saltato");
        return;
    }

    let app = keeppix_server::embed::mount_stateless();
    let response = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers().get("cache-control").unwrap(), "no-cache");
    // Il fallback SPA serve proprio il documento che carica l'intera
    // applicazione: se esce senza header di sicurezza, la pagina principale
    // di Keeppix gira senza CSP in produzione mentre `/health` ce l'ha (vedi
    // il commento su `keeppix_api::with_common_layers`).
    assert_security_headers(response.headers());

    // Prova che l'embed contenga davvero i file compilati da Vite, non un
    // corpo vuoto con status 200: un embed vuoto degraderebbe silenziosamente
    // a "200 senza contenuto" su ogni percorso.
    let body = http_body_util::BodyExt::collect(response.into_body())
        .await
        .unwrap()
        .to_bytes();
    let html = String::from_utf8_lossy(&body);
    assert!(
        html.contains("<script"),
        "index.html incorporato deve contenere il bundle generato da Vite"
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn client_routes_fall_back_to_index() {
    if !frontend_built() {
        eprintln!("frontend/dist assente: test saltato");
        return;
    }

    let app = keeppix_server::embed::mount_stateless();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/albums/42")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "il routing è lato client"
    );
    assert_security_headers(response.headers());
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn api_paths_never_fall_back_to_index() {
    if !frontend_built() {
        eprintln!("frontend/dist assente: test saltato");
        return;
    }

    let app = keeppix_server::embed::mount_stateless();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/nope")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        response.headers().get("content-type").unwrap(),
        "application/problem+json",
        "un client API non deve mai ricevere HTML"
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn media_and_dav_paths_never_fall_back_to_index() {
    if !frontend_built() {
        eprintln!("frontend/dist assente: test saltato");
        return;
    }

    let app = keeppix_server::embed::mount_stateless();
    for path in ["/media/thumb/deadbeef", "/dav/foo"] {
        let response = app
            .clone()
            .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{path}");
        assert_eq!(
            response.headers().get("content-type").unwrap(),
            "application/problem+json",
            "{path} non deve ricevere index.html"
        );
    }
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn assets_are_served_as_immutable() {
    if !frontend_built() {
        eprintln!("frontend/dist assente: test saltato");
        return;
    }

    // Il nome esatto del bundle contiene un hash generato da Vite ad ogni
    // build e non è prevedibile; l'unica cosa stabile è la cartella.
    let asset_path = std::fs::read_dir(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../frontend/dist/assets"
    ))
    .unwrap()
    .filter_map(Result::ok)
    .find_map(|entry| {
        let name = entry.file_name().into_string().ok()?;
        std::path::Path::new(&name)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("js"))
            .then_some(name)
    })
    .expect("la build di Vite produce almeno un bundle .js");

    let app = keeppix_server::embed::mount_stateless();
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/assets/{asset_path}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    // È anche la controprova del layer `Cache-Control: private` aggiunto in
    // `with_common_layers`: usa `if_not_present`, quindi la politica immutabile
    // impostata dall'handler deve sopravvivere. Con `overriding` questo test
    // fallirebbe — e la prima voce della §9.4 (asset hashati mai richiesti due
    // volte) sarebbe annullata dalla sua ultima riga.
    assert_eq!(
        response.headers().get("cache-control").unwrap(),
        "public, max-age=31536000, immutable"
    );
}
