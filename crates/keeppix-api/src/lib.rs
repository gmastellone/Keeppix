//! Keeppix's HTTP surface. Contains no SQL: every data access goes through
//! `keeppix-db`'s repositories, which require an `AuthContext`.

pub mod cookie;
pub mod csrf;
pub mod dav;
pub mod extract;
pub mod idempotency;
pub mod json;
pub mod openapi;
pub mod problem;
pub mod ratelimit;
pub mod routes;
pub mod state;

pub mod batch;
pub mod bulk;

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

/// Restrictive Content Security Policy, **with no `unsafe-*` exemptions**.
///
/// `style-src` used to allow `'unsafe-inline'` on the theory that Vue
/// injects scoped styles at runtime. That's not true, and it was verified
/// on the produced bundle: Vite extracts SFC scoped styles at build time
/// into an external stylesheet (`dist/index.html` loads a
/// `<link rel="stylesheet">` and contains not a single `<style>` tag or
/// `style=` attribute), and the styles Vue and Reka UI actually set at
/// runtime do so via the CSSOM (`element.style`), which the CSP doesn't
/// intercept. The exemption weakened a policy declared restrictive without
/// buying anything; the CSP is required to ship with no `unsafe-inline`.
///
/// If it's ever needed again, state *what* requires it, with a verifiable
/// reference: `keeppix-test-support` fails the tests if an `unsafe-*`
/// exemption reappears in `script-src`.
const CSP: &str = "default-src 'self'; script-src 'self'; style-src 'self'; \
                   img-src 'self' data: blob:; connect-src 'self'; frame-ancestors 'none'; \
                   base-uri 'none'; form-action 'self'";

/// HSTS. One year, including subdomains.
///
/// It's unconditional on purpose, and doesn't break plaintext access on a
/// LAN: a browser **ignores** `Strict-Transport-Security` received over
/// HTTP, by definition (RFC 6797 §8.1), so the header only takes effect
/// where the TLS it claims to require already exists — i.e. behind the
/// reverse proxy that `docs/DEPLOY.md` describes as the normal
/// installation. `preload` is **not** included: it would enroll the user's
/// domain in a globally distributed list that's hard to reverse, and that
/// isn't a decision Keeppix can make on their behalf.
const HSTS: &str = "max-age=31536000; includeSubDomains";

/// Stateful router, mounted by tests that want a JSON 404 instead of the
/// SPA fallback (the latter is only added by the binary, see
/// `keeppix_server::embed::mount`).
pub fn router(state: AppState) -> Router {
    with_common_layers(all_routes(state).fallback(not_found))
}

/// Stateless router, for tests that don't touch the database.
pub fn router_without_state() -> Router {
    with_common_layers(
        Router::new()
            .route("/health", get(routes::health::get_without_db))
            .route("/api/openapi.json", get(openapi::serve))
            .method_not_allowed_fallback(method_not_allowed)
            .fallback(not_found),
    )
}

/// Keeppix's routes, **with no** layers or fallback: whoever mounts them
/// decides both the fallback (JSON 404 above, SPA in the binary) and when
/// to apply `with_common_layers` — which must come *after* the fallback is
/// set, for the reason explained there.
pub fn router_parts(state: AppState) -> Router {
    all_routes(state)
}

/// Applies the common layers to every response from the server: security
/// headers, compression, tracing. **The router passed in must already have
/// its own fallback set** — in axum 0.8, `Router::fallback` directly
/// replaces the fallback service, while `.layer()` only wraps whatever
/// fallback is already present at the time it's called. Adding a fallback
/// *after* this function means that fallback comes out with no CSP,
/// `x-content-type-options`, `referrer-policy`, or `permissions-policy` —
/// already a defect for a 404, but worse for the binary's SPA fallback:
/// that's `index.html` itself, the document that loads the whole
/// application, which would ship with no CSP in production. Don't
/// reorder this: fallback first, `with_common_layers` after.
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
        // `Cache-Control: private` on everything authenticated.
        // `if_not_present`, **not** `overriding`: routes that set their own
        // cache policy must win — the frontend's hashed assets come out
        // with `public, max-age=31536000, immutable`
        // (`keeppix_server::embed`). `/media/*` is authenticated: `private`
        // + `immutable` on the route (public links get their own policy).
        .layer(SetResponseHeaderLayer::if_not_present(
            axum::http::header::CACHE_CONTROL,
            HeaderValue::from_static("private"),
        ))
        .layer(CompressionLayer::new().br(true).gzip(true))
        .layer(TraceLayer::new_for_http())
}

