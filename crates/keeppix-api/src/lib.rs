//! Superficie HTTP di Keeppix. Non contiene SQL: ogni accesso ai dati passa
//! dai repository di `keeppix-db`, che richiedono un `AuthContext`.

pub mod cookie;
pub mod csrf;
pub mod extract;
pub mod json;
pub mod openapi;
pub mod problem;
pub mod ratelimit;
pub mod routes;
pub mod state;

pub mod batch;

pub use extract::{AdminAuth, Auth, SESSION_COOKIE, SessionNotShare, SessionOrShare, ShareAuth};
pub use json::Json;
pub use problem::Problem;
pub use state::AppState;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::http::HeaderValue;
use axum::routing::get;
use tower_http::compression::CompressionLayer;
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::trace::TraceLayer;

/// Content Security Policy restrittiva, **senza deroghe `unsafe-*`**.
///
/// `style-src` ammetteva `'unsafe-inline'` «perché Vue inietta stili scoped a
/// runtime». Non è vero, ed è stato verificato sul bundle prodotto: Vite
/// estrae gli stili scoped delle SFC a build time in un foglio esterno
/// (`dist/index.html` carica `<link rel="stylesheet">` e non contiene un solo
/// `<style>` né un attributo `style=`), e gli stili che Vue e Reka UI
/// impostano davvero a runtime lo fanno via CSSOM (`element.style`), che la CSP
/// non intercetta. La deroga indeboliva una policy dichiarata restrittiva senza
/// comprare nulla; spec §9.5 chiede esplicitamente «CSP senza `unsafe-inline`».
///
/// Se un giorno servisse rimetterla, va detto *cosa* la richiede, con un
/// riferimento verificabile: `keeppix-test-support` fa fallire i test se una
/// deroga `unsafe-*` ricompare in `script-src`.
const CSP: &str = "default-src 'self'; script-src 'self'; style-src 'self'; \
                   img-src 'self' data: blob:; connect-src 'self'; frame-ancestors 'none'; \
                   base-uri 'none'; form-action 'self'";

/// HSTS, richiesto da spec §9.5. Un anno con i sottodomini.
///
/// È incondizionato di proposito, e non rompe l'accesso in chiaro in LAN: un
/// browser **ignora** `Strict-Transport-Security` ricevuto su HTTP, per
/// definizione (RFC 6797 §8.1), quindi l'header ha effetto solo dove esiste già
/// il TLS che dichiara di pretendere — cioè dietro il reverse proxy che
/// `docs/DEPLOY.md` descrive come installazione normale. `preload` **non** è
/// incluso: iscriverebbe il dominio dell'utente a una lista globale
/// difficilmente reversibile, e non è una decisione che Keeppix può prendere
/// per lui.
const HSTS: &str = "max-age=31536000; includeSubDomains";

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
        .layer(SetResponseHeaderLayer::overriding(
            axum::http::header::STRICT_TRANSPORT_SECURITY,
            HeaderValue::from_static(HSTS),
        ))
        // Spec §9.4: «`Cache-Control: private` su tutto ciò che è autenticato».
        // `if_not_present`, **non** `overriding`: le rotte che impostano una
        // propria politica di cache devono vincere — gli asset hashati del
        // frontend escono con `public, max-age=31536000, immutable`
        // (`keeppix_server::embed`). `/media/*` è autenticato: `private` +
        // `immutable` sulla rotta (i link pubblici arrivano in Fase 3).
        .layer(SetResponseHeaderLayer::if_not_present(
            axum::http::header::CACHE_CONTROL,
            HeaderValue::from_static("private"),
        ))
        .layer(CompressionLayer::new().br(true).gzip(true))
        .layer(TraceLayer::new_for_http())
}

