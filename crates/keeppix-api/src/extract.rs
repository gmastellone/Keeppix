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

/// Traduce l'errore di una verifica di sessione. È l'unico posto in cui questa
/// decisione viene presa: la usano sia l'extractor `Auth` sia l'handler
/// `refresh`, che sono i due punti in cui un token viene consultato.
///
/// La distinzione che conta è fra «questa sessione non vale» e «il database non
/// risponde». Mappare tutto su `401` faceva sì che dieci secondi di riavvio di
/// Postgres si presentassero a ogni client come «sessione scaduta»: il
/// frontend azzera l'utente e la guardia del router lo manda a `/login`, cioè
/// un logout di massa che non compare come 5xx in nessuna metrica. Un `503`
/// con `Retry-After` dice al client la verità — riprova, non rifare il login.
pub(crate) fn session_problem(err: DbError) -> Problem {
    match err {
        DbError::Connection(e) => {
            tracing::error!(error = %e, "session lookup failed: database unavailable");
            Problem::service_unavailable()
        }
        // Token sconosciuto, scaduto, revocato, consumato, utente disabilitato,
        // riuso rilevato: la sessione non vale, e il client deve rifare il
        // login. Le cause non si distinguono di proposito.
        DbError::NotFound | DbError::Forbidden => Problem::unauthenticated(),
        // Riga illeggibile (per esempio un `role` fuori dal CHECK) o migrazione
        // fallita: non è una sessione non valida, è un difetto del server. Non
        // concede accesso, ma non va nemmeno spacciato per scadenza.
        other => {
            tracing::error!(error = %other, "session lookup failed");
            Problem::internal()
        }
    }
}

/// Estrae il contesto di autenticazione dal cookie di sessione.
/// Ogni handler che tratta dati di un utente **deve** prendere questo
/// extractor: è il modo in cui l'`AuthContext` raggiunge i repository.
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

/// Come `Auth`, ma rifiuta chi non è amministratore.
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

    /// Pin sulla tassonomia: se qualcuno riscrivesse `session_problem` come
    /// `|_| Problem::unauthenticated()` — che è com'era prima — questo test
    /// fallisce. È l'unica asserzione deterministica sulla proprietà: la prova
    /// end-to-end (`a_database_outage_is_a_503_not_a_401` in `tests/auth.rs`)
    /// richiede di spegnere un container e si salta dove non è possibile.
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
            "una riga illeggibile non è una sessione scaduta"
        );
    }
}