#[allow(clippy::too_many_lines)]
fn api_routes(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/setup/status", get(routes::setup::status))
        .route("/setup", axum::routing::post(routes::setup::create))
        .route("/auth/login", axum::routing::post(routes::auth::login))
        .route("/auth/refresh", axum::routing::post(routes::auth::refresh))
        .route("/auth/logout", axum::routing::post(routes::auth::logout))
        .route("/auth/me", get(routes::auth::me))
        .route("/bootstrap", get(routes::bootstrap::get))
        .route(
            "/auth/totp",
            get(routes::totp::status).delete(routes::totp::disable),
        )
        .route("/auth/totp/setup", axum::routing::post(routes::totp::setup))
        .route(
            "/auth/totp/confirm",
            axum::routing::post(routes::totp::confirm),
        )
        .route(
            "/auth/totp/recovery-codes",
            axum::routing::post(routes::totp::regenerate_recovery),
        )
        .route("/timeline/buckets", get(routes::timeline::buckets))
        .route("/timeline/geometry", get(routes::timeline::geometry))
        .route(
            "/timeline/by-ids",
            axum::routing::post(routes::timeline::by_ids),
        )
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
        .route("/map/clusters", get(routes::map::clusters))
        .route("/map/tiles/{region}/{z}/{x}/{y}", get(routes::map::tiles))
        .route(
            "/map/regions",
            get(routes::regions::list).post(routes::regions::download),
        )
        .route(
            "/map/regions/{id}",
            axum::routing::delete(routes::regions::delete),
        )
        .route(
            "/map/regions/{id}/cancel",
            axum::routing::post(routes::regions::cancel),
        )
        .route(
            "/saved-searches",
            get(routes::search::list_saved).post(routes::search::create_saved),
        )
        .route("/ws/ticket", axum::routing::post(routes::ws::ticket))
        .route("/ws", get(routes::ws::connect))
        .route(
            "/operations/{id}/cancel",
            axum::routing::post(routes::operations::cancel),
        )
        .route("/sync/delta", get(routes::sync::delta))
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
        .route("/libraries/{id}/storage", get(routes::libraries::storage))
        .route(
            "/libraries/{id}/probe",
            axum::routing::post(routes::libraries::probe),
        )
        .route(
            "/libraries/{id}/culling-root",
            axum::routing::patch(routes::libraries::set_culling_root),
        )
        .route(
            "/libraries/{id}/culling/lots",
            get(routes::culling::list_lots),
        )
        .route(
            "/culling/lots/{id}/empty-skipped",
            axum::routing::post(routes::culling::empty_skipped),
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
        .route(
            "/users/me/home",
            axum::routing::put(routes::users::set_home).delete(routes::users::delete_home),
        )
        .route(
            "/users/me/preferences",
            get(routes::preferences::get).patch(routes::preferences::patch),
        )
        .route(
            "/users/me/app-passwords",
            axum::routing::post(routes::credentials::create).get(routes::credentials::list),
        )
        .route(
            "/users/me/app-passwords/{id}",
            axum::routing::delete(routes::credentials::revoke),
        )
        .route("/users/me/sessions", get(routes::sessions::list))
        .route(
            "/users/me/sessions/revoke-others",
            axum::routing::post(routes::sessions::revoke_others),
        )
        .route(
            "/users/me/sessions/{id}",
            axum::routing::delete(routes::sessions::revoke),
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
        .route(
            "/assets/{id}",
            get(routes::timeline::asset).delete(routes::trash::delete),
        )
        .route(
            "/assets/{id}/restore",
            axum::routing::post(routes::trash::restore),
        )
        .route(
            "/assets/batch/delete",
            axum::routing::post(routes::trash::batch_delete),
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
            "/metadata/batch/recalculate-timezones/preview",
            axum::routing::post(routes::metadata::preview_timezones),
        )
        .route(
            "/metadata/batch/recalculate-timezones",
            axum::routing::post(routes::metadata::apply_timezones),
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
            "/assets/batch/rename/preview",
            axum::routing::post(routes::rename::preview),
        )
        .route(
            "/assets/batch/rename",
            axum::routing::post(routes::rename::apply_batch),
        )
        .route(
            "/assets/batch/rename/{batch_id}/undo",
            axum::routing::post(routes::rename::undo_batch),
        )
        .route(
            "/assets/batch/move",
            axum::routing::post(routes::asset_move::batch_move),
        )
        .route(
            "/assets/{id}/flags",
            get(routes::flags::get).put(routes::flags::set),
        )
        .route(
            "/assets/{id}/pick",
            axum::routing::post(routes::culling::pick),
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
            "/albums/{id}/refresh",
            axum::routing::post(routes::albums::refresh),
        )
        .route("/tags", get(routes::tags::list).post(routes::tags::create))
        .route("/tags/proposals", get(routes::tags::list_proposals))
        .route(
            "/tags/{id}",
            get(routes::tags::get)
                .patch(routes::tags::patch)
                .delete(routes::tags::delete),
        )
        .route(
            "/tags/{id}/proposals/confirm",
            axum::routing::post(routes::tags::confirm_all_proposals),
        )
        .route(
            "/tags/{id}/proposals/reject",
            axum::routing::post(routes::tags::reject_all_proposals),
        )
        .route(
            "/tags/{id}/assets/{asset_id}/confirm",
            axum::routing::post(routes::tags::confirm_proposal),
        )
        .route(
            "/tags/{id}/assets/{asset_id}/reject",
            axum::routing::post(routes::tags::reject_proposal),
        )
        .route(
            "/tags/{id}/assets/{asset_id}/remove",
            axum::routing::post(routes::tags::remove_confirmed_tag),
        )
        .route(
            "/tags/{id}/assets/batch",
            axum::routing::post(routes::tags::assign_batch),
        )
        .route(
            "/tags/{id}/assets/batch/remove",
            axum::routing::post(routes::tags::unassign_batch),
        )
        .route("/assets/{id}/tags", get(routes::tags::list_tags_for_asset))
        .route("/assets/{id}/albums", get(routes::albums::list_for_asset))
        .route("/assets/{id}/faces", get(routes::faces::list_for_asset))
        .route("/faces/proposals", get(routes::faces::list_proposals))
        .route(
            "/faces/data",
            axum::routing::delete(routes::faces::delete_all_data),
        )
        .route(
            "/faces/{id}/assign",
            axum::routing::post(routes::faces::assign),
        )
        .route(
            "/faces/{id}/reject",
            axum::routing::post(routes::faces::reject),
        )
        .route(
            "/faces/{id}/confirm",
            axum::routing::post(routes::faces::confirm_proposal),
        )
        .route(
            "/persons",
            get(routes::persons::list).post(routes::persons::create),
        )
        .route(
            "/persons/{id}",
            get(routes::persons::get)
                .patch(routes::persons::patch)
                .delete(routes::persons::delete),
        )
        .route(
            "/persons/{id}/merge",
            axum::routing::post(routes::persons::merge),
        )
        .route(
            "/persons/{id}/separate",
            axum::routing::post(routes::persons::separate),
        )
        .route(
            "/persons/{id}/proposals/confirm",
            axum::routing::post(routes::faces::confirm_all_proposals),
        )
        .route(
            "/persons/{id}/proposals/reject",
            axum::routing::post(routes::faces::reject_all_proposals),
        )
        .route(
            "/person-groups",
            get(routes::persons::list_groups).post(routes::persons::create_group),
        )
        .route(
            "/person-groups/{id}",
            axum::routing::patch(routes::persons::patch_group)
                .delete(routes::persons::delete_group),
        )
        .route(
            "/person-groups/{id}/members",
            get(routes::persons::list_group_members),
        )
        .route(
            "/person-groups/{id}/members/{person_id}",
            axum::routing::post(routes::persons::add_group_member)
                .delete(routes::persons::remove_group_member),
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
        .route("/shared-with-me", get(routes::permissions::shared_with_me))
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
        .route(
            "/backup/preferences",
            get(routes::backup::get_preferences).put(routes::backup::put_preferences),
        )
        .route(
            "/backup/destinations",
            get(routes::backup::list_destinations).post(routes::backup::create_destination),
        )
        .route(
            "/backup/destinations/{id}",
            axum::routing::delete(routes::backup::delete_destination),
        )
        .route(
            "/backup/destinations/{id}/test",
            axum::routing::post(routes::backup::test_destination),
        )
        .route("/backup/runs", get(routes::backup::list_runs))
        .route("/backup/run", axum::routing::post(routes::backup::run_now))
        .route(
            "/restore/inspect",
            axum::routing::post(routes::restore::inspect),
        )
        .route("/restore", axum::routing::post(routes::restore::restore))
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
        .route("/upload/check", axum::routing::post(routes::upload::check))
        .route("/upload", axum::routing::post(routes::upload::create))
        .route(
            "/upload/{id}",
            axum::routing::head(routes::upload::head)
                .patch(routes::upload::patch)
                .layer(DefaultBodyLimit::disable()),
        )
        // Server-side half of the CSRF defense: a layer, not a per-handler
        // check, so routes are covered by construction. See `csrf.rs` for
        // the property being bought and the exemptions already planned
        // for (WebDAV, tus).
        .layer(axum::middleware::from_fn_with_state(
            state,
            idempotency::apply,
        ))
        .layer(axum::middleware::from_fn(csrf::require_client_header))
}

fn all_routes(state: AppState) -> Router {
    Router::new()
        .route("/health", get(routes::health::get))
        .route("/api/openapi.json", get(openapi::serve))
        .route("/media/thumb/{hash}", get(routes::media::thumb))
        .route("/media/preview/{hash}", get(routes::media::preview))
        .route("/media/full/{hash}", get(routes::media::full))
        .route("/media/original/{id}", get(routes::media::original))
        .route("/media/video/{id}/playback", get(routes::video::playback))
        .route("/media/video/{id}/poster", get(routes::video::poster))
        .route("/media/video/{id}/hls/{file}", get(routes::video::hls))
        // WebDAV: outside `/api/v1` on purpose — it isn't a REST API and
        // doesn't belong in the frozen contract. Authentication via
        // app-password Basic Auth, never a session cookie (`dav::handler`).
        // `axum::routing::any` also catches the non-standard methods that
        // WebDAV clients use (PROPFIND, MKCOL, MOVE, COPY, LOCK, UNLOCK).
        .route("/dav/{*path}", axum::routing::any(dav::handler))
        .nest("/api/v1", api_routes(state.clone()))
        // Must be called **after** registering the routes: it sets the
        // fallback of every `MethodRouter` already present, and a
        // `route(...)` added afterward would fall back to axum's
        // empty-body `405`. Same class of ordering trap as `.fallback(...)`
        // documented above.
        .method_not_allowed_fallback(method_not_allowed)
        .with_state(state)
}

async fn not_found() -> Problem {
    Problem::not_found()
}

/// `405` inside the RFC 9457 contract: without this fallback axum responds
/// with an empty body and no `type`, and a client branching on the error
/// code has nothing to read.
async fn method_not_allowed() -> Problem {
    Problem::method_not_allowed()
}
