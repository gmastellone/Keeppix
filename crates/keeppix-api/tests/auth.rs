mod harness;

use harness::{TestServer, assert_security_headers, plain_client};
use keeppix_db::UserRepo;
use keeppix_domain::{AuthContext, SystemRole};
use serde_json::json;

/// `keeppix_api::router(state)` — the router *with* state, mounted by
/// `TestServer` and used by every test in this file — applies the same
/// four security headers as the stateless router (`router_without_state`,
/// covered by `tests/health.rs` and `tests/openapi.rs`). The two routers
/// set up the fallback and call `with_common_layers` separately
/// (`crates/keeppix-api/src/lib.rs`): without this test, a bug in the
/// specific ordering of `router(state)` would not fail any test, because
/// no other test in this file looks at the headers — only the status code
/// and the response body.
#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn router_with_state_carries_the_security_headers() {
    let server = TestServer::start().await;

    // An existing route.
    let ok_response = server
        .client
        .get(server.url("/api/v1/setup/status"))
        .send()
        .await
        .unwrap();
    assert_eq!(ok_response.status(), reqwest::StatusCode::OK);
    assert_security_headers(ok_response.headers());

    // The 404 fallback (no API route of this kind exists).
    let not_found_response = server
        .client
        .get(server.url("/api/v1/this-route-does-not-exist"))
        .send()
        .await
        .unwrap();
    assert_eq!(not_found_response.status(), reqwest::StatusCode::NOT_FOUND);
    assert_security_headers(not_found_response.headers());
}

/// No CDN caching on private content: `Cache-Control: private` on
/// everything authenticated. Without the header, `GET /auth/me` — which
/// returns the session's user — is eligible for a shared proxy's
/// heuristic cache. The layer uses `if_not_present`: the counter-proof
/// that it doesn't clobber legitimate policies is
/// `assets_are_served_as_immutable` in `keeppix-server/tests/embed.rs`.
#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn authenticated_responses_are_marked_private() {
    let server = TestServer::start().await;
    setup(&server).await;

    let me = server
        .client
        .get(server.url("/api/v1/auth/me"))
        .send()
        .await
        .unwrap();

    assert_eq!(me.status(), 200);
    assert_eq!(me.headers().get("cache-control").unwrap(), "private");
}

