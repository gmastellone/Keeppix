//! Shared test assertions across the workspace crates.
//!
//! It exists for a mechanical reason: two test binaries in **different
//! crates** cannot share code except through a crate. The assertions on
//! security headers were needed by `keeppix-api` (routes, 404, 405,
//! `OpenAPI` document) and by `keeppix-server` (SPA fallback), and there
//! used to be **three** textual copies: three places to update whenever the
//! policy changes, with the statistical certainty that one would be left
//! behind.
//!
//! The header type is `http::HeaderMap`, the same type that `axum::http`
//! and `reqwest::header` re-export: the same function serves both the tests
//! that speak HTTP via `reqwest` and those that call a `Router` with
//! `oneshot`.

// Test code asserts by failing: `unwrap`/`expect` here are the tool, not an
// oversight.
#![allow(clippy::expect_used)]

use http::HeaderMap;

/// Expected policy in `Content-Security-Policy`, directive by directive. The
/// comparison is per exact directive, not a `contains` on the whole header:
/// `default-src 'self' *` contains `default-src 'self'` and must not pass.
const REQUIRED_CSP_DIRECTIVES: [&str; 5] = [
    // Foundation: everything not covered by a specific directive comes from
    // the origin itself.
    "default-src 'self'",
    // The half that matters: without inline scripts, `'self'` is stronger
    // than a nonce.
    "script-src 'self'",
    // Anti-clickjacking. Replaces `X-Frame-Options`.
    "frame-ancestors 'none'",
    // Prevents an injection from rewriting the base for relative paths.
    "base-uri 'none'",
    // An injected form cannot submit credentials to another host.
    "form-action 'self'",
];

/// Security headers expected on **every** response, whatever route produces
/// it: an existing route, the 404 fallback, the `405` fallback, the SPA
/// fallback, or the `OpenAPI` document. They are applied by
/// `keeppix_api::with_common_layers`, and the trap this assertion guards
/// against is the order of `.fallback(...)` relative to `.layer(...)` (see
/// the comment on that function): getting it wrong makes `index.html`
/// itself come out without CSP.
///
/// # Panics
/// If a header is missing or has a value different from the one expected.
pub fn assert_security_headers(headers: &HeaderMap) {
    assert_eq!(
        headers
            .get("x-content-type-options")
            .expect("x-content-type-options"),
        "nosniff"
    );
    assert_eq!(
        headers.get("referrer-policy").expect("referrer-policy"),
        "no-referrer"
    );
    assert_eq!(
        headers
            .get("permissions-policy")
            .expect("permissions-policy"),
        "camera=(), microphone=(), geolocation=()"
    );
    // HSTS is among the mandatory headers. A browser ignores it when it
    // arrives over HTTP, so the unconditional header doesn't break
    // plain-HTTP use on a LAN and is honored wherever there's a TLS proxy in
    // front.
    assert_eq!(
        headers
            .get("strict-transport-security")
            .expect("strict-transport-security"),
        "max-age=31536000; includeSubDomains"
    );
    assert_content_security_policy(headers);
}

/// Verifies the **substance** of the CSP, not just its presence. The three
/// copies this function replaces did `assert!(csp.is_some())`: replacing the
/// entire policy with `default-src *` still left the suite green, i.e. the
/// assertion asserted nothing. Here a weakened policy makes the tests fail.
///
/// # Panics
/// If the header is missing, if a required directive isn't present exactly
/// as written, or if an `unsafe-*` exception reappears.
pub fn assert_content_security_policy(headers: &HeaderMap) {
    let csp = headers
        .get("content-security-policy")
        .expect("content-security-policy")
        .to_str()
        .expect("CSP is ASCII");

    let directives: Vec<&str> = csp.split(';').map(str::trim).collect();
    for required in REQUIRED_CSP_DIRECTIVES {
        assert!(
            directives.contains(&required),
            "CSP does not contain the directive `{required}`: `{csp}`"
        );
    }

    // No `unsafe-*` exception in **any** directive. On `script-src` this is
    // the property that makes the policy effective: without it, a
    // `<script>` injection won't execute. On `style-src` the exception used
    // to be there, justified by a misleading comment, and served no
    // purpose: the Vite bundle has no inline styles (verified against
    // `dist/index.html`) and whatever Vue sets at runtime goes through the
    // CSSOM, which CSP does not intercept.
    assert!(
        !csp.contains("unsafe-inline") && !csp.contains("unsafe-eval"),
        "CSP must not allow unsafe exceptions: `{csp}`"
    );
    assert!(
        directives.iter().any(|d| d.starts_with("style-src")),
        "style-src must be declared: without it, `default-src` applies and \
         removing `unsafe-inline` would not be observable: `{csp}`"
    );
}
