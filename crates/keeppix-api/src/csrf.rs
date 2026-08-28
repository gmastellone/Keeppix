//! Server-side half of the CSRF defense.
//!
//! Three things are required together: `SameSite=Lax` on the cookie, a
//! mandatory `Content-Type: application/json`, and a custom header on
//! mutations. The first two already existed (`cookie.rs` and the
//! `crate::json::Json` extractor, which rejects a non-JSON body with
//! `415`); this module adds the third, which is also the only one that
//! covers mutations **with no body** — `POST /auth/refresh` and
//! `POST /auth/logout` don't go through `Json<T>`.
//!
//! The property being bought here is precise: an HTML `<form>` on a hostile
//! site can send a cross-site POST with cookies attached (which is why
//! `SameSite=Lax` alone isn't enough against a *same-site* attacker:
//! `evil.example.com` and `photos.example.com` are the same site as far as
//! `SameSite` is concerned, and the `__Host-` prefix prevents *setting* the
//! cookie for the domain, not *sending* it), but it **cannot set a custom
//! header** — that would require `fetch`/`XHR`, i.e. a CORS preflight,
//! which this instance doesn't grant.
//!
//! The check is a layer, not a per-handler check, so routes added later
//! inside `api_routes()` are covered without anyone having to remember.
//! Exemptions to plan for as they arrive: `WebDAV` (its clients are Finder
//! and rclone, which never send Keeppix headers) and **tus** uploads
//! (`application/offset+octet-stream`); both live outside `/api/v1` and
//! therefore outside this layer, but the decision needs to be made
//! explicitly, not by oversight.

use axum::extract::Request;
use axum::http::Method;
use axum::middleware::Next;
use axum::response::{IntoResponse as _, Response};

use crate::problem::Problem;

/// Header every Keeppix client sends on mutations. The frontend sets it in
/// `apiFetch` (`frontend/src/api/client.ts`) on **all** calls; the value
/// doesn't matter, what matters is that a cross-site form can't produce it.
pub const CLIENT_HEADER: &str = "x-keeppix-client";

/// Rejects a mutation missing `x-keeppix-client` with
/// `403 keeppix/csrf-check-failed`. Safe methods (`GET`, `HEAD`, `OPTIONS`)
/// always pass: they don't change state, and requiring the header on them
/// would break directly opening a URL.
pub async fn require_client_header(req: Request, next: Next) -> Response {
    // Exemption for `/dav/*`: WebDAV clients (Finder, rclone, …) never send
    // `x-keeppix-client`, and there's no session cookie to protect there —
    // authentication is Basic Auth with an app-password. The exemption is
    // by path prefix, not by absence of a cookie: it doesn't narrow
    // `/api/v1`, which stays fully covered.
    if req.uri().path().starts_with("/dav/") {
        return next.run(req).await;
    }

    let mutating = matches!(
        *req.method(),
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    );

    if mutating && !req.headers().contains_key(CLIENT_HEADER) {
        return Problem::csrf_check_failed()
            .with_detail(format!(
                "{CLIENT_HEADER} is required on state-changing requests"
            ))
            .into_response();
    }

    next.run(req).await
}
