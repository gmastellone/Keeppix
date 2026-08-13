## Task 9: Stato applicativo, errori RFC 9457 ed extractor di autenticazione

**Files:**
- Create: `crates/keeppix-api/src/state.rs`
- Create: `crates/keeppix-api/src/problem.rs`
- Create: `crates/keeppix-api/src/extract.rs`
- Create: `crates/keeppix-api/src/routes/mod.rs`
- Create: `crates/keeppix-api/src/routes/health.rs`
- Create: `crates/keeppix-api/tests/health.rs`
- Modify: `crates/keeppix-api/src/lib.rs`, `crates/keeppix-api/Cargo.toml`

**Interfaces:**
- Consumes: `Db`, `DbError`, `SessionRepo` (Task 4, 7); `AuthContext`, `SessionToken` (Task 2, 6).
- Produces:
  - `AppState { db: Db, session_ttl: Duration }` clonabile, con `AppState::new(db: Db, session_ttl_secs: u64) -> AppState`.
  - `Problem` con `Problem::new(status: StatusCode, type_slug: &str, title: &str) -> Problem`, `with_detail(self, detail: impl Into<String>) -> Problem`, e `impl IntoResponse` che emette `application/problem+json`. `impl From<DbError> for Problem`.
  - `Auth(pub AuthContext)` — extractor Axum che legge il cookie `__Host-kpx_session`; risponde `401 keeppix/unauthenticated` se assente o non valido.
  - `AdminAuth(pub AuthContext)` — come sopra ma `403 keeppix/forbidden` se non admin.
  - `SESSION_COOKIE: &str = "__Host-kpx_session"`.
  - `router(state: AppState) -> axum::Router` con `GET /health` e gli header di sicurezza applicati.

- [ ] **Step 1: Aggiungere le dipendenze**

```bash
cargo add axum --features macros -p keeppix-api
cargo add axum-extra --features cookie -p keeppix-api
cargo add tower-http --features set-header,trace,compression-br,cors -p keeppix-api
cargo add tower keeppix-db --path crates/keeppix-db -p keeppix-api
cargo add serde serde_json tokio tracing http -p keeppix-api
cargo add --dev tower --features util -p keeppix-api
cargo add --dev http-body-util -p keeppix-api
```

- [ ] **Step 2: Scrivere i test che falliscono**

`crates/keeppix-api/tests/health.rs`:

```rust
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use tower::ServiceExt as _;

/// `/health` non tocca il database, quindi il test non ha bisogno di Postgres.
fn app() -> axum::Router {
    keeppix_api::router_without_state()
}

#[tokio::test]
async fn health_returns_ok() {
    let response = app()
        .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn security_headers_are_present() {
    let response = app()
        .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();

    let headers = response.headers();
    assert_eq!(headers.get("x-content-type-options").unwrap(), "nosniff");
    assert_eq!(headers.get("referrer-policy").unwrap(), "no-referrer");
    assert!(headers.get("content-security-policy").is_some());
    assert_eq!(
        headers.get("permissions-policy").unwrap(),
        "camera=(), microphone=(), geolocation=()"
    );
}

#[tokio::test]
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

    let body = http_body_util::BodyExt::collect(response.into_body())
        .await
        .unwrap()
        .to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["type"], "keeppix/not-found");
    assert_eq!(json["status"], 404);
}
```

- [ ] **Step 3: Eseguire e verificare il fallimento**

Run: `cargo test -p keeppix-api --test health`
Expected: FAIL — `cannot find function router_without_state`.

- [ ] **Step 4: Implementare `problem.rs`**

```rust
use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use keeppix_db::DbError;
use serde::Serialize;

/// Errore in formato RFC 9457. Il campo `type` è un codice stabile su cui i
/// client possono ramificare; `title` è in inglese e serve al debug, non
/// all'utente finale — la traduzione avviene nel frontend.
#[derive(Debug, Serialize)]
pub struct Problem {
    #[serde(rename = "type")]
    pub type_slug: String,
    pub title: String,
    pub status: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(skip)]
    status_code: StatusCode,
}

impl Problem {
    #[must_use]
    pub fn new(status: StatusCode, type_slug: &str, title: &str) -> Self {
        Self {
            type_slug: format!("keeppix/{type_slug}"),
            title: title.to_owned(),
            status: status.as_u16(),
            detail: None,
            status_code: status,
        }
    }

    #[must_use]
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    #[must_use]
    pub fn not_found() -> Self {
        Self::new(StatusCode::NOT_FOUND, "not-found", "Resource not found")
    }

    #[must_use]
    pub fn unauthenticated() -> Self {
        Self::new(StatusCode::UNAUTHORIZED, "unauthenticated", "Authentication required")
    }

    #[must_use]
    pub fn forbidden() -> Self {
        Self::new(StatusCode::FORBIDDEN, "forbidden", "Not allowed")
    }

    #[must_use]
    pub fn internal() -> Self {
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal-error",
            "Unexpected server error",
        )
    }
}

impl IntoResponse for Problem {
    fn into_response(self) -> Response {
        let status = self.status_code;
        let mut response = (status, Json(self)).into_response();
        response.headers_mut().insert(
            axum::http::header::CONTENT_TYPE,
            axum::http::HeaderValue::from_static("application/problem+json"),
        );
        response
    }
}

impl From<DbError> for Problem {
    fn from(err: DbError) -> Self {
        match err {
            DbError::NotFound => Self::not_found(),
            DbError::Forbidden => Self::forbidden(),
            DbError::Conflict(msg) => {
                Self::new(StatusCode::CONFLICT, "conflict", "Conflict").with_detail(msg)
            }
            // I dettagli interni restano nei log, non nella risposta.
            other => {
                tracing::error!(error = %other, "database error");
                Self::internal()
            }
        }
    }
}
```

