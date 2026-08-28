use axum::extract::FromRequestParts;
use axum::http::HeaderMap;
use axum::http::request::Parts;
use axum_extra::extract::CookieJar;
use keeppix_db::{DbError, SessionRepo, ShareLinkRepo};
use keeppix_domain::{AuthContext, SessionToken, ShareToken};

use crate::problem::Problem;
use crate::state::AppState;

pub const SESSION_COOKIE: &str = "__Host-kpx_session";
/// Opaque proof that the guest unlocked a password-protected share link.
pub const SHARE_UNLOCK_COOKIE: &str = "__Host-kpx_share";
/// Share-link token for same-origin media requests that cannot set headers.
pub const SHARE_LINK_COOKIE: &str = "__Host-kpx_share_link";

/// Translates a session-verification error into a `Problem`. This is the
/// only place where this decision is made: both the `Auth` extractor and
/// the `refresh` handler use it, the two places where a token is looked up.
///
/// The distinction that matters is between "this session isn't valid" and
/// "the database isn't responding". Mapping everything to `401` meant that
/// ten seconds of Postgres restarting would show up to every client as
/// "session expired": the frontend clears the user and the router guard
/// sends them to `/login`, i.e. a mass logout that doesn't show up as a 5xx
/// in any metric. A `503` with `Retry-After` tells the client the truth —
/// retry, don't log in again.
pub(crate) fn session_problem(err: DbError) -> Problem {
    match err {
        DbError::Connection(e) => {
            tracing::error!(error = %e, "session lookup failed: database unavailable");
            Problem::service_unavailable()
        }
        // Unknown, expired, revoked, or consumed token, disabled user, reuse
        // detected: the session isn't valid, and the client must log in
        // again. The causes are deliberately not distinguished.
        DbError::NotFound | DbError::Forbidden => Problem::unauthenticated(),
        // Unreadable row (e.g. a `role` outside the CHECK constraint) or a
        // failed migration: not an invalid session, a server defect. This
        // doesn't grant access, but it shouldn't be passed off as expiry
        // either.
        other => {
            tracing::error!(error = %other, "session lookup failed");
            Problem::internal()
        }
    }
}

/// Extracts the authentication context from the session cookie. Every
/// handler that touches a user's data **must** take this extractor: it's
/// how `AuthContext` reaches the repositories.
pub struct Auth(pub AuthContext);

impl FromRequestParts<AppState> for Auth {
    type Rejection = Problem;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let jar = CookieJar::from_headers(&parts.headers);
        let cookie = jar
            .get(SESSION_COOKIE)
            .ok_or_else(Problem::unauthenticated)?;
        let token = SessionToken::from_string(cookie.value().to_owned());

        if let Some(ctx) = state.sessions.get(&token) {
            if let Some(hook) = &state.on_authenticated {
                hook();
            }
            return Ok(Self(ctx));
        }

        let ctx = SessionRepo::new(&state.db)
            .authenticate(&token)
            .await
            .map_err(session_problem)?;
        state.sessions.put(&token, ctx.clone());

        if let Some(hook) = &state.on_authenticated {
            hook();
        }

        Ok(Self(ctx))
    }
}

const SHARE_TOKEN_HEADER: &str = "x-share-token";

/// Share links are confined to explicit share routes and media; timeline and
/// search must not widen the perimeter.
///
/// # Errors
/// `403` when a share token is present on a session-only route.
pub fn reject_public_share_token(headers: &HeaderMap) -> Result<(), Problem> {
    if headers.contains_key(SHARE_TOKEN_HEADER) {
        return Err(Problem::forbidden());
    }
    Ok(())
}

/// Extractor for share-link authentication. Reads the token from the
/// `X-Share-Token` header or from the path parameter `:token`. Produces an
/// `AuthContext::ShareLink` if valid; otherwise rejects with 403.
pub struct ShareAuth(pub AuthContext);

