//! Frontend embedded in the binary: `frontend/dist` is compiled into the
//! executable at build time (`rust-embed`), so the Docker image ships a
//! single artifact, without a separate Vite container.

use axum::body::Body;
use axum::http::{HeaderValue, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use keeppix_api::Problem;
use rust_embed::Embed;

#[derive(Embed)]
#[folder = "$CARGO_MANIFEST_DIR/../../frontend/dist"]
struct Assets;

/// Serves an embedded file, or `index.html` as SPA fallback: page routing
/// happens client-side (`vue-router`), so any path not recognized as an
/// asset must still receive the document that boots the application.
///
/// API paths never reach here in the real binary: they are registered
/// earlier in the router (see `mount`), so they never fall through to the
/// fallback. The check on `api/` below is still defense in depth, not the
/// only safeguard: if the route order ever changed, an API client must
/// still not receive HTML instead of a JSON 404.
async fn serve(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');

    if path.starts_with("api/") || path.starts_with("media/") || path.starts_with("dav/") {
        return Problem::not_found().into_response();
    }

    if let Some(file) = Assets::get(path) {
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        // Bundle filenames under `assets/` contain a content hash: they are
        // immutable, and can be cached forever.
        let cache = if path.starts_with("assets/") {
            "public, max-age=31536000, immutable"
        } else {
            "no-cache"
        };

        return Response::builder()
            .status(StatusCode::OK)
            .header(
                header::CONTENT_TYPE,
                HeaderValue::from_str(mime.as_ref())
                    .unwrap_or(HeaderValue::from_static("application/octet-stream")),
            )
            .header(header::CACHE_CONTROL, HeaderValue::from_static(cache))
            .body(Body::from(file.data.into_owned()))
            .unwrap_or_else(|_| Problem::internal().into_response());
    }

    match Assets::get("index.html") {
        Some(index) => Response::builder()
            .status(StatusCode::OK)
            .header(
                header::CONTENT_TYPE,
                HeaderValue::from_static("text/html; charset=utf-8"),
            )
            .header(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"))
            .body(Body::from(index.data.into_owned()))
            .unwrap_or_else(|_| Problem::internal().into_response()),
        None => Problem::not_found().into_response(),
    }
}

/// Adds the SPA fallback to a router and applies the common layers to it
/// (security headers, compression, tracing). Generic over state `S` so that
/// `mount_stateless()` can be `mount(Router::new())`: a single
/// implementation of the invariant below, exercised both by the real binary
/// (`S = AppState`, via `main.rs`) and by database-less tests (`S = ()`),
/// instead of two separate function bodies that a refactor could let drift
/// apart without the tests noticing.
///
/// Order matters: the fallback must be set **before**
/// `keeppix_api::with_common_layers`, not after. In axum 0.8, `.layer()`
/// only wraps the fallback already present at the time it is called — a
/// fallback added afterward would come out without CSP, which here would
/// mean serving `index.html`, the document that loads the entire
/// application, without `Content-Security-Policy` in production. See the
/// comment on `with_common_layers` in `keeppix-api` for details. Do not
/// reorder.
pub fn mount<S: Clone + Send + Sync + 'static>(router: axum::Router<S>) -> axum::Router<S> {
    keeppix_api::with_common_layers(router.fallback(get(serve)))
}

/// Stateless router, with the same SPA fallback and the same common layers
/// as `mount`, for tests that don't touch the database. It is literally
/// `mount` applied to an empty router, not a second implementation: tests
/// that call this function exercise the same code that `main.rs` runs in
/// production.
pub fn mount_stateless() -> axum::Router {
    mount(axum::Router::new())
}
