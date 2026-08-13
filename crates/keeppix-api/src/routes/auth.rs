use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::{Json, response::IntoResponse};
use axum_extra::extract::CookieJar;
use keeppix_db::{SessionRepo, UserRepo};
use keeppix_domain::{Password, SessionToken, SystemRole, User, Username, verify_password};
use serde::{Deserialize, Serialize};

use crate::cookie::{clearing_cookie, session_cookie, should_be_secure};
use crate::extract::{Auth, SESSION_COOKIE};
use crate::problem::Problem;
use crate::routes::setup::{host, user_agent};
use crate::state::AppState;

/// Rappresentazione pubblica dell'utente. Non contiene l'hash della password
/// né il segreto TOTP: quei campi non lasciano mai il database.
#[derive(Serialize, utoipa::ToSchema)]
pub struct UserView {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub email: Option<String>,
    // Il campo resta `&'static str` (è una costante scelta dal server, non un
    // dato allocato): al documento basta sapere che sul filo è una stringa.
    #[schema(value_type = String)]
    pub role: &'static str,
    pub locale: Option<String>,
}

impl From<&User> for UserView {
    fn from(u: &User) -> Self {
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
        }
    }
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct LoginRequest {
    username: String,
    password: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct LoginResponse {
    user: UserView,
}

/// # Errors
/// `401 invalid-credentials` per utente inesistente, password errata o account
/// disabilitato: le tre situazioni sono indistinguibili dall'esterno.
#[utoipa::path(
    post,
    path = "/api/v1/auth/login",
    tag = "auth",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Sessione aperta", body = LoginResponse),
        (status = 401, description = "Credenziali non valide")
    )
)]
pub async fn login(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Json(req): Json<LoginRequest>,
) -> Result<impl IntoResponse, Problem> {
    let invalid = || {
        Problem::new(
            StatusCode::UNAUTHORIZED,
            "invalid-credentials",
            "Invalid credentials",
        )
    };

    let username = Username::parse(&req.username).map_err(|_| invalid())?;
    let password = Password::parse(&req.password).map_err(|_| invalid())?;

    let found = UserRepo::new(&state.db).find_by_username(&username).await?;
    let Some((user, hash)) = found else {
        // Verifica fittizia per non far trapelare l'esistenza dell'utente
        // dal tempo di risposta: l'hash sotto è un Argon2id valido, quindi
        // `verify_password` esegue l'intero calcolo prima di fallire.
        let _ = verify_password(&password, &dummy_hash());
        return Err(invalid());
    };

    if !verify_password(&password, &hash) || !user.is_active() {
        return Err(invalid());
    }

    let token = SessionRepo::new(&state.db)
        .create(user.id, state.session_ttl, user_agent(&headers))
        .await?;

    let secure = should_be_secure(host(&headers));
    let jar = jar.add(session_cookie(&token, state.session_ttl, secure));

    Ok((
        StatusCode::OK,
        jar,
        Json(LoginResponse {
            user: UserView::from(&user),
        }),
    ))
}

/// Hash costante usato solo per pareggiare i tempi di risposta quando lo
/// username non esiste. Deve essere un Argon2id **valido** — altrimenti
/// `verify_password` fallisce a livello di parsing senza mai eseguire
/// Argon2, e la differenza di tempo che questa funzione dovrebbe mascherare
/// resta interamente visibile. Generato una tantum con `hash_password` su
/// una password arbitraria mai usata per un login reale; vedi il test
/// `dummy_hash_is_a_valid_argon2id_phc_string` sotto.
fn dummy_hash() -> keeppix_domain::PasswordHash {
    keeppix_domain::PasswordHash::from_stored(
        "$argon2id$v=19$m=19456,t=2,p=1$BKjMC3FKz54nTDnFf9fLRQ$\
         Lckl7W7KbvukoSApSxfeAzdhbmnPBAyeHtIIl9Dhmhs"
            .to_owned(),
    )
}

