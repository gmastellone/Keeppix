## Task 10: Setup iniziale e autenticazione

**Files:**
- Create: `crates/keeppix-api/src/routes/setup.rs`
- Create: `crates/keeppix-api/src/routes/auth.rs`
- Create: `crates/keeppix-api/src/cookie.rs`
- Create: `crates/keeppix-api/tests/harness/mod.rs`
- Create: `crates/keeppix-api/tests/auth.rs`
- Modify: `crates/keeppix-api/src/lib.rs`, `crates/keeppix-api/src/routes/mod.rs`, `crates/keeppix-api/Cargo.toml`

**Interfaces:**
- Consumes: `AppState`, `Problem`, `Auth`, `SESSION_COOKIE` (Task 9); `UserRepo`, `SessionRepo` (Task 5, 7); `Password`, `Username`, `hash_password`, `verify_password` (Task 2-3).
- Produces gli endpoint:
  - `GET /api/v1/setup/status` → `{ "initialised": bool }`. Pubblico.
  - `POST /api/v1/setup` con `{ username, display_name, email?, password }` → `201` + cookie di sessione + `{ user }`. `409 keeppix/already-initialised` se esistono già utenti.
  - `POST /api/v1/auth/login` con `{ username, password }` → `200` + cookie + `{ user }`. `401 keeppix/invalid-credentials`.
  - `POST /api/v1/auth/refresh` → `204` + cookie ruotato. `401` se riuso rilevato.
  - `POST /api/v1/auth/logout` → `204` + cookie cancellato.
  - `GET /api/v1/auth/me` → `{ user }`. Richiede `Auth`.
- Produce inoltre `cookie::session_cookie(token, ttl) -> Cookie` e `cookie::clearing_cookie() -> Cookie`.

- [ ] **Step 1: Aggiungere le dipendenze**

```bash
cargo add keeppix-domain --path crates/keeppix-domain -p keeppix-api
cargo add --dev testcontainers-modules --features postgres -p keeppix-api
cargo add --dev testcontainers tokio --features macros,rt-multi-thread -p keeppix-api
cargo add --dev reqwest --no-default-features --features json,rustls-tls,cookies -p keeppix-api
```

- [ ] **Step 2: Scrivere l'harness HTTP**

`crates/keeppix-api/tests/harness/mod.rs`:

```rust
//! Server reale su porta effimera con Postgres reale in container.
//! I test parlano HTTP come un browser, cookie inclusi: è l'unico modo di
//! verificare davvero il comportamento dei cookie di sessione.

use keeppix_db::Db;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::{ContainerAsync, ImageExt};

pub struct TestServer {
    _container: ContainerAsync<Postgres>,
    pub base_url: String,
    pub client: reqwest::Client,
}

impl TestServer {
    /// # Panics
    /// Se Docker non è disponibile o il server non si avvia.
    pub async fn start() -> Self {
        let container = Postgres::default()
            .with_tag("17-3.5")
            .with_name("postgis/postgis")
            .start()
            .await
            .expect("container Postgres");
        let port = container.get_host_port_ipv4(5432).await.expect("porta");
        let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");

        let db = Db::connect(&url, 5).await.expect("connessione");
        db.migrate().await.expect("migrazioni");

        let state = keeppix_api::AppState::new(db, 3600);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("indirizzo");

        tokio::spawn(async move {
            axum::serve(listener, keeppix_api::router(state)).await.ok();
        });

        let client = reqwest::Client::builder()
            .cookie_store(true)
            .build()
            .expect("client http");

        Self { _container: container, base_url: format!("http://{addr}"), client }
    }

    pub fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base_url)
    }
}
```

> **Nota sui cookie nei test:** il cookie usa il prefisso `__Host-`, che richiede `Secure` e quindi HTTPS. In test si parla in chiaro su `127.0.0.1`, dove `reqwest` non memorizzerebbe un cookie `Secure`. Per questo `session_cookie` emette l'attributo `Secure` solo quando la richiesta non arriva da localhost — vedi Step 4.

- [ ] **Step 3: Scrivere i test che falliscono**

`crates/keeppix-api/tests/auth.rs`:

```rust
mod harness;

use harness::TestServer;
use serde_json::json;

#[tokio::test]
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
        .expect("il setup deve autenticare subito")
        .to_str()
        .unwrap()
        .to_owned();
    assert!(cookie.contains("__Host-kpx_session="));
    assert!(cookie.contains("HttpOnly"));
    assert!(cookie.contains("SameSite=Lax"));

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
async fn setup_can_only_run_once() {
    let server = TestServer::start().await;
    let payload = json!({
        "username": "giovanni",
        "display_name": "Giovanni",
        "password": "correct horse battery staple"
    });

    server.client.post(server.url("/api/v1/setup")).json(&payload).send().await.unwrap();

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

    assert_eq!(response.status(), 200, "lo username è case-insensitive");
}

#[tokio::test]
async fn login_fails_with_wrong_password() {
    let server = TestServer::start().await;
    setup(&server).await;

    let response = server
        .client
        .post(server.url("/api/v1/auth/login"))
        .json(&json!({ "username": "giovanni", "password": "password sbagliata" }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 401);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["type"], "keeppix/invalid-credentials");
}

#[tokio::test]
async fn login_fails_identically_for_unknown_user() {
    let server = TestServer::start().await;
    setup(&server).await;

    let response = server
        .client
        .post(server.url("/api/v1/auth/login"))
        .json(&json!({ "username": "nessuno", "password": "correct horse battery staple" }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 401);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(
        body["type"], "keeppix/invalid-credentials",
        "utente inesistente e password errata devono essere indistinguibili"
    );
}

#[tokio::test]
async fn me_requires_authentication() {
    let server = TestServer::start().await;
    setup(&server).await;

    // Client nuovo, senza cookie.
    let anonymous = reqwest::Client::new();
    let response = anonymous.get(server.url("/api/v1/auth/me")).send().await.unwrap();

    assert_eq!(response.status(), 401);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["type"], "keeppix/unauthenticated");
}

#[tokio::test]
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

    let refresh = server.client.post(server.url("/api/v1/auth/refresh")).send().await.unwrap();
    assert_eq!(refresh.status(), 204);
    let after = session_value_from(&refresh);

    assert_ne!(before, after, "il cookie deve cambiare a ogni refresh");

    // Il nuovo cookie continua a valere.
    let me = server.client.get(server.url("/api/v1/auth/me")).send().await.unwrap();
    assert_eq!(me.status(), 200);
}

#[tokio::test]
async fn logout_invalidates_the_session() {
    let server = TestServer::start().await;
    setup(&server).await;

    let response = server.client.post(server.url("/api/v1/auth/logout")).send().await.unwrap();
    assert_eq!(response.status(), 204);

    let me = server.client.get(server.url("/api/v1/auth/me")).send().await.unwrap();
    assert_eq!(me.status(), 401);
}

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

/// Estrae il valore del cookie di sessione dall'header `set-cookie` di una
/// risposta. Il cookie store di `reqwest` non è ispezionabile, quindi si legge
/// direttamente ciò che il server ha emesso.
fn session_value_from(response: &reqwest::Response) -> String {
    response
        .headers()
        .get("set-cookie")
        .expect("set-cookie presente")
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .trim_start_matches("__Host-kpx_session=")
        .to_owned()
}
```

- [ ] **Step 4: Eseguire e verificare il fallimento**

Run: `cargo test -p keeppix-api --test auth`
Expected: FAIL — tutti i test rispondono 404, gli endpoint non esistono.

- [ ] **Step 5: Implementare `cookie.rs`**

```rust
use std::time::Duration;

use axum_extra::extract::cookie::{Cookie, SameSite};
use keeppix_domain::SessionToken;

use crate::extract::SESSION_COOKIE;

/// Cookie di sessione. `__Host-` impone `Secure` e `Path=/`, e vieta `Domain`:
/// il cookie non può essere piazzato da un sottodominio compromesso.
///
/// `secure` è parametrico perché in test si parla in chiaro su 127.0.0.1, dove
/// un client conforme scarterebbe un cookie `Secure`.
#[must_use]
pub fn session_cookie(token: &SessionToken, ttl: Duration, secure: bool) -> Cookie<'static> {
    let mut cookie = Cookie::new(SESSION_COOKIE, token.as_str().to_owned());
    cookie.set_http_only(true);
    cookie.set_secure(secure);
    cookie.set_same_site(SameSite::Lax);
    cookie.set_path("/");
    cookie.set_max_age(Some(
        time::Duration::try_from(ttl).unwrap_or(time::Duration::days(30)),
    ));
    cookie
}

#[must_use]
pub fn clearing_cookie() -> Cookie<'static> {
    let mut cookie = Cookie::new(SESSION_COOKIE, "");
    cookie.set_http_only(true);
    cookie.set_path("/");
    cookie.set_max_age(Some(time::Duration::ZERO));
    cookie
}

/// Vero quando la richiesta non arriva da localhost: in produzione si sta
/// dietro HTTPS, quindi il cookie deve essere `Secure`.
#[must_use]
pub fn should_be_secure(host: Option<&str>) -> bool {
    !matches!(host, Some(h) if h.starts_with("127.0.0.1") || h.starts_with("localhost"))
}
```

