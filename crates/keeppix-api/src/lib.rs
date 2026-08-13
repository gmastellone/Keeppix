//! Superficie HTTP di Keeppix. Non contiene SQL: ogni accesso ai dati passa
//! dai repository di `keeppix-db`, che richiedono un `AuthContext`.

pub mod cookie;
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
pub fn router(state: AppState) -> Router {
    base_router().with_state(state)
}

/// Router senza stato, per i test che non toccano il database.
pub fn router_without_state() -> Router {
    base_router_stateless()
}

fn common_layers<S: Clone + Send + Sync + 'static>(router: Router<S>) -> Router<S> {
    router
        // `fallback` must be set *before* the `.layer(...)` calls below: in
        // axum 0.8, `Router::fallback` replaces the router's fallback
        // service directly, while `.layer()` only wraps whatever fallback
        // is already present at the time it's called. If `.fallback()` ran
        // last, the security headers, compression and tracing layers would
        // never apply to unmatched routes (404s) — silently reintroducing
        // that bug by moving this call is easy, so don't reorder it.
        .fallback(not_found)
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

fn api_routes() -> Router<AppState> {
    Router::new()
        .route("/setup/status", get(routes::setup::status))
        .route("/setup", axum::routing::post(routes::setup::create))
        .route("/auth/login", axum::routing::post(routes::auth::login))
        .route("/auth/refresh", axum::routing::post(routes::auth::refresh))
        .route("/auth/logout", axum::routing::post(routes::auth::logout))
        .route("/auth/me", get(routes::auth::me))
}

fn base_router() -> Router<AppState> {
    common_layers(
        Router::new()
            .route("/health", get(routes::health::get))
            .nest("/api/v1", api_routes()),
    )
}

fn base_router_stateless() -> Router {
    common_layers(Router::new().route("/health", get(routes::health::get)))
}

async fn not_found() -> Problem {
    Problem::not_found()
}
