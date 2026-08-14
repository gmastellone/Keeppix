use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum_extra::extract::CookieJar;
use keeppix_db::{DbError, SessionRepo};
use keeppix_domain::{AuthContext, SessionToken};

use crate::problem::Problem;
use crate::state::AppState;

pub const SESSION_COOKIE: &str = "__Host-kpx_session";

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

        let ctx = SessionRepo::new(&state.db)
            .authenticate(&token)
            .await
            .map_err(session_problem)?;

        if let Some(hook) = &state.on_authenticated {
            hook();
        }

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
