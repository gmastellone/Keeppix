mod harness;

use harness::TestServer;
use serde_json::json;

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
        .expect("il setup deve autenticare subito")
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

    assert_eq!(response.status(), 200, "lo username è case-insensitive");
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
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
#[allow(clippy::unwrap_used)]
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
#[allow(clippy::unwrap_used)]
async fn me_requires_authentication() {
    let server = TestServer::start().await;
    setup(&server).await;

    // Client nuovo, senza cookie.
    let anonymous = reqwest::Client::new();
    let response = anonymous
        .get(server.url("/api/v1/auth/me"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 401);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["type"], "keeppix/unauthenticated");
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

    let refresh = server
        .client
        .post(server.url("/api/v1/auth/refresh"))
        .send()
        .await
        .unwrap();
    assert_eq!(refresh.status(), 204);
    let after = session_value_from(&refresh);

    assert_ne!(before, after, "il cookie deve cambiare a ogni refresh");

    // Il nuovo cookie continua a valere.
    let me = server
        .client
        .get(server.url("/api/v1/auth/me"))
        .send()
        .await
        .unwrap();
    assert_eq!(me.status(), 200);

    // Il vecchio cookie, invece, non deve più valere: la rotazione deve aver
    // consumato il genitore, non solo emesso un figlio in parallelo. Un
    // client fresco senza cookie store, con il valore pre-refresh presentato
    // esplicitamente, è l'unico modo di dimostrarlo — il cookie store di
    // `server.client` ha già sostituito `before` con `after`.
    let replay_me = reqwest::Client::new()
        .get(server.url("/api/v1/auth/me"))
        .header("cookie", format!("__Host-kpx_session={before}"))
        .send()
        .await
        .unwrap();
    assert_eq!(
        replay_me.status(),
        401,
        "il token pre-refresh deve essere stato consumato, non solo affiancato da uno nuovo"
    );
}

/// `SessionRepo::rotate` revoca l'intera famiglia quando un token già
/// consumato viene ripresentato — il segnale che una copia sia finita in
/// mano a qualcun altro. La documentazione di `refresh` lo promette
/// esplicitamente, ma senza questo test la copertura HTTP di quel ramo era
/// nulla.
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

    // Consuma il token una prima volta con il flusso normale.
    let first_refresh = server
        .client
        .post(server.url("/api/v1/auth/refresh"))
        .send()
        .await
        .unwrap();
    assert_eq!(first_refresh.status(), 204);

    // Ripresentare il token pre-refresh, già consumato, deve essere rifiutato.
    let reused = reqwest::Client::new()
        .post(server.url("/api/v1/auth/refresh"))
        .header("cookie", format!("__Host-kpx_session={before}"))
        .send()
        .await
        .unwrap();
    assert_eq!(reused.status(), 401);

    // Il 401 sopra da solo non distingue "token consumato rifiutato" da
    // "intera famiglia revocata": il primo lo darebbe anche una `rotate` che
    // si limitasse a restituire un errore. La prova del ramo di revoca è che
    // anche il token *nuovo* — emesso dalla rotazione e valido fino a un
    // istante fa — smetta di funzionare.
    let after = session_value_from(&first_refresh);
    let survivor = reqwest::Client::new()
        .get(server.url("/api/v1/auth/me"))
        .header("cookie", format!("__Host-kpx_session={after}"))
        .send()
        .await
        .unwrap();
    assert_eq!(
        survivor.status(),
        401,
        "il riuso deve revocare l'intera famiglia, non solo il token ripresentato"
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

    let response = server
        .client
        .post(server.url("/api/v1/auth/logout"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 204);

    // Client fresco, senza cookie store: replica esplicitamente il cookie
    // pre-logout. Se ci affidassimo al cookie store di `server.client`, la
    // richiesta successiva partirebbe senza alcun cookie — il logout locale
    // del client, non la revoca lato server, spiegherebbe il 401.
    let replay_me = reqwest::Client::new()
        .get(server.url("/api/v1/auth/me"))
        .header("cookie", format!("__Host-kpx_session={session_value}"))
        .send()
        .await
        .unwrap();
    assert_eq!(
        replay_me.status(),
        401,
        "la sessione deve essere invalidata lato server, non solo dimenticata dal client"
    );
}

/// Host fittizio di produzione. Contro l'harness l'header `Host` reale è
/// `127.0.0.1:<porta>`, e lì `should_be_secure` restituisce — legittimamente —
/// `false`: un test che non contraffacesse l'header osserverebbe un cookie
/// senza `Secure` e non proverebbe nulla.
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
    // `Max-Age` è il TTL di sessione dell'harness (3600 secondi).
    assert_host_prefix_attributes(&response, "Max-Age=3600");
}

/// Verifica sull'header `set-cookie` grezzo tutti gli attributi che rendono
/// accettabile un cookie con prefisso `__Host-`: un browser conforme scarta
/// **per intero** un `__Host-` privo di `Secure` o di `Path=/`
/// (RFC 6265bis §4.1.3.2). Sul cookie cancellante l'effetto è che il logout non
/// cancella nulla e la sessione sopravvive nel browser; su quello di sessione,
/// che il cookie viaggia anche in chiaro. Nessuna delle due cose si vede in
/// test — l'harness parla HTTP su 127.0.0.1 — quindi la si pinna qui.
///
/// Si legge la risposta, non il cookie store di `reqwest`: quello scarterebbe
/// un cookie `Secure` arrivato su HTTP in chiaro, nascondendo proprio ciò che
/// si vuole osservare.
#[allow(clippy::unwrap_used, clippy::expect_used)]
fn assert_host_prefix_attributes(response: &reqwest::Response, expected_max_age: &str) {
    let set_cookie = response
        .headers()
        .get("set-cookie")
        .expect("set-cookie presente")
        .to_str()
        .unwrap()
        .to_owned();

    // Gli attributi si confrontano per intero, non con `contains` sull'header:
    // il valore del token è casuale e potrebbe contenere una qualsiasi di
    // queste stringhe.
    let mut parts = set_cookie.split(';').map(str::trim);
    let name_value = parts.next().expect("coppia nome=valore");
    let attributes: Vec<&str> = parts.collect();

    assert!(
        name_value.starts_with("__Host-kpx_session="),
        "cookie inatteso: {set_cookie}"
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
            "manca `{expected}` in `{set_cookie}`"
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

/// Estrae il valore del cookie di sessione dall'header `set-cookie` di una
/// risposta. Il cookie store di `reqwest` non è ispezionabile, quindi si legge
/// direttamente ciò che il server ha emesso.
#[allow(clippy::unwrap_used, clippy::expect_used)]
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