- [ ] **Step 5: Implementare `state.rs`**

```rust
use std::time::Duration;

use keeppix_db::Db;

#[derive(Clone)]
pub struct AppState {
    pub db: Db,
    pub session_ttl: Duration,
}

impl AppState {
    #[must_use]
    pub const fn new(db: Db, session_ttl_secs: u64) -> Self {
        Self { db, session_ttl: Duration::from_secs(session_ttl_secs) }
    }
}
```

- [ ] **Step 6: Implementare `extract.rs`**

```rust
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum_extra::extract::CookieJar;
use keeppix_db::SessionRepo;
use keeppix_domain::{AuthContext, SessionToken};

use crate::problem::Problem;
use crate::state::AppState;

pub const SESSION_COOKIE: &str = "__Host-kpx_session";

/// Estrae il contesto di autenticazione dal cookie di sessione.
/// Ogni handler che tratta dati di un utente **deve** prendere questo
/// extractor: è il modo in cui l'`AuthContext` raggiunge i repository.
pub struct Auth(pub AuthContext);

impl FromRequestParts<AppState> for Auth {
    type Rejection = Problem;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let jar = CookieJar::from_headers(&parts.headers);
        let cookie = jar.get(SESSION_COOKIE).ok_or_else(Problem::unauthenticated)?;
        let token = SessionToken::from_string(cookie.value().to_owned());

        let ctx = SessionRepo::new(&state.db)
            .authenticate(&token)
            .await
            .map_err(|_| Problem::unauthenticated())?;

        Ok(Self(ctx))
    }
}

/// Come `Auth`, ma rifiuta chi non è amministratore.
pub struct AdminAuth(pub AuthContext);

impl FromRequestParts<AppState> for AdminAuth {
    type Rejection = Problem;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let Auth(ctx) = Auth::from_request_parts(parts, state).await?;
        if !ctx.is_admin() {
            return Err(Problem::forbidden());
        }
        Ok(Self(ctx))
    }
}
```

- [ ] **Step 7: Implementare `routes/health.rs` e `routes/mod.rs`**

`routes/health.rs`:

```rust
use axum::Json;
use serde::Serialize;

#[derive(Serialize)]
pub struct Health {
    status: &'static str,
    version: &'static str,
}

pub async fn get() -> Json<Health> {
    Json(Health { status: "ok", version: env!("CARGO_PKG_VERSION") })
}
```

`routes/mod.rs`:

```rust
pub mod health;
```

- [ ] **Step 8: Implementare `lib.rs`**

```rust
//! Superficie HTTP di Keeppix. Non contiene SQL: ogni accesso ai dati passa
//! dai repository di `keeppix-db`, che richiedono un `AuthContext`.

pub mod extract;
pub mod problem;
pub mod routes;
pub mod state;

pub use extract::{AdminAuth, Auth, SESSION_COOKIE};
pub use problem::Problem;
pub use state::AppState;

use axum::Router;
use axum::http::HeaderValue;
use axum::routing::get;
use tower_http::compression::CompressionLayer;
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::trace::TraceLayer;

/// Content Security Policy restrittiva. `style-src` ammette `unsafe-inline`
/// perché Vue inietta stili scoped a runtime; gli script no.
const CSP: &str = "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; \
                   img-src 'self' data: blob:; connect-src 'self'; frame-ancestors 'none'; \
                   base-uri 'none'; form-action 'self'";

/// Router con stato, montato dal binario.
#[must_use]
pub fn router(state: AppState) -> Router {
    base_router().with_state(state)
}

/// Router senza stato, per i test che non toccano il database.
#[must_use]
pub fn router_without_state() -> Router {
    base_router_stateless()
}

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
        .fallback(not_found)
}

fn base_router() -> Router<AppState> {
    common_layers(Router::new().route("/health", get(routes::health::get)))
}

fn base_router_stateless() -> Router {
    common_layers(Router::new().route("/health", get(routes::health::get)))
}

async fn not_found() -> Problem {
    Problem::not_found()
}
```

- [ ] **Step 9: Eseguire i test**

Run: `cargo test -p keeppix-api --test health`
Expected: PASS — 3 test.

- [ ] **Step 10: Verificare che il server compili e si avvii**

```bash
docker run -d --name keeppix-dev -e POSTGRES_PASSWORD=postgres -p 55432:5432 postgis/postgis:17-3.5
sleep 5
DATABASE_URL=postgres://postgres:postgres@127.0.0.1:55432/postgres \
KEEPPIX_LOG_FORMAT=pretty KEEPPIX_DATA_DIR=./data \
cargo run --bin keeppix -- --config ./nonexistent.toml serve
```

In un altro terminale: `curl -i http://127.0.0.1:5673/health`
Expected: `200 OK`, corpo `{"status":"ok","version":"0.1.0"}`, header di sicurezza presenti.

- [ ] **Step 11: Commit**

```bash
git add crates/keeppix-api crates/keeppix-server
git commit -m "feat(api): add app state, rfc9457 problems and auth extractors"
```

---