/// # Errors
/// `401 unauthenticated` se il cookie manca, è scaduto o è stato riusato dopo
/// il consumo — in quest'ultimo caso l'intera famiglia è già stata revocata.
#[utoipa::path(
    post,
    path = "/api/v1/auth/refresh",
    tag = "auth",
    responses(
        (status = 204, description = "Sessione ruotata, nuovo cookie emesso"),
        (status = 401, description = "Cookie assente, scaduto o già consumato")
    )
)]
pub async fn refresh(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
) -> Result<impl IntoResponse, Problem> {
    let cookie = jar
        .get(SESSION_COOKIE)
        .ok_or_else(Problem::unauthenticated)?;
    let token = SessionToken::from_string(cookie.value().to_owned());

    let next = SessionRepo::new(&state.db)
        .rotate(&token, state.session_ttl)
        .await
        .map_err(|_| Problem::unauthenticated())?;

    let secure = should_be_secure(host(&headers));
    let jar = jar.add(session_cookie(&next, state.session_ttl, secure));

    Ok((StatusCode::NO_CONTENT, jar))
}

/// Sempre `204`, anche senza cookie: uscire deve funzionare comunque.
#[utoipa::path(
    post,
    path = "/api/v1/auth/logout",
    tag = "auth",
    responses((status = 204, description = "Sessione chiusa e cookie ripulito"))
)]
pub async fn logout(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Some(cookie) = jar.get(SESSION_COOKIE) {
        let token = SessionToken::from_string(cookie.value().to_owned());
        if let Err(e) = SessionRepo::new(&state.db).revoke(&token).await {
            tracing::warn!(error = %e, "revoca sessione fallita");
        }
    }
    let secure = should_be_secure(host(&headers));
    (StatusCode::NO_CONTENT, jar.add(clearing_cookie(secure)))
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct MeResponse {
    user: UserView,
}

/// # Errors
/// `401` se non autenticato, `404` se l'utente è stato nel frattempo rimosso.
#[utoipa::path(
    get,
    path = "/api/v1/auth/me",
    tag = "auth",
    responses(
        (status = 200, description = "Utente della sessione corrente", body = MeResponse),
        (status = 401, description = "Non autenticato")
    )
)]
pub async fn me(
    State(state): State<AppState>,
    Auth(ctx): Auth,
) -> Result<Json<MeResponse>, Problem> {
    let id = ctx.user_id().ok_or_else(Problem::unauthenticated)?;
    let user = UserRepo::new(&state.db).find_by_id(&ctx, id).await?;
    Ok(Json(MeResponse {
        user: UserView::from(&user),
    }))
}

#[cfg(test)]
mod tests {
    use super::dummy_hash;
    use keeppix_domain::{Password, verify_password};

    /// Plaintext usato una tantum per generare la costante `dummy_hash()`.
    /// Pubblicarlo qui è innocuo: nessun account usa questa password, e la
    /// difesa ha bisogno di *tempo* di verifica comparabile, non di segretezza.
    const DUMMY_HASH_PLAINTEXT: &str = "this password is never used to log in";

    /// Pin sul bug che questa funzione corregge: il `dummy_hash()` originale
    /// della brief era un PHC malformato, quindi `verify_password` falliva
    /// nel *parsing* e non eseguiva mai Argon2 — la differenza di tempo tra
    /// "utente inesistente" e "password errata" restava intera.
    ///
    /// `starts_with("$argon2id$")`, `contains("m=19456,t=2,p=1")` e
    /// `!verify_password(altra_password, ..)` da soli non bastano a pinnare
    /// questo: un hash corrotto proprio nell'ultimo segmento — cioè lo stesso
    /// identico bug — supererebbe comunque tutti e tre, perché un fallimento
    /// di parsing restituisce `false` in modo indistinguibile da un mismatch
    /// reale. Solo un match **positivo** contro il plaintext che ha generato
    /// l'hash dimostra che il parsing è riuscito e che Argon2 ha girato per
    /// intero.
    #[test]
    #[allow(clippy::unwrap_used)]
    fn dummy_hash_is_a_valid_argon2id_phc_string() {
        let hash = dummy_hash();
        assert!(hash.as_str().starts_with("$argon2id$"));
        assert!(hash.as_str().contains("m=19456,t=2,p=1"));

        let matching = Password::parse(DUMMY_HASH_PLAINTEXT).unwrap();
        assert!(
            verify_password(&matching, &hash),
            "il parsing dell'hash deve riuscire e Argon2 deve girare per intero"
        );

        // Nessuna password reale di login deve verificare contro l'hash
        // fittizio: il suo plaintext non corrisponde a nessun account.
        let attempted = Password::parse("correct horse battery staple").unwrap();
        assert!(!verify_password(&attempted, &hash));
    }
}