Aggiungere la dipendenza: `cargo add time -p keeppix-api`

- [ ] **Step 6: Implementare `routes/setup.rs`**

```rust
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::{Json, response::IntoResponse};
use axum_extra::extract::CookieJar;
use keeppix_db::{SessionRepo, UserRepo};
use keeppix_domain::{NewUser, Password, SystemRole, Username, hash_password};
use serde::{Deserialize, Serialize};

use crate::cookie::{session_cookie, should_be_secure};
use crate::problem::Problem;
use crate::routes::auth::UserView;
use crate::state::AppState;

#[derive(Serialize)]
pub struct SetupStatus {
    initialised: bool,
}

/// # Errors
/// `Problem` se il conteggio degli utenti fallisce.
pub async fn status(State(state): State<AppState>) -> Result<Json<SetupStatus>, Problem> {
    let count = UserRepo::new(&state.db).count().await?;
    Ok(Json(SetupStatus { initialised: count > 0 }))
}

#[derive(Deserialize)]
pub struct SetupRequest {
    username: String,
    display_name: String,
    email: Option<String>,
    password: String,
}

#[derive(Serialize)]
pub struct SetupResponse {
    user: UserView,
}

/// Crea il primo amministratore e apre subito una sessione.
///
/// # Errors
/// `409 already-initialised` se l'istanza è già configurata;
/// `422 invalid-username` / `422 invalid-password` sui dati non validi.
pub async fn create(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Json(req): Json<SetupRequest>,
) -> Result<impl IntoResponse, Problem> {
    let username = Username::parse(&req.username).map_err(|e| {
        Problem::new(StatusCode::UNPROCESSABLE_ENTITY, "invalid-username", "Invalid username")
            .with_detail(e.to_string())
    })?;
    let password = Password::parse(&req.password).map_err(|e| {
        Problem::new(StatusCode::UNPROCESSABLE_ENTITY, "invalid-password", "Invalid password")
            .with_detail(e.to_string())
    })?;
    let hash = hash_password(&password).map_err(|_| Problem::internal())?;

    let users = UserRepo::new(&state.db);
    let user = users
        .create_bootstrap_admin(NewUser {
            username,
            email: req.email,
            display_name: req.display_name,
            password_hash: hash.as_str().to_owned(),
            role: SystemRole::Admin,
        })
        .await
        .map_err(|e| match e {
            keeppix_db::DbError::Conflict(_) => Problem::new(
                StatusCode::CONFLICT,
                "already-initialised",
                "Instance is already initialised",
            ),
            other => Problem::from(other),
        })?;

    let token = SessionRepo::new(&state.db)
        .create(user.id, state.session_ttl, user_agent(&headers))
        .await?;

    let secure = should_be_secure(host(&headers));
    let jar = jar.add(session_cookie(&token, state.session_ttl, secure));

    Ok((
        StatusCode::CREATED,
        jar,
        Json(SetupResponse { user: UserView::from(&user) }),
    ))
}

pub(crate) fn user_agent(headers: &HeaderMap) -> Option<&str> {
    headers.get(header::USER_AGENT).and_then(|v| v.to_str().ok())
}

pub(crate) fn host(headers: &HeaderMap) -> Option<&str> {
    headers.get(header::HOST).and_then(|v| v.to_str().ok())
}
```

- [ ] **Step 7: Implementare `routes/auth.rs`**