#[allow(clippy::too_many_lines)]
fn api_routes() -> Router<AppState> {
    Router::new()
        .route("/setup/status", get(routes::setup::status))
        .route("/setup", axum::routing::post(routes::setup::create))
        .route("/auth/login", axum::routing::post(routes::auth::login))
        .route("/auth/refresh", axum::routing::post(routes::auth::refresh))
        .route("/auth/logout", axum::routing::post(routes::auth::logout))
        .route("/auth/me", get(routes::auth::me))
        .route("/timeline/buckets", get(routes::timeline::buckets))
        .route("/timeline", get(routes::timeline::page))
        .route("/folders/tree", get(routes::folders::tree))
        .route("/folders/{id}/children", get(routes::folders::children))
        .route(
            "/folders/{id}",
            axum::routing::patch(routes::folders::relocate),
        )
        .route("/viewport", axum::routing::post(routes::viewport::promote))
        .route("/search", axum::routing::post(routes::search::run))
        .route("/search/suggest", get(routes::search::suggest))
        .route("/places/reverse", get(routes::places::reverse))
        .route("/places/suggest", get(routes::places::suggest))
        .route(
            "/saved-searches",
            get(routes::search::list_saved).post(routes::search::create_saved),
        )
        .route("/ws/ticket", axum::routing::post(routes::ws::ticket))
        .route("/ws", get(routes::ws::connect))
        .route("/problems", get(routes::problems::list))
        .route("/duplicates", get(routes::duplicates::list))
        .route(
            "/duplicates/{content_hash}",
            get(routes::duplicates::members),
        )
        .route(
            "/duplicates/{content_hash}/resolve",
            axum::routing::post(routes::duplicates::resolve),
        )
        .route("/libraries/preview", get(routes::libraries::preview))
        .route(
            "/libraries",
            get(routes::libraries::list).post(routes::libraries::create),
        )
        .route(
            "/libraries/{id}",
            get(routes::libraries::get)
                .patch(routes::libraries::patch)
                .delete(routes::libraries::delete),
        )
        .route(
            "/libraries/{id}/scan",
            get(routes::libraries::scan_status).post(routes::libraries::start_scan),
        )
        .route(
            "/groups",
            get(routes::groups::list).post(routes::groups::create),
        )
        .route(
            "/groups/{id}",
            axum::routing::patch(routes::groups::patch).delete(routes::groups::delete),
        )
        .route("/groups/{id}/members", get(routes::groups::list_members))
        .route(
            "/groups/{group_id}/members/{user_id}",
            axum::routing::post(routes::groups::add_member).delete(routes::groups::remove_member),
        )
        .route(
            "/users",
            get(routes::users::list).post(routes::users::create),
        )
        .route(
            "/users/me/password",
            axum::routing::post(routes::users::change_password),
        )
        .route("/users/{id}", axum::routing::patch(routes::users::patch))
        .route(
            "/users/{id}/disable",
            axum::routing::post(routes::users::disable),
        )
        .route(
            "/users/{id}/enable",
            axum::routing::post(routes::users::enable),
        )
        .route("/assets/{id}", axum::routing::delete(routes::trash::delete))
        .route(
            "/assets/{id}/restore",
            axum::routing::post(routes::trash::restore),
        )
        .route(
            "/assets/{id}/stack/primary",
            axum::routing::post(routes::stacks::set_primary),
        )
        .route("/assets/{id}/stack", get(routes::stacks::get_members))
        .route("/trash", get(routes::trash::list))
        .route("/trash/empty", axum::routing::post(routes::trash::empty))
        .route(
            "/assets/{id}/metadata",
            get(routes::metadata::effective).patch(routes::metadata::apply),
        )
        .route(
            "/metadata/batch",
            axum::routing::post(routes::metadata::apply_batch),
        )
        .route(
            "/metadata/batch/shift-taken-at",
            axum::routing::post(routes::metadata::shift_taken_at),
        )
        .route(
            "/metadata/batch/copy-location",
            axum::routing::post(routes::geotag::copy_location),
        )
        .route(
            "/metadata/batch/import-gpx",
            axum::routing::post(routes::geotag::import_gpx),
        )
        .route(
            "/metadata/batch/{batch_id}/undo",
            axum::routing::post(routes::metadata::undo_batch),
        )
        .route(
            "/assets/{id}/flags",
            get(routes::flags::get).put(routes::flags::set),
        )
        .route(
            "/flags/batch",
            axum::routing::post(routes::flags::batch_set),
        )
        .route(
            "/albums",
            get(routes::albums::list).post(routes::albums::create),
        )
        .route(
            "/albums/{id}",
            get(routes::albums::get)
                .patch(routes::albums::patch)
                .delete(routes::albums::delete),
        )
        .route("/albums/{id}/assets", get(routes::albums::list_assets))
        .route(
            "/albums/{id}/assets/{asset_id}",
            axum::routing::post(routes::albums::add_asset).delete(routes::albums::remove_asset),
        )
        .route(
            "/albums/{id}/assets/{asset_id}/position",
            axum::routing::patch(routes::albums::reorder_asset),
        )
        .route(
            "/permissions",
            get(routes::permissions::list).post(routes::permissions::grant),
        )
        .route("/permissions/explain", get(routes::permissions::explain))
        .route(
            "/permissions/{id}",
            axum::routing::patch(routes::permissions::patch).delete(routes::permissions::revoke),
        )
        .route(
            "/share/links",
            get(routes::share::list_links).post(routes::share::create_link),
        )
        .route(
            "/share/links/{id}",
            axum::routing::delete(routes::share::revoke_link),
        )
        .route(
            "/guest-uploads/{id}/approve",
            axum::routing::post(routes::share::approve_guest_upload),
        )
        .route("/audit", get(routes::audit::list))
        .route("/share/{token}", get(routes::share::public_info))
        .route("/share/{token}/assets", get(routes::share::public_assets))
        .route(
            "/share/{token}/auth",
            axum::routing::post(routes::share::public_auth),
        )
        .route(
            "/share/{token}/uploads",
            axum::routing::post(routes::share::public_upload).layer(DefaultBodyLimit::disable()),
        )
        // Metà server-side della difesa CSRF (spec §9.5): un layer, non un
        // controllo per handler, così le rotte della Fase 1 sono coperte per
        // costruzione. Vedi `csrf.rs` per la proprietà comprata e le deroghe
        // già previste (WebDAV, tus).
        .layer(axum::middleware::from_fn(csrf::require_client_header))
}

fn all_routes() -> Router<AppState> {
    Router::new()
        .route("/health", get(routes::health::get))
        .route("/api/openapi.json", get(openapi::serve))
        .route("/media/thumb/{hash}", get(routes::media::thumb))
        .route("/media/preview/{hash}", get(routes::media::preview))
        .route("/media/full/{hash}", get(routes::media::full))
        .route("/media/original/{id}", get(routes::media::original))
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
