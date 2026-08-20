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
    pub disabled_at: Option<DateTime<Utc>>,
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
            disabled_at: u.disabled_at,
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
/// `401 invalid-credentials` per utente inesistente, password errata o account
/// disabilitato: le tre situazioni sono indistinguibili dall'esterno.
#[utoipa::path(
    post,
    path = "/api/v1/auth/login",
    tag = "auth",
    operation_id = "auth_login",
    summary = "Open a session with username and password",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Sessione aperta", body = LoginResponse),
        (status = 400, description = "Corpo JSON sintatticamente non valido", body = Problem),
        (status = 401, description = "Credenziali non valide", body = Problem),
        (status = 415, description = "Content-Type diverso da application/json", body = Problem),
        (status = 422, description = "Corpo JSON valido ma di forma inattesa", body = Problem),
        (status = 500, description = "Errore del database o nella creazione della sessione", body = Problem)
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
    // heap copy we control (the HTTP body `Bytes` remain — see ledger).
    let password = Password::parse_owned(req.password).map_err(|_| invalid())?;

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
/// il consumo — in quest'ultimo caso l'intera famiglia è già stata revocata;
/// `503 service-unavailable` se il database non risponde.
// Le cause di un `401` restano indistinguibili di proposito: il client non deve
// poter dire una rotazione rifiutata da un token già consumato. L'argomento
// vale però solo fra `NotFound` e `Forbidden` — non per un database
// irraggiungibile, che non rivela nulla sullo stato del token e che, mappato su
// 401, disconnetterebbe tutti a ogni riavvio di Postgres. La distinzione vive
// in un solo posto, `crate::extract::session_problem`.
#[utoipa::path(
    post,
    path = "/api/v1/auth/refresh",
    tag = "auth",
    operation_id = "auth_refresh",
    summary = "Rotate the current session cookie",
    security(("session_cookie" = [])),
    responses(
        (status = 204, description = "Sessione ruotata, nuovo cookie emesso"),
        (status = 401, description = "Cookie assente, scaduto o già consumato", body = Problem),
        (status = 500, description = "Riga di sessione illeggibile", body = Problem),
        (status = 503, description = "Database non raggiungibile: riprovare, la sessione è ancora valida", body = Problem)
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

/// Sempre `204`, anche senza cookie: uscire deve funzionare comunque.
// Nessun `security(...)`: la rotta è deliberatamente utilizzabile anche senza
// cookie, e l'errore di revoca viene loggato, non restituito — 204 è l'unico
// esito possibile.
#[utoipa::path(
    post,
    path = "/api/v1/auth/logout",
    tag = "auth",
    operation_id = "auth_logout",
    summary = "Revoke the current session and clear the cookie",
    responses((status = 204, description = "Sessione chiusa e cookie ripulito"))
)]
pub async fn logout(State(state): State<AppState>, jar: CookieJar) -> impl IntoResponse {
    if let Some(cookie) = jar.get(SESSION_COOKIE) {
        let token = SessionToken::from_string(cookie.value().to_owned());
        state.sessions.drop_token(&token);
        if let Err(e) = SessionRepo::new(&state.db).revoke(&token).await {
            tracing::warn!(error = %e, "revoca sessione fallita");
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
/// `401` se non autenticato, `404` se l'utente è stato nel frattempo rimosso.
#[utoipa::path(
    get,
    path = "/api/v1/auth/me",
    tag = "auth",
    operation_id = "auth_me",
    summary = "Return the current authenticated user",
    security(("session_cookie" = [])),
    responses(
        (status = 200, description = "Utente della sessione corrente", body = MeResponse),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 404, description = "Utente rimosso mentre la sessione era aperta", body = Problem),
        (status = 500, description = "Errore del database", body = Problem),
        (status = 503, description = "Database non raggiungibile durante la verifica della sessione", body = Problem)
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
