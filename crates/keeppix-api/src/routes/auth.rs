use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum_extra::extract::CookieJar;
use chrono::{DateTime, Utc};
use keeppix_db::{SessionRepo, TotpRepo, UserRepo};
use keeppix_domain::{Password, SessionToken, SystemRole, User, Username, verify_password};
use serde::{Deserialize, Serialize};

use crate::cookie::{clearing_cookie, session_cookie};
use crate::extract::{Auth, SESSION_COOKIE};
use crate::json::Json;
use crate::problem::Problem;
use crate::routes::setup::user_agent;
use crate::state::AppState;

/// Public representation of a user. Contains neither the password hash nor
/// the TOTP secret: those fields never leave the database.
#[derive(Serialize, utoipa::ToSchema)]
pub struct UserView {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub email: Option<String>,
    // The field stays `&'static str` (it's a constant chosen by the server,
    // not allocated data): the schema only needs to know it's a string on
    // the wire.
    #[schema(value_type = String)]
    pub role: &'static str,
    pub locale: Option<String>,
    pub disabled_at: Option<DateTime<Utc>>,
    /// Server name (cosmetic — `AppState::server_name`).
    pub server_name: String,
    /// When the password was last set ("Last changed"). Matches account
    /// creation until it is changed.
    pub password_changed_at: DateTime<Utc>,
}

impl UserView {
    #[must_use]
    pub fn new(u: &User, server_name: &str) -> Self {
        Self {
            id: u.id.to_string(),
            username: u.username.as_str().to_owned(),
            display_name: u.display_name.clone(),
            email: u.email.clone(),
            role: match u.role {
                SystemRole::Admin => "admin",
                SystemRole::User => "user",
            },
            locale: u.locale.clone(),
            disabled_at: u.disabled_at,
            server_name: server_name.to_owned(),
            password_changed_at: u.password_changed_at,
        }
    }
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct LoginRequest {
    username: String,
    password: String,
    /// Optional TOTP or recovery code. Required once the account has 2FA enabled.
    #[serde(default)]
    totp_code: Option<String>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct LoginResponse {
    user: UserView,
}

/// # Errors
/// `401 invalid-credentials` for a nonexistent user, wrong password, or a
/// disabled account: the three situations are indistinguishable from the
/// outside.
#[utoipa::path(
    post,
    path = "/api/v1/auth/login",
    tag = "auth",
    operation_id = "auth_login",
    summary = "Open a session with username and password",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Session opened", body = LoginResponse),
        (status = 400, description = "JSON body is not syntactically valid", body = Problem),
        (status = 401, description = "Invalid credentials", body = Problem),
        (status = 415, description = "Content-Type other than application/json", body = Problem),
        (status = 422, description = "Valid JSON body but of unexpected shape", body = Problem),
        (status = 500, description = "Database error or failure creating the session", body = Problem)
    )
)]
pub async fn login(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Json(req): Json<LoginRequest>,
) -> Result<impl IntoResponse, Problem> {
    let rate_key = client_ip(&headers);
    if !state.login_limiter.check_and_record(&rate_key) {
        return Err(Problem::too_many_requests());
    }

    let invalid = || {
        Problem::new(
            StatusCode::UNAUTHORIZED,
            "invalid-credentials",
            "Invalid credentials",
        )
    };

    let username = Username::parse(&req.username).map_err(|_| invalid())?;
    // Move the serde allocation into `Password` so Drop zeroizes the only
    // heap copy we control (the HTTP body `Bytes` remain outside that
    // control and are not zeroized).
    let password = Password::parse_owned(req.password).map_err(|_| invalid())?;

    let found = UserRepo::new(&state.db).find_by_username(&username).await?;
    let Some((user, hash)) = found else {
        // Dummy verification so the response time doesn't leak whether the
        // user exists: the hash below is a valid Argon2id, so
        // `verify_password` runs the full computation before failing.
        let _ = verify_password(&password, &dummy_hash());
        return Err(invalid());
    };

    if !verify_password(&password, &hash) || !user.is_active() {
        return Err(invalid());
    }

    let totp = TotpRepo::new(&state.db);
    if totp.is_enabled_for_user(user.id).await? {
        let Some(code) = req
            .totp_code
            .as_deref()
            .map(str::trim)
            .filter(|c| !c.is_empty())
        else {
            return Err(Problem::new(
                StatusCode::UNAUTHORIZED,
                "totp-required",
                "Two-factor authentication code required",
            ));
        };
        if !totp.verify_login(user.id, code).await? {
            return Err(invalid());
        }
    }

    let token = SessionRepo::new(&state.db)
        .create(user.id, state.session_ttl, user_agent(&headers))
        .await?;

    let jar = jar.add(session_cookie(&token, state.session_ttl));

    Ok((
        StatusCode::OK,
        jar,
        Json(LoginResponse {
            user: UserView::new(&user, &state.server_name),
        }),
    ))
}

/// Constant hash used only to equalize response times when the username
/// does not exist. Must be a **valid** Argon2id — otherwise
/// `verify_password` fails at the parsing stage without ever running
/// Argon2, and the timing difference this function is supposed to mask
/// remains entirely visible. Generated once with `hash_password` on an
/// arbitrary password never used for a real login; see the
/// `dummy_hash_is_a_valid_argon2id_phc_string` test below.
fn dummy_hash() -> keeppix_domain::PasswordHash {
    keeppix_domain::PasswordHash::from_stored(
        "$argon2id$v=19$m=19456,t=2,p=1$BKjMC3FKz54nTDnFf9fLRQ$\
         Lckl7W7KbvukoSApSxfeAzdhbmnPBAyeHtIIl9Dhmhs"
            .to_owned(),
    )
}