impl FromRequestParts<AppState> for ShareAuth {
    type Rejection = Problem;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let token_str = parts
            .headers
            .get(SHARE_TOKEN_HEADER)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned)
            .or_else(|| {
                parts
                    .uri
                    .path()
                    .strip_prefix("/api/v1/share/")
                    .and_then(|rest| rest.split('/').next())
                    .map(str::to_owned)
            })
            .or_else(|| {
                CookieJar::from_headers(&parts.headers)
                    .get(SHARE_LINK_COOKIE)
                    .map(|c| c.value().to_owned())
            })
            .ok_or_else(Problem::forbidden)?;

        if !state.share_limiter.check_and_record(&token_str) {
            return Err(Problem::too_many_requests());
        }

        let share_token = ShareToken::from_string(token_str);
        let hash = share_token.digest();

        let row = ShareLinkRepo::new(&state.db)
            .lookup_by_token_hash(&hash)
            .await
            .map_err(|_| Problem::forbidden())?
            .ok_or_else(Problem::forbidden)?;

        if row.password_hash.is_some() {
            let jar = CookieJar::from_headers(&parts.headers);
            let unlock = parts
                .headers
                .get("x-share-unlock")
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned)
                .or_else(|| jar.get(SHARE_UNLOCK_COOKIE).map(|c| c.value().to_owned()))
                .ok_or_else(Problem::forbidden)?;
            let unlock_token = ShareToken::from_string(unlock);
            if !state.share_unlocks.check(row.id, &unlock_token) {
                return Err(Problem::forbidden());
            }
        }

        let ctx = AuthContext::share_link(
            row.id,
            keeppix_domain::ShareLinkParams {
                object_type: row.object_type,
                object_id: row.object_id,
                allow_download: row.allow_download,
                allow_original: row.allow_original,
                hide_metadata: row.hide_metadata,
                allow_upload: row.allow_upload,
                upload_quota_bytes: row.upload_quota_bytes,
            },
        );
        Ok(Self(ctx))
    }
}

/// Rejects share-link tokens on routes that must stay session-only.
pub struct SessionNotShare(pub AuthContext);

impl FromRequestParts<AppState> for SessionNotShare {
    type Rejection = Problem;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        reject_public_share_token(&parts.headers)?;
        let Auth(ctx) = Auth::from_request_parts(parts, state).await?;
        Ok(Self(ctx))
    }
}

/// Session cookie or `X-Share-Token` header — used by media routes that must
/// work for both logged-in users and public share links.
pub struct SessionOrShare(pub AuthContext);

impl FromRequestParts<AppState> for SessionOrShare {
    type Rejection = Problem;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let jar = CookieJar::from_headers(&parts.headers);
        if jar.get(SESSION_COOKIE).is_some() {
            let Auth(ctx) = Auth::from_request_parts(parts, state).await?;
            return Ok(Self(ctx));
        }
        let ShareAuth(ctx) = ShareAuth::from_request_parts(parts, state).await?;
        Ok(Self(ctx))
    }
}

/// Like `Auth`, but rejects anyone who isn't an administrator.
pub struct AdminAuth(pub AuthContext);

impl FromRequestParts<AppState> for AdminAuth {
    type Rejection = Problem;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let Auth(ctx) = Auth::from_request_parts(parts, state).await?;
        if !ctx.is_admin() {
            return Err(Problem::forbidden());
        }
        Ok(Self(ctx))
    }
}

#[cfg(test)]
mod tests {
    use super::session_problem;
    use axum::http::StatusCode;
    use keeppix_db::DbError;

    /// Pin on the taxonomy: if someone rewrote `session_problem` as
    /// `|_| Problem::unauthenticated()` — which is how it used to be —
    /// this test fails. It's the only deterministic assertion of this
    /// property: the end-to-end proof (`a_database_outage_is_a_503_not_a_401`
    /// in `tests/auth.rs`) requires shutting down a container and is
    /// skipped where that isn't possible.
    #[test]
    fn a_database_outage_is_transient_a_bad_session_is_not() {
        let outage = session_problem(DbError::Connection(sqlx::Error::PoolClosed));
        assert_eq!(outage.status, StatusCode::SERVICE_UNAVAILABLE.as_u16());
        assert_eq!(outage.type_slug, "keeppix/service-unavailable");

        for invalid in [DbError::NotFound, DbError::Forbidden] {
            let problem = session_problem(invalid);
            assert_eq!(problem.status, StatusCode::UNAUTHORIZED.as_u16());
            assert_eq!(problem.type_slug, "keeppix/unauthenticated");
        }

        let corrupted = session_problem(DbError::Corrupted("unknown role: root".to_owned()));
        assert_eq!(
            corrupted.status,
            StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
            "an unreadable row is not an expired session"
        );
    }
}
