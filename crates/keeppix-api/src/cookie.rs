use std::time::Duration;

use axum_extra::extract::cookie::{Cookie, SameSite};
use keeppix_domain::{SessionToken, ShareToken};

use crate::extract::{SESSION_COOKIE, SHARE_LINK_COOKIE, SHARE_UNLOCK_COOKIE};

/// Session cookie with the `__Host-` prefix.
///
/// The `__Host-` prefix (RFC 6265bis §4.1.3.2) mandates `Secure`, `Path=/`,
/// and forbids `Domain`: a compliant client that receives a `Set-Cookie`
/// with that prefix but is missing even one of these requirements discards
/// the cookie **entirely**, not just the missing attribute. This always
/// applies, regardless of the request's transport: `Secure` must be set
/// even when talking in plaintext over `127.0.0.1` in development, because
/// it's the *literal presence* of the attribute in the header that's
/// required, not actual TLS usage.
///
/// Not to be confused with a different, more permissive rule that browsers
/// apply *downstream*: a "potentially trustworthy" origin like
/// `127.0.0.1`/`localhost`/`::1` is exempted from the requirement that
/// `Secure` **only works** over an encrypted connection, so a browser (and,
/// verified empirically, also `cookie_store`/`reqwest` at the version used
/// by this workspace) accepts and resends a `Secure` cookie received in
/// plaintext over loopback. But this never makes it optional to *set* the
/// attribute server-side: if it's missing, the `__Host-` prefix causes the
/// cookie to be discarded regardless of where the request is headed. A
/// previous implementation omitted `Secure` on hosts that looked like
/// loopback, thinking it was needed to make the cookie observable in
/// tests: that was doubly wrong, both because it broke every session
/// opened in local development via a real browser, and because the test
/// library already handled that case correctly.
#[must_use]
pub fn session_cookie(token: &SessionToken, ttl: Duration) -> Cookie<'static> {
    let mut cookie = Cookie::new(SESSION_COOKIE, token.as_str().to_owned());
    cookie.set_http_only(true);
    cookie.set_secure(true);
    cookie.set_same_site(SameSite::Lax);
    cookie.set_path("/");
    cookie.set_max_age(Some(
        time::Duration::try_from(ttl).unwrap_or(time::Duration::days(30)),
    ));
    cookie
}

/// Same shape as `session_cookie`: a clearing `__Host-` cookie that omitted
/// `Secure` or `SameSite` would be discarded entirely by a compliant
/// browser (RFC 6265bis §4.1.3.2), leaving the expired — but still sent by
/// the client — session cookie surviving logout in production.
#[must_use]
pub fn clearing_cookie() -> Cookie<'static> {
    let mut cookie = Cookie::new(SESSION_COOKIE, "");
    cookie.set_http_only(true);
    cookie.set_secure(true);
    cookie.set_same_site(SameSite::Lax);
    cookie.set_path("/");
    cookie.set_max_age(Some(time::Duration::ZERO));
    cookie
}

fn host_cookie(name: &'static str, value: String, ttl: Duration) -> Cookie<'static> {
    let mut cookie = Cookie::new(name, value);
    cookie.set_http_only(true);
    cookie.set_secure(true);
    cookie.set_same_site(SameSite::Lax);
    cookie.set_path("/");
    cookie.set_max_age(Some(
        time::Duration::try_from(ttl).unwrap_or(time::Duration::hours(1)),
    ));
    cookie
}

/// Unlock proof for a password-protected share link. Same `__Host-` rules as
/// the session cookie. TTL is short: losing it only means re-entering the
/// password, not losing a login.
#[must_use]
pub fn share_unlock_cookie(token: &ShareToken, ttl: Duration) -> Cookie<'static> {
    host_cookie(SHARE_UNLOCK_COOKIE, token.as_str().to_owned(), ttl)
}

/// Lets the public page's `<img>` tags authenticate without a custom header.
#[must_use]
pub fn share_link_cookie(token: &ShareToken, ttl: Duration) -> Cookie<'static> {
    host_cookie(SHARE_LINK_COOKIE, token.as_str().to_owned(), ttl)
}