/// # Errors
/// `401 unauthenticated` if the cookie is missing, expired, or was reused
/// after being consumed — in the latter case the entire family has already
/// been revoked; `503 service-unavailable` if the database does not
/// respond.
// The causes of a `401` remain indistinguishable on purpose: the client
// must not be able to tell a rejected rotation apart from an already-consumed
// token. That argument only holds between `NotFound` and `Forbidden`,
// though — not for an unreachable database, which reveals nothing about the
// token's state and which, if mapped to 401, would log everyone out on
// every Postgres restart. The distinction lives in one place,
// `crate::extract::session_problem`.
#[utoipa::path(
    post,
    path = "/api/v1/auth/refresh",
    tag = "auth",
    operation_id = "auth_refresh",
    summary = "Rotate the current session cookie",
    security(("session_cookie" = [])),
    responses(
        (status = 204, description = "Session rotated, new cookie issued"),
        (status = 401, description = "Cookie missing, expired, or already consumed", body = Problem),
        (status = 500, description = "Session row unreadable", body = Problem),
        (status = 503, description = "Database unreachable: retry, the session is still valid", body = Problem)
    )
)]
pub async fn refresh(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<impl IntoResponse, Problem> {
    let cookie = jar
        .get(SESSION_COOKIE)
        .ok_or_else(Problem::unauthenticated)?;
    let token = SessionToken::from_string(cookie.value().to_owned());
    state.sessions.drop_token(&token);

    let next = SessionRepo::new(&state.db)
        .rotate(&token, state.session_ttl)
        .await
        .map_err(crate::extract::session_problem)?;

    let jar = jar.add(session_cookie(&next, state.session_ttl));

    Ok((StatusCode::NO_CONTENT, jar))
}

/// Always `204`, even without a cookie: logging out must work regardless.
// No `security(...)`: the route is deliberately usable even without a
// cookie, and the revocation error is logged, not returned — 204 is the
// only possible outcome.
#[utoipa::path(
    post,
    path = "/api/v1/auth/logout",
    tag = "auth",
    operation_id = "auth_logout",
    summary = "Revoke the current session and clear the cookie",
    responses((status = 204, description = "Session closed and cookie cleared"))
)]
pub async fn logout(State(state): State<AppState>, jar: CookieJar) -> impl IntoResponse {
    if let Some(cookie) = jar.get(SESSION_COOKIE) {
        let token = SessionToken::from_string(cookie.value().to_owned());
        state.sessions.drop_token(&token);
        if let Err(e) = SessionRepo::new(&state.db).revoke(&token).await {
            tracing::warn!(error = %e, "session revocation failed");
        }
    }
    (StatusCode::NO_CONTENT, jar.add(clearing_cookie()))
}

fn client_ip(headers: &HeaderMap) -> String {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map_or_else(|| "unknown".to_owned(), str::to_owned)
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct MeResponse {
    user: UserView,
}

/// # Errors
/// `401` if not authenticated, `404` if the user was removed in the
/// meantime.
#[utoipa::path(
    get,
    path = "/api/v1/auth/me",
    tag = "auth",
    operation_id = "auth_me",
    summary = "Return the current authenticated user",
    security(("session_cookie" = [])),
    responses(
        (status = 200, description = "User of the current session", body = MeResponse),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 404, description = "User removed while the session was open", body = Problem),
        (status = 500, description = "Database error", body = Problem),
        (status = 503, description = "Database unreachable while verifying the session", body = Problem)
    )
)]
pub async fn me(
    State(state): State<AppState>,
    Auth(ctx): Auth,
) -> Result<Json<MeResponse>, Problem> {
    let id = ctx.user_id().ok_or_else(Problem::unauthenticated)?;
    let user = UserRepo::new(&state.db).find_by_id(&ctx, id).await?;
    Ok(Json(MeResponse {
        user: UserView::new(&user, &state.server_name),
    }))
}

#[cfg(test)]
mod tests {
    use super::dummy_hash;
    use keeppix_domain::{Password, verify_password};

    /// Plaintext used once to generate the `dummy_hash()` constant.
    /// Publishing it here is harmless: no account uses this password, and
    /// the defense needs comparable verification *time*, not secrecy.
    const DUMMY_HASH_PLAINTEXT: &str = "this password is never used to log in";

    /// Pins the bug this function fixes: the original `dummy_hash()` was a
    /// malformed PHC string, so `verify_password` failed at the *parsing*
    /// stage and never ran Argon2 — the timing difference between
    /// "nonexistent user" and "wrong password" stayed fully visible.
    ///
    /// `starts_with("$argon2id$")`, `contains("m=19456,t=2,p=1")`, and
    /// `!verify_password(other_password, ..)` alone are not enough to pin
    /// this down: a hash corrupted right in the last segment — i.e. the
    /// exact same bug — would still pass all three, because a parsing
    /// failure returns `false` indistinguishably from a real mismatch. Only
    /// a **positive** match against the plaintext that generated the hash
    /// proves that parsing succeeded and that Argon2 ran to completion.
    #[test]
    #[allow(clippy::unwrap_used)]
    fn dummy_hash_is_a_valid_argon2id_phc_string() {
        let hash = dummy_hash();
        assert!(hash.as_str().starts_with("$argon2id$"));
        assert!(hash.as_str().contains("m=19456,t=2,p=1"));

        let matching = Password::parse(DUMMY_HASH_PLAINTEXT).unwrap();
        assert!(
            verify_password(&matching, &hash),
            "hash parsing must succeed and Argon2 must run to completion"
        );

        // No real login password should verify against the dummy hash: its
        // plaintext does not correspond to any account.
        let attempted = Password::parse("correct horse battery staple").unwrap();
        assert!(!verify_password(&attempted, &hash));
    }
}
