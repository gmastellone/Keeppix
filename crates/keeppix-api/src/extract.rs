use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum_extra::extract::CookieJar;
use keeppix_db::SessionRepo;
use keeppix_domain::{AuthContext, SessionToken};

use crate::problem::Problem;
use crate::state::AppState;

pub const SESSION_COOKIE: &str = "__Host-kpx_session";

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

        let ctx = SessionRepo::new(&state.db)
            .authenticate(&token)
            .await
            .map_err(|_| Problem::unauthenticated())?;

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