```rust
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::{Json, response::IntoResponse};
use axum_extra::extract::CookieJar;
use keeppix_db::{SessionRepo, UserRepo};
use keeppix_domain::{
    AuthContext, Password, SessionToken, SystemRole, User, Username, verify_password,
};
use serde::{Deserialize, Serialize};

use crate::cookie::{clearing_cookie, session_cookie, should_be_secure};
use crate::extract::{Auth, SESSION_COOKIE};
use crate::problem::Problem;
use crate::routes::setup::{host, user_agent};
use crate::state::AppState;

/// Rappresentazione pubblica dell'utente. Non contiene l'hash della password
/// né il segreto TOTP: quei campi non lasciano mai il database.
#[derive(Serialize)]
pub struct UserView {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub email: Option<String>,
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

#[derive(Deserialize)]
pub struct LoginRequest {
    username: String,
    password: String,
}

#[derive(Serialize)]
pub struct LoginResponse {
    user: UserView,
}

/// # Errors
/// `401 invalid-credentials` per utente inesistente, password errata o account
/// disabilitato: le tre situazioni sono indistinguibili dall'esterno.
pub async fn login(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Json(req): Json<LoginRequest>,
) -> Result<impl IntoResponse, Problem> {
    let invalid = || {
        Problem::new(StatusCode::UNAUTHORIZED, "invalid-credentials", "Invalid credentials")
    };

    let username = Username::parse(&req.username).map_err(|_| invalid())?;
    let password = Password::parse(&req.password).map_err(|_| invalid())?;

    let found = UserRepo::new(&state.db).find_by_username(&username).await?;
    let Some((user, hash)) = found else {
        // Verifica fittizia per non far trapelare l'esistenza dell'utente
        // dal tempo di risposta.
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

    Ok((StatusCode::OK, jar, Json(LoginResponse { user: UserView::from(&user) })))
}

/// Hash costante usato solo per pareggiare i tempi di risposta.
fn dummy_hash() -> keeppix_domain::PasswordHash {
    keeppix_domain::PasswordHash::from_stored(
        "$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHR2YWx1ZQ$\
         0000000000000000000000000000000000000000000"
            .to_owned(),
    )
}

/// # Errors
/// `401 unauthenticated` se il cookie manca, è scaduto o è stato riusato dopo
/// il consumo — in quest'ultimo caso l'intera famiglia è già stata revocata.
pub async fn refresh(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
) -> Result<impl IntoResponse, Problem> {
    let cookie = jar.get(SESSION_COOKIE).ok_or_else(Problem::unauthenticated)?;
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
pub async fn logout(State(state): State<AppState>, jar: CookieJar) -> impl IntoResponse {
    if let Some(cookie) = jar.get(SESSION_COOKIE) {
        let token = SessionToken::from_string(cookie.value().to_owned());
        if let Err(e) = SessionRepo::new(&state.db).revoke(&token).await {
            tracing::warn!(error = %e, "revoca sessione fallita");
        }
    }
    (StatusCode::NO_CONTENT, jar.add(clearing_cookie()))
}

#[derive(Serialize)]
pub struct MeResponse {
    user: UserView,
}

/// # Errors
/// `401` se non autenticato, `404` se l'utente è stato nel frattempo rimosso.
pub async fn me(
    State(state): State<AppState>,
    Auth(ctx): Auth,
) -> Result<Json<MeResponse>, Problem> {
    let id = ctx.user_id().ok_or_else(Problem::unauthenticated)?;
    let user = UserRepo::new(&state.db).find_by_id(&ctx, id).await?;
    Ok(Json(MeResponse { user: UserView::from(&user) }))
}

/// Riesportato per gli handler che devono costruire un contesto a mano.
pub type Ctx = AuthContext;
```

- [ ] **Step 8: Montare le rotte**

`routes/mod.rs`:

```rust
pub mod auth;
pub mod health;
pub mod setup;
```

In `lib.rs`, sostituire `base_router` e `base_router_stateless`:

```rust
fn api_routes() -> Router<AppState> {
    Router::new()
        .route("/setup/status", get(routes::setup::status))
        .route("/setup", axum::routing::post(routes::setup::create))
        .route("/auth/login", axum::routing::post(routes::auth::login))
        .route("/auth/refresh", axum::routing::post(routes::auth::refresh))
        .route("/auth/logout", axum::routing::post(routes::auth::logout))
        .route("/auth/me", get(routes::auth::me))
}

fn base_router() -> Router<AppState> {
    common_layers(
        Router::new()
            .route("/health", get(routes::health::get))
            .nest("/api/v1", api_routes()),
    )
}
```

E aggiungere `pub mod cookie;` in cima a `lib.rs`.

`base_router_stateless` resta com'è: serve solo ai test di `/health`.

- [ ] **Step 9: Eseguire i test**

Run: `cargo test -p keeppix-api`
Expected: PASS — 13 test (3 di health, 10 di auth).

- [ ] **Step 10: Verificare i lint su tutto il workspace**

Run: `cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --check`
Expected: nessun warning.

- [ ] **Step 11: Commit**

```bash
git add crates/keeppix-api
git commit -m "feat(api): add first-run setup and session authentication"
```

---

