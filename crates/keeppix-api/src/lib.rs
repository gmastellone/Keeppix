//! Superficie HTTP di Keeppix. Non contiene SQL: ogni accesso ai dati passa
//! dai repository di `keeppix-db`, che richiedono un `AuthContext`.

pub mod cookie;
pub mod extract;
pub mod json;
pub mod openapi;
pub mod problem;
pub mod routes;
pub mod state;

pub use extract::{AdminAuth, Auth, SESSION_COOKIE};
pub use json::Json;
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

/// Router con stato, montato dai test che vogliono un 404 in JSON invece del
/// fallback SPA (quest'ultimo lo aggiunge solo il binario, vedi
/// `keeppix_server::embed::mount`).
pub fn router(state: AppState) -> Router {
    with_common_layers(all_routes().fallback(not_found)).with_state(state)
}

/// Router senza stato, per i test che non toccano il database.
pub fn router_without_state() -> Router {
    with_common_layers(
        Router::new()
            .route("/health", get(routes::health::get))
            .route("/api/openapi.json", get(openapi::serve))
            .method_not_allowed_fallback(method_not_allowed)
            .fallback(not_found),
    )
}

/// Rotte di Keeppix, **senza** layer né fallback: chi le monta decide sia il
/// fallback (404 JSON qui sopra, SPA nel binario) sia il momento in cui
/// applicare `with_common_layers` — che deve essere *dopo* aver impostato il
/// fallback, per il motivo spiegato lì.
pub fn router_parts() -> Router<AppState> {
    all_routes()
}

/// Applica gli strati comuni a tutte le risposte del server: header di
/// sicurezza, compressione, tracing. **Il router passato deve già avere il
/// proprio fallback impostato** — in axum 0.8 `Router::fallback` sostituisce
/// direttamente il servizio di fallback, mentre `.layer()` avvolge soltanto
/// il fallback già presente al momento in cui viene chiamato. Se si aggiunge
/// un fallback *dopo* questa funzione, quel fallback esce senza CSP,
/// `x-content-type-options`, `referrer-policy` e `permissions-policy` —
/// per un 404 è già un difetto (corretto nel Task 9), ma per il fallback SPA
/// del binario (Task 13) sarebbe peggio: è proprio `index.html`, il documento
/// che carica l'intera applicazione, che uscirebbe senza CSP in produzione.
/// Non spostare l'ordine: fallback prima, `with_common_layers` dopo.
pub fn with_common_layers<S: Clone + Send + Sync + 'static>(router: Router<S>) -> Router<S> {
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

fn api_routes() -> Router<AppState> {
    Router::new()
        .route("/setup/status", get(routes::setup::status))
        .route("/setup", axum::routing::post(routes::setup::create))
        .route("/auth/login", axum::routing::post(routes::auth::login))
        .route("/auth/refresh", axum::routing::post(routes::auth::refresh))
        .route("/auth/logout", axum::routing::post(routes::auth::logout))
        .route("/auth/me", get(routes::auth::me))
}

fn all_routes() -> Router<AppState> {
    Router::new()
        .route("/health", get(routes::health::get))
        .route("/api/openapi.json", get(openapi::serve))
        .nest("/api/v1", api_routes())
        // Va chiamata **dopo** aver registrato le rotte: imposta il fallback
        // di ogni `MethodRouter` già presente, e un `route(...)` aggiunto in
        // seguito tornerebbe al `405` a corpo vuoto di axum. Stessa classe di
        // trappola dell'ordine di `.fallback(...)` documentato sotto.
        .method_not_allowed_fallback(method_not_allowed)
}

async fn not_found() -> Problem {
    Problem::not_found()
}

/// `405` dentro il contratto RFC 9457: senza questo fallback axum risponde con
/// un corpo vuoto e nessun `type`, e un client che ramifica sul codice
/// d'errore non ha niente da leggere.
async fn method_not_allowed() -> Problem {
    Problem::method_not_allowed()
}