/// The profile view shows the server name and the last password change:
/// `UserView` now carries both on every response that returns the user,
/// not just on `/auth/me`.
#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn me_response_carries_server_name_and_password_changed_at() {
    let server =
        TestServer::start_with(|state| state.with_server_name("Casa Mastellone".to_owned())).await;
    setup(&server).await;

    let me: serde_json::Value = server
        .client
        .get(server.url("/api/v1/auth/me"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    assert_eq!(me["user"]["server_name"], "Casa Mastellone");
    assert!(
        me["user"]["password_changed_at"].is_string(),
        "password_changed_at must be a date, not absent: {me}"
    );
}

/// The three rejections axum produces **before** the handler must stay
/// within the RFC 9457 contract: wrong `Content-Type`, malformed body,
/// wrong method. Without the `keeppix_api::Json` wrapper and the
/// `method_not_allowed_fallback`, these would come out as `text/plain`
/// (or an empty body), leaving a client that branches on `type` with
/// nothing to read.
#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn a_wrong_content_type_is_rejected_as_problem_json() {
    let server = TestServer::start().await;

    // Also the shape a cross-site HTML form would use: without
    // `application/json` the request never reaches the handler.
    let response = server
        .client
        .post(server.url("/api/v1/auth/login"))
        .header(
            reqwest::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
        .body("username=giovanni&password=correct+horse+battery+staple")
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 415);
    assert_eq!(
        response.headers().get("content-type").unwrap(),
        "application/problem+json"
    );
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["type"], "keeppix/unsupported-media-type");
    assert_eq!(body["status"], 415);
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn a_malformed_json_body_is_rejected_as_problem_json() {
    let server = TestServer::start().await;

    let broken = server
        .client
        .post(server.url("/api/v1/auth/login"))
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body("{\"username\": ")
        .send()
        .await
        .unwrap();

    assert_eq!(broken.status(), 400, "invalid JSON syntax");
    assert_eq!(
        broken.headers().get("content-type").unwrap(),
        "application/problem+json"
    );
    let body: serde_json::Value = broken.json().await.unwrap();
    assert_eq!(body["type"], "keeppix/invalid-json");

    // Valid JSON but the wrong shape: axum distinguishes this with `422`,
    // but the `type` stays the same because for the client it's the same
    // problem.
    let wrong_shape = server
        .client
        .post(server.url("/api/v1/auth/login"))
        .json(&json!({ "username": "giovanni" }))
        .send()
        .await
        .unwrap();

    assert_eq!(wrong_shape.status(), 422, "missing password field");
    let body: serde_json::Value = wrong_shape.json().await.unwrap();
    assert_eq!(body["type"], "keeppix/invalid-json");
    assert!(
        body["detail"].as_str().unwrap().contains("password"),
        "the detail must say which field is missing: {body}"
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn a_wrong_method_is_rejected_as_problem_json() {
    let server = TestServer::start().await;

    let response = server
        .client
        .get(server.url("/api/v1/auth/login"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 405);
    assert_eq!(
        response.headers().get("content-type").unwrap(),
        "application/problem+json",
        "axum's default 405 has an empty body"
    );
    // The 405 comes from a `MethodRouter`'s fallback, not the router: if
    // it fell outside `with_common_layers` it would come out without CSP.
    assert_security_headers(response.headers());

    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["type"], "keeppix/method-not-allowed");
}

/// The server-side half of the CSRF defense. The client built here is
/// deliberately *without* `x-keeppix-client`: it's what a `<form>` on a
/// hostile site can do — send the POST with cookies attached but without
/// being able to set custom headers. The body-less mutations — `logout`
/// and `refresh` — are the only ones that didn't even go through
/// `Json<T>`'s `Content-Type` check, so they were entirely uncovered.
#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn a_mutation_without_the_client_header_is_rejected() {
    let server = TestServer::start().await;
    setup(&server).await;
    let forged = reqwest::Client::new();

    for path in ["/api/v1/auth/logout", "/api/v1/auth/refresh"] {
        let response = forged.post(server.url(path)).send().await.unwrap();

        assert_eq!(response.status(), 403, "{path} without custom header");
        assert_eq!(
            response.headers().get("content-type").unwrap(),
            "application/problem+json"
        );
        let body: serde_json::Value = response.json().await.unwrap();
        assert_eq!(body["type"], "keeppix/csrf-check-failed");
    }

    // Reads don't require the header: denying them would break opening a
    // URL directly and would buy nothing.
    let read = forged
        .get(server.url("/api/v1/setup/status"))
        .send()
        .await
        .unwrap();
    assert_eq!(read.status(), 200, "a GET does not change state");
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn a_mutation_with_the_client_header_succeeds() {
    let server = TestServer::start().await;
    setup(&server).await;

    // `server.client` carries the header by default, like the frontend's `apiFetch`.
    let response = server
        .client
        .post(server.url("/api/v1/auth/logout"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 204);
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn a_fresh_instance_reports_not_initialised() {
    let server = TestServer::start().await;
    let body: serde_json::Value = server
        .client
        .get(server.url("/api/v1/setup/status"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    assert_eq!(body["initialised"], false);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn setup_creates_the_first_admin_and_logs_in() {
    let server = TestServer::start().await;

    let response = server
        .client
        .post(server.url("/api/v1/setup"))
        .json(&json!({
            "username": "giovanni",
            "display_name": "Giovanni",
            "password": "correct horse battery staple"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 201);
    let cookie = response
        .headers()
        .get("set-cookie")
        .expect("setup must authenticate immediately")
        .to_str()
        .unwrap()
        .to_owned();
    assert!(cookie.contains("__Host-kpx_session="));
    assert!(cookie.contains("HttpOnly"));
    assert!(cookie.contains("SameSite=Lax"));
    assert!(cookie.contains("Path=/"));

    let me: serde_json::Value = server
        .client
        .get(server.url("/api/v1/auth/me"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(me["user"]["username"], "giovanni");
    assert_eq!(me["user"]["role"], "admin");
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn setup_can_only_run_once() {
    let server = TestServer::start().await;
    let payload = json!({
        "username": "giovanni",
        "display_name": "Giovanni",
        "password": "correct horse battery staple"
    });

    server
        .client
        .post(server.url("/api/v1/setup"))
        .json(&payload)
        .send()
        .await
        .unwrap();

    let second = server
        .client
        .post(server.url("/api/v1/setup"))
        .json(&json!({
            "username": "mario",
            "display_name": "Mario",
            "password": "correct horse battery staple"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(second.status(), 409);
    let body: serde_json::Value = second.json().await.unwrap();
    assert_eq!(body["type"], "keeppix/already-initialised");
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn setup_rejects_a_weak_password() {
    let server = TestServer::start().await;
    let response = server
        .client
        .post(server.url("/api/v1/setup"))
        .json(&json!({ "username": "giovanni", "display_name": "G", "password": "corta" }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 422);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["type"], "keeppix/invalid-password");
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn login_succeeds_with_correct_credentials() {
    let server = TestServer::start().await;
    setup(&server).await;

    let response = server
        .client
        .post(server.url("/api/v1/auth/login"))
        .json(&json!({ "username": "GIOVANNI", "password": "correct horse battery staple" }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200, "username is case-insensitive");
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn login_fails_with_wrong_password() {
    let server = TestServer::start().await;
    setup(&server).await;

    let response = server
        .client
        .post(server.url("/api/v1/auth/login"))
        .json(&json!({ "username": "giovanni", "password": "wrong password" }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 401);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["type"], "keeppix/invalid-credentials");
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn login_fails_identically_for_unknown_user() {
    let server = TestServer::start().await;
    setup(&server).await;

    let response = server
        .client
        .post(server.url("/api/v1/auth/login"))
        .json(&json!({ "username": "nobody", "password": "correct horse battery staple" }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 401);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(
        body["type"], "keeppix/invalid-credentials",
        "a nonexistent user and a wrong password must be indistinguishable"
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn me_requires_authentication() {
    let server = TestServer::start().await;
    setup(&server).await;

    // Fresh client, no cookie.
    let anonymous = plain_client();
    let response = anonymous
        .get(server.url("/api/v1/auth/me"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 401);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["type"], "keeppix/unauthenticated");
}

/// A database blip must not present itself as "expired session": the
/// frontend treats `401` as "no active session", clears the user, and
/// sends them to `/login`. With the previous mapping — any `DbError` →
/// `401` — ten seconds of Postgres restarting was a mass logout invisible
/// as a 5xx. Here the database is actually shut down: there's no mock
/// between the handler and the pool, so this is the only way to observe
/// the property end-to-end.
#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn a_database_outage_is_a_503_not_a_401() {
    let server = TestServer::start_stoppable().await;
    setup(&server).await;

    // Sanity check: the session is valid *before* shutdown, otherwise the
    // expected 503 below could come from a cookie that was never issued.
    let before = server
        .client
        .get(server.url("/api/v1/auth/me"))
        .send()
        .await
        .unwrap();
    assert_eq!(before.status(), 200);

    if !server.stop_database().await {
        eprintln!(
            "KEEPPIX_TEST_DATABASE_URL is set: the Postgres server is shared \
             and cannot be stopped, skipping test"
        );
        return;
    }

    let response = server
        .client
        .get(server.url("/api/v1/auth/me"))
        .send()
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        503,
        "an unreachable database is not an invalid session"
    );
    assert_eq!(
        response.headers().get("retry-after").unwrap(),
        "5",
        "the client must know the error is transient"
    );
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["type"], "keeppix/service-unavailable");
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn refresh_rotates_the_session_cookie() {
    let server = TestServer::start().await;

    let setup_response = server
        .client
        .post(server.url("/api/v1/setup"))
        .json(&json!({
            "username": "giovanni",
            "display_name": "Giovanni",
            "password": "correct horse battery staple"
        }))
        .send()
        .await
        .unwrap();
    let before = session_value_from(&setup_response);

    let warmed = server
        .client
        .get(server.url("/api/v1/auth/me"))
        .send()
        .await
        .unwrap();
    assert_eq!(warmed.status(), 200);

    let refresh = server
        .client
        .post(server.url("/api/v1/auth/refresh"))
        .send()
        .await
        .unwrap();
    assert_eq!(refresh.status(), 204);
    let after = session_value_from(&refresh);

    assert_ne!(before, after, "the cookie must change on every refresh");

    // The new cookie continues to be valid.
    let me = server
        .client
        .get(server.url("/api/v1/auth/me"))
        .send()
        .await
        .unwrap();
    assert_eq!(me.status(), 200);

    // The old cookie, however, must no longer be valid: rotation must have
    // consumed the parent, not just issued a child in parallel. A fresh
    // client with no cookie store, presenting the pre-refresh value
    // explicitly, is the only way to demonstrate this — `server.client`'s
    // cookie store has already replaced `before` with `after`.
    let replay_me = plain_client()
        .get(server.url("/api/v1/auth/me"))
        .header("cookie", format!("__Host-kpx_session={before}"))
        .send()
        .await
        .unwrap();
    assert_eq!(
        replay_me.status(),
        401,
        "the pre-refresh token must have been consumed, not just joined by a new one"
    );
}

/// `authenticate` does not slide `expires_at`. Without a `POST
/// /auth/refresh` (now from the SPA's watchdog, while the tab is visible)
/// the session is absolute: once expired, the next GET is 401.
#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn an_expired_session_is_unauthenticated_without_calling_refresh() {
    let server =
        TestServer::start_with(|s| s.with_session_ttl(std::time::Duration::from_secs(0))).await;
    let setup = server
        .client
        .post(server.url("/api/v1/setup"))
        .json(&json!({
            "username": "giovanni",
            "display_name": "Giovanni",
            "password": "correct horse battery staple"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(setup.status(), 201);
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let me = server
        .client
        .get(server.url("/api/v1/auth/me"))
        .send()
        .await
        .unwrap();
    assert_eq!(me.status(), 401);
    let body: serde_json::Value = me.json().await.unwrap();
    assert_eq!(body["type"], "keeppix/unauthenticated");
}

/// `SessionRepo::rotate` revokes the entire family when an already-
/// consumed token is presented again — the signal that a copy ended up in
/// someone else's hands. `refresh`'s documentation promises this
/// explicitly, but without this test the HTTP coverage of that branch was
/// nil.
#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn refresh_rejects_a_reused_token() {
    let server = TestServer::start().await;

    let setup_response = server
        .client
        .post(server.url("/api/v1/setup"))
        .json(&json!({
            "username": "giovanni",
            "display_name": "Giovanni",
            "password": "correct horse battery staple"
        }))
        .send()
        .await
        .unwrap();
    let before = session_value_from(&setup_response);

    // Consume the token once through the normal flow.
    let first_refresh = server
        .client
        .post(server.url("/api/v1/auth/refresh"))
        .send()
        .await
        .unwrap();
    assert_eq!(first_refresh.status(), 204);

    // Presenting the already-consumed pre-refresh token again must be rejected.
    let reused = plain_client()
        .post(server.url("/api/v1/auth/refresh"))
        .header("cookie", format!("__Host-kpx_session={before}"))
        .send()
        .await
        .unwrap();
    assert_eq!(reused.status(), 401);

    // The 401 above alone doesn't distinguish "consumed token rejected"
    // from "entire family revoked": a `rotate` that merely returned an
    // error would also produce the former. The proof of the revocation
    // branch is that even the *new* token — issued by the rotation and
    // valid until a moment ago — stops working.
    let after = session_value_from(&first_refresh);
    let survivor = plain_client()
        .get(server.url("/api/v1/auth/me"))
        .header("cookie", format!("__Host-kpx_session={after}"))
        .send()
        .await
        .unwrap();
    assert_eq!(
        survivor.status(),
        401,
        "reuse must revoke the entire family, not just the replayed token"
    );
}

/// `rotate` re-checks `disabled_at`. The HTTP disable endpoint also
/// revokes sessions: here only the column is set, otherwise the 401 would
/// be for a revoked token and the join would go unnoticed.
#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn refresh_rejects_a_disabled_user() {
    let server = TestServer::start().await;
    let setup = server
        .client
        .post(server.url("/api/v1/setup"))
        .json(&json!({
            "username": "giovanni",
            "display_name": "Giovanni",
            "password": "correct horse battery staple"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(setup.status(), 201);

    let me = server
        .client
        .get(server.url("/api/v1/auth/me"))
        .send()
        .await
        .unwrap();
    let id = me.json::<serde_json::Value>().await.unwrap()["user"]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    UserRepo::new(&server.db)
        .disable(&AuthContext::user(id, SystemRole::Admin), id)
        .await
        .unwrap();

    let refresh = server
        .client
        .post(server.url("/api/v1/auth/refresh"))
        .send()
        .await
        .unwrap();
    assert_eq!(refresh.status(), 401);
    let body: serde_json::Value = refresh.json().await.unwrap();
    assert_eq!(body["type"], "keeppix/unauthenticated");
}

/// Short TTL: without refresh the session drops; with a refresh while the
/// tab is active it survives past the original expiry.
#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn refresh_slides_expiry_so_an_active_session_survives() {
    let server =
        TestServer::start_with(|s| s.with_session_ttl(std::time::Duration::from_secs(2))).await;
    let setup = server
        .client
        .post(server.url("/api/v1/setup"))
        .json(&json!({
            "username": "giovanni",
            "display_name": "Giovanni",
            "password": "correct horse battery staple"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(setup.status(), 201);

    tokio::time::sleep(std::time::Duration::from_millis(800)).await;
    let refresh = server
        .client
        .post(server.url("/api/v1/auth/refresh"))
        .send()
        .await
        .unwrap();
    assert_eq!(refresh.status(), 204);

    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
    let me = server
        .client
        .get(server.url("/api/v1/auth/me"))
        .send()
        .await
        .unwrap();
    assert_eq!(
        me.status(),
        200,
        "the refresh must have slid expires_at past the original expiry"
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn logout_invalidates_the_session() {
    let server = TestServer::start().await;
    let setup_response = server
        .client
        .post(server.url("/api/v1/setup"))
        .json(&json!({
            "username": "giovanni",
            "display_name": "Giovanni",
            "password": "correct horse battery staple"
        }))
        .send()
        .await
        .unwrap();
    let session_value = session_value_from(&setup_response);

    // Populate `Auth`'s cache before revocation: without this GET, a
    // cache-aside that doesn't invalidate on `revoke` would stay green —
    // the cache would be empty and the 401 would come from the database,
    // not from the drop.
    let warmed = server
        .client
        .get(server.url("/api/v1/auth/me"))
        .send()
        .await
        .unwrap();
    assert_eq!(warmed.status(), 200);

    let response = server
        .client
        .post(server.url("/api/v1/auth/logout"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 204);

    // Fresh client, no cookie store: replays the pre-logout cookie
    // explicitly. If we relied on `server.client`'s cookie store, the next
    // request would start with no cookie at all — the client's local
    // logout, not server-side revocation, would explain the 401.
    let replay_me = plain_client()
        .get(server.url("/api/v1/auth/me"))
        .header("cookie", format!("__Host-kpx_session={session_value}"))
        .send()
        .await
        .unwrap();
    assert_eq!(
        replay_me.status(),
        401,
        "the session must be invalidated server-side, not just forgotten by the client"
    );
}

/// Fake production host. This exists purely as a regression guard: with
/// the fix, `Secure` is unconditional, so whatever value the `Host`
/// header declares — real (`127.0.0.1:<port>`, the harness's actual host)
/// or spoofed like this one — the attribute must still appear. If someone
/// were to reintroduce host-conditional logic in the future, this test
/// would keep passing just as much as the one against the default host
/// and would not catch it: the test that actually proves the fixed
/// property is
/// `login_issues_the_cookie_with_a_valid_host_prefix_on_the_default_test_host`
/// below, against the harness's real host with no spoofing at all.
const PRODUCTION_HOST: &str = "photos.example.com";

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn logout_clears_the_cookie_with_a_valid_host_prefix() {
    let server = TestServer::start().await;
    setup(&server).await;

    let response = server
        .client
        .post(server.url("/api/v1/auth/logout"))
        .header(reqwest::header::HOST, PRODUCTION_HOST)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 204);
    assert_host_prefix_attributes(&response, "Max-Age=0");
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn login_issues_the_cookie_with_a_valid_host_prefix() {
    let server = TestServer::start().await;
    setup(&server).await;

    let response = server
        .client
        .post(server.url("/api/v1/auth/login"))
        .header(reqwest::header::HOST, PRODUCTION_HOST)
        .json(&json!({ "username": "giovanni", "password": "correct horse battery staple" }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    // `Max-Age` is the harness's session TTL (3600 seconds).
    assert_host_prefix_attributes(&response, "Max-Age=3600");
}

/// The test that actually demonstrates the bug this fix corrected: with
/// the default client (no spoofed `Host`, the real header
/// `127.0.0.1:<port>` — the actual host `TestServer` listens on), the
/// session cookie issued by `logout` still carries `Secure`. Before the
/// fix, `should_be_secure` recognized `127.0.0.1` as loopback and omitted
/// `Secure`: that cookie would have been discarded entirely by a real
/// browser, even in the clear on loopback (see the comment on
/// `cookie.rs`).
#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn logout_clears_the_cookie_with_a_valid_host_prefix_on_the_default_test_host() {
    let server = TestServer::start().await;
    setup(&server).await;

    let response = server
        .client
        .post(server.url("/api/v1/auth/logout"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 204);
    assert_host_prefix_attributes(&response, "Max-Age=0");
}

/// As above, but for `login`: proves that the cookie issued against the
/// test's real host (loopback, not spoofed) is still valid under the
/// `__Host-` prefix.
#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn login_issues_the_cookie_with_a_valid_host_prefix_on_the_default_test_host() {
    let server = TestServer::start().await;
    setup(&server).await;

    let response = server
        .client
        .post(server.url("/api/v1/auth/login"))
        .json(&json!({ "username": "giovanni", "password": "correct horse battery staple" }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    assert_host_prefix_attributes(&response, "Max-Age=3600");
}

/// Behavioral test that complements `assert_host_prefix_attributes`
/// (which checks the header literally): after a successful login against
/// the harness's real host (no spoofed `Host`), the **same `reqwest`
/// client** with its automatic cookie jar — not a cookie re-attached by
/// hand — can call `/api/v1/auth/me`. This is exactly the property that
/// was broken: before the fix, `cookie_store` would receive a
/// `Set-Cookie` without `Secure`, and — consistent with the `__Host-`
/// prefix rule that no generic HTTP library implements — would have
/// re-accepted it anyway (`cookie_store` doesn't know about `__Host-`);
/// the bug was only observable in a real browser, never in this
/// round-trip. This test alone therefore would *not* catch a regression
/// in `should_be_secure`: its job is to pin that the normal flow works,
/// not to replace `assert_host_prefix_attributes`.
#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn login_then_me_stays_authenticated_on_the_same_client() {
    let server = TestServer::start().await;
    setup(&server).await;

    let login = server
        .client
        .post(server.url("/api/v1/auth/login"))
        .json(&json!({ "username": "giovanni", "password": "correct horse battery staple" }))
        .send()
        .await
        .unwrap();
    assert_eq!(login.status(), 200);

    let me = server
        .client
        .get(server.url("/api/v1/auth/me"))
        .send()
        .await
        .unwrap();
    assert_eq!(
        me.status(),
        200,
        "the client with cookie-jar must remain authenticated"
    );
}

/// Checks every attribute on the raw `set-cookie` header that makes a
/// `__Host-`-prefixed cookie acceptable: a compliant browser discards
/// **entirely** a `__Host-` cookie missing `Secure` or `Path=/` (RFC
/// 6265bis §4.1.3.2). On the clearing cookie, the effect is that logout
/// clears nothing and the session survives in the browser; on the session
/// cookie, that it also travels in the clear. Neither of these is visible
/// in tests — the harness speaks HTTP on 127.0.0.1 — so it's pinned here
/// instead.
///
/// The response is read directly rather than `reqwest`'s cookie store,
/// for a reason different from what one might expect: it is **not** that
/// `reqwest` discards a `Secure` cookie received in the clear on
/// loopback — verified empirically, it does not: `cookie_store` (the
/// library `reqwest` uses for its jar) applies the same "potentially
/// trustworthy origin" exception for loopback that real browsers do. The
/// reason is that the cookie store does not implement `__Host-` prefix
/// validation at all (it's a browser-specific extension, not part of core
/// RFC 6265): reading the jar could never detect a missing `Secure`,
/// `Path=/`, or `Domain`, no matter what the server does. Only inspecting
/// the literal header (or a real browser engine, as in a manual
/// Playwright check) proves this property.
#[allow(clippy::unwrap_used, clippy::expect_used)]
fn assert_host_prefix_attributes(response: &reqwest::Response, expected_max_age: &str) {
    let set_cookie = response
        .headers()
        .get("set-cookie")
        .expect("set-cookie present")
        .to_str()
        .unwrap()
        .to_owned();

    // Attributes are compared as whole tokens, not with `contains` on the
    // header: the token value is random and could contain any of these
    // strings.
    let mut parts = set_cookie.split(';').map(str::trim);
    let name_value = parts.next().expect("name=value pair");
    let attributes: Vec<&str> = parts.collect();

    assert!(
        name_value.starts_with("__Host-kpx_session="),
        "unexpected cookie: {set_cookie}"
    );
    for expected in [
        "Secure",
        "SameSite=Lax",
        "Path=/",
        "HttpOnly",
        expected_max_age,
    ] {
        assert!(
            attributes.contains(&expected),
            "missing `{expected}` in `{set_cookie}`"
        );
    }
}

#[allow(clippy::unwrap_used)]
async fn setup(server: &TestServer) {
    server
        .client
        .post(server.url("/api/v1/setup"))
        .json(&json!({
            "username": "giovanni",
            "display_name": "Giovanni",
            "password": "correct horse battery staple"
        }))
        .send()
        .await
        .unwrap();
}

/// Extracts the session cookie's value from a response's `set-cookie`
/// header. `reqwest`'s cookie store isn't inspectable, so this reads what
/// the server emitted directly.
#[allow(clippy::unwrap_used, clippy::expect_used)]
fn session_value_from(response: &reqwest::Response) -> String {
    response
        .headers()
        .get("set-cookie")
        .expect("set-cookie present")
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .trim_start_matches("__Host-kpx_session=")
        .to_owned()
}
