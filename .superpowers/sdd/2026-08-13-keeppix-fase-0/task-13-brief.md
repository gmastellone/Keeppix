## Task 13: Frontend incorporato nel binario

**Files:**
- Create: `crates/keeppix-server/src/embed.rs`
- Create: `crates/keeppix-server/tests/embed.rs`
- Modify: `crates/keeppix-server/src/lib.rs`, `crates/keeppix-server/src/main.rs`, `crates/keeppix-server/Cargo.toml`

**Interfaces:**
- Consumes: `router(state)` (Task 9-11); `frontend/dist` (Task 12).
- Produces: `embed::spa_fallback() -> axum::routing::MethodRouter` e `embed::mount(router: Router<AppState>) -> Router<AppState>`.

Comportamento: `/assets/*` servito con `Cache-Control: immutable` (i nomi contengono l'hash del contenuto), `index.html` con `no-cache`, e qualunque percorso non-API restituisce `index.html` perché il routing è lato client. I percorsi sotto `/api` non ricadono mai nel fallback: devono restituire `404 problem+json`.

- [ ] **Step 1: Aggiungere le dipendenze**

```bash
cargo add rust-embed --features interpolate-folder-path -p keeppix-server
cargo add mime_guess -p keeppix-server
```

- [ ] **Step 2: Scrivere i test che falliscono**

`crates/keeppix-server/tests/embed.rs`:

```rust
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt as _;

/// Il test gira solo quando il frontend è stato compilato: in CI la build del
/// frontend precede quella del backend.
fn frontend_built() -> bool {
    std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/dist/index.html"))
        .exists()
}

#[tokio::test]
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
}

#[tokio::test]
async fn client_routes_fall_back_to_index() {
    if !frontend_built() {
        return;
    }

    let app = keeppix_server::embed::mount_stateless();
    let response = app
        .oneshot(Request::builder().uri("/login").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK, "il routing è lato client");
}

#[tokio::test]
async fn api_paths_never_fall_back_to_index() {
    if !frontend_built() {
        return;
    }

    let app = keeppix_server::embed::mount_stateless();
    let response = app
        .oneshot(Request::builder().uri("/api/v1/nope").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        response.headers().get("content-type").unwrap(),
        "application/problem+json",
        "un client API non deve mai ricevere HTML"
    );
}
```

- [ ] **Step 3: Eseguire e verificare il fallimento**

Run: `cd frontend && npm run build && cd .. && cargo test -p keeppix-server --test embed`
Expected: FAIL — `cannot find module embed`.

- [ ] **Step 4: Implementare `embed.rs`**

```rust
use axum::body::Body;
use axum::http::{HeaderValue, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use keeppix_api::{AppState, Problem};
use rust_embed::Embed;

#[derive(Embed)]
#[folder = "$CARGO_MANIFEST_DIR/../../frontend/dist"]
struct Assets;

/// Serve un file incorporato oppure `index.html` come fallback SPA.
/// I percorsi API non arrivano qui: sono registrati prima nel router.
async fn serve(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');

    // Difesa in profondità: se un giorno l'ordine delle rotte cambiasse,
    // un client API non deve comunque ricevere HTML.
    if path.starts_with("api/") {
        return Problem::not_found().into_response();
    }

    if let Some(file) = Assets::get(path) {
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        // I nomi dei bundle contengono l'hash del contenuto: sono immutabili.
        let cache = if path.starts_with("assets/") {
            "public, max-age=31536000, immutable"
        } else {
            "no-cache"
        };

        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, HeaderValue::from_str(mime.as_ref()).unwrap_or(
                HeaderValue::from_static("application/octet-stream"),
            ))
            .header(header::CACHE_CONTROL, HeaderValue::from_static(cache))
            .body(Body::from(file.data.into_owned()))
            .unwrap_or_else(|_| Problem::internal().into_response());
    }

    match Assets::get("index.html") {
        Some(index) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, HeaderValue::from_static("text/html; charset=utf-8"))
            .header(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"))
            .body(Body::from(index.data.into_owned()))
            .unwrap_or_else(|_| Problem::internal().into_response()),
        None => Problem::not_found().into_response(),
    }
}

/// Aggiunge il fallback SPA a un router con stato.
#[must_use]
pub fn mount(router: axum::Router<AppState>) -> axum::Router<AppState> {
    router.fallback(get(serve))
}

/// Router minimo per i test: solo API 404 + fallback SPA.
#[must_use]
pub fn mount_stateless() -> axum::Router {
    axum::Router::new().fallback(get(serve))
}
```

- [ ] **Step 5: Esportare e montare**

In `crates/keeppix-server/src/lib.rs` aggiungere `pub mod embed;`.

In `main.rs`, sostituire la costruzione del router:

```rust
    let app = keeppix_server::embed::mount(keeppix_api::router_parts())
        .with_state(keeppix_api::AppState::new(db, config.session_ttl_secs));
```

E in `keeppix-api/src/lib.rs` separare gli strati dal fallback, sostituendo `common_layers`, `base_router` e `base_router_stateless` con:

```rust
/// Strati comuni a tutti i router. Non registra alcun fallback: chi monta
/// decide se rispondere 404 in JSON (API pura) o servire la SPA (binario).
fn common_layers<S: Clone + Send + Sync + 'static>(router: Router<S>) -> Router<S> {
    router
        .layer(SetResponseHeaderLayer::overriding(
            axum::http::header::HeaderName::from_static("x-content-type-options"),
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            axum::http::header::HeaderName::from_static("referrer-policy"),
            HeaderValue::from_static("no-referrer"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            axum::http::header::HeaderName::from_static("content-security-policy"),
            HeaderValue::from_static(CSP),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            axum::http::header::HeaderName::from_static("permissions-policy"),
            HeaderValue::from_static("camera=(), microphone=(), geolocation=()"),
        ))
        .layer(CompressionLayer::new().br(true).gzip(true))
        .layer(TraceLayer::new_for_http())
}

fn all_routes() -> Router<AppState> {
    Router::new()
        .route("/health", get(routes::health::get))
        .route("/api/openapi.json", get(openapi::serve))
        .nest("/api/v1", api_routes())
}

/// Router con tutte le rotte ma **senza** fallback: il binario aggiunge il
/// proprio, che serve il frontend incorporato.
#[must_use]
pub fn router_parts() -> Router<AppState> {
    common_layers(all_routes())
}

/// Router completo con fallback 404 in `problem+json`. Usato dai test API.
#[must_use]
pub fn router(state: AppState) -> Router {
    router_parts().fallback(not_found).with_state(state)
}

/// Router senza stato per i test che non toccano il database.
#[must_use]
pub fn router_without_state() -> Router {
    common_layers(
        Router::new()
            .route("/health", get(routes::health::get))
            .route("/api/openapi.json", get(openapi::serve)),
    )
    .fallback(not_found)
}
```

Verificare che `router_without_state` non richieda più `AppState`: le due rotte che espone (`/health` e `/api/openapi.json`) sono handler senza stato, quindi il tipo `Router` (senza parametro) è corretto.

- [ ] **Step 6: Eseguire i test**

Run: `cargo test -p keeppix-server`
Expected: PASS — 3 test di embed + 4 di config.

- [ ] **Step 7: Verificare a mano il binario completo**

```bash
cd frontend && npm run build && cd ..
DATABASE_URL=postgres://postgres:postgres@127.0.0.1:55432/postgres \
KEEPPIX_LOG_FORMAT=pretty cargo run --release --bin keeppix -- --config ./nonexistent.toml
```

Aprire `http://127.0.0.1:5673`: il frontend deve essere servito dal binario, senza Vite.

- [ ] **Step 8: Commit**

```bash
git add crates/keeppix-server crates/keeppix-api
git commit -m "feat(server): embed the frontend and add spa fallback"
```

---

