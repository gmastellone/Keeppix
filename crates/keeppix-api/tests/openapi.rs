mod harness;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use harness::{TestServer, assert_security_headers};
use tower::ServiceExt as _;

/// Il documento non tocca il database: nasce dalle annotazioni sui tipi.
/// Attenzione: questo router monta solo `/health` e `/api/openapi.json`, non
/// `/api/v1` — per interrogare le rotte reali serve `TestServer`.
fn app() -> axum::Router {
    keeppix_api::router_without_state()
}

/// I metodi che in un Path Item Object di `OpenAPI` 3.1 sono operazioni; le
/// altre chiavi (`summary`, `parameters`, `servers`, …) non lo sono.
const HTTP_METHODS: [&str; 8] = [
    "get", "put", "post", "delete", "options", "head", "patch", "trace",
];

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn openapi_document_is_served_and_complete() {
    let response = app()
        .oneshot(
            Request::builder()
                .uri("/api/openapi.json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = http_body_util::BodyExt::collect(response.into_body())
        .await
        .unwrap()
        .to_bytes();
    let doc: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(doc["openapi"].as_str().unwrap(), "3.1.0");
    assert_eq!(doc["info"]["title"], "Keeppix API");

    for path in [
        "/api/v1/setup/status",
        "/api/v1/setup",
        "/api/v1/auth/login",
        "/api/v1/auth/refresh",
        "/api/v1/auth/logout",
        "/api/v1/auth/me",
    ] {
        assert!(doc["paths"][path].is_object(), "manca il percorso {path}");
    }

    assert!(doc["components"]["schemas"]["UserView"].is_object());
}

/// Pin sul punto in cui è facile sbagliare: la rotta va montata **dentro**
/// l'argomento di `common_layers`, non concatenata dopo la sua chiamata. Nel
/// secondo caso i `.layer(...)` non la avvolgerebbero e il documento uscirebbe
/// senza CSP, nosniff, referrer-policy e permissions-policy — lo stesso difetto
/// che il fallback 404 ha già pinnato in `tests/health.rs`.
#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn openapi_document_carries_the_security_headers() {
    let response = app()
        .oneshot(
            Request::builder()
                .uri("/api/openapi.json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_security_headers(response.headers());
}

/// Lega il documento alle rotte davvero montate. Percorso e metodo di ogni
/// operazione sono stringhe scritte a mano dentro `#[utoipa::path]`: niente nel
/// compilatore le confronta con le `Router::route(...)` di `lib.rs`, quindi
/// senza questo test un refuso nell'attributo pubblicherebbe un'operazione che
/// non esiste — o esiste con un altro metodo — e tutta la suite resterebbe
/// verde. Ogni operazione viene chiamata sul router **reale con stato**: uno
/// status 404 significa percorso inesistente, 405 percorso giusto e metodo
/// sbagliato. Qualunque altro esito va bene: qui non si verifica la logica
/// degli handler, solo che la superficie HTTP descritta esista.
///
/// **Direzione non coperta: rotta montata e non documentata.** Servirebbe
/// enumerare le rotte del router, e axum 0.8 non espone la propria tabella
/// (`Router` non ha API di introspezione). Chi aggiunge una `route(...)` senza
/// il corrispondente `#[utoipa::path]` non trova qui nessun avviso: il
/// controllo va fatto in review, oppure in CI confrontando il documento con
/// l'elenco delle rotte una volta che axum lo renderà leggibile.
#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn documented_operations_are_all_mounted() {
    let server = TestServer::start().await;

    let doc: serde_json::Value = server
        .client
        .get(server.url("/api/openapi.json"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    let mut checked = 0_usize;
    for (path, item) in doc["paths"].as_object().unwrap() {
        for (method, _) in item.as_object().unwrap() {
            if !HTTP_METHODS.contains(&method.as_str()) {
                continue;
            }

            let verb = reqwest::Method::from_bytes(method.to_uppercase().as_bytes()).unwrap();
            let status = server
                .client
                .request(verb, server.url(path))
                .send()
                .await
                .unwrap()
                .status();

            assert_ne!(
                status,
                reqwest::StatusCode::NOT_FOUND,
                "il documento dichiara {method} {path}, ma quel percorso non è montato"
            );
            assert_ne!(
                status,
                reqwest::StatusCode::METHOD_NOT_ALLOWED,
                "il documento dichiara {method} {path}, ma la rotta non accetta quel metodo"
            );
            checked += 1;
        }
    }

    // Senza questo, un documento vuoto — o un `paths` che smette di essere un
    // oggetto di operazioni — farebbe passare il test a ciclo mai eseguito.
    assert_eq!(checked, 6, "il documento deve descrivere sei operazioni");
}

/// I nomi degli schemi di sicurezza sono letterali dentro `#[utoipa::path]` e
/// non possono riferirsi a `openapi::SESSION_SCHEME`: se le due scritture
/// divergono, il documento dichiara `security` verso uno schema inesistente e
/// un generatore di client non sa quale credenziale mandare. Qui si verifica
/// che ogni requisito punti a uno schema dichiarato, che le due rotte protette
/// lo abbiano, e che il cookie descritto sia davvero quello che l'extractor
/// legge.
#[test]
#[allow(clippy::unwrap_used)]
fn security_requirements_name_a_declared_scheme() {
    let doc =
        serde_json::to_value(<keeppix_api::openapi::ApiDoc as utoipa::OpenApi>::openapi()).unwrap();

    let schemes = doc["components"]["securitySchemes"].as_object().unwrap();
    let session = &schemes[keeppix_api::openapi::SESSION_SCHEME];
    assert_eq!(session["type"], "apiKey");
    assert_eq!(session["in"], "cookie");
    assert_eq!(session["name"], keeppix_api::SESSION_COOKIE);

    let mut protected = Vec::new();
    for (path, item) in doc["paths"].as_object().unwrap() {
        for (method, operation) in item.as_object().unwrap() {
            if !HTTP_METHODS.contains(&method.as_str()) {
                continue;
            }
            let Some(requirements) = operation["security"].as_array() else {
                continue;
            };
            for requirement in requirements {
                for name in requirement.as_object().unwrap().keys() {
                    assert!(
                        schemes.contains_key(name),
                        "{method} {path} richiede lo schema {name}, che non è dichiarato in components"
                    );
                }
            }
            protected.push(path.clone());
        }
    }

    protected.sort();
    assert_eq!(protected, ["/api/v1/auth/me", "/api/v1/auth/refresh"]);
}

/// `operationId` diventa il nome del metodo nei client generati e deve essere
/// unico in tutto il documento. Derivarlo dal nome della funzione Rust non
/// basta: `setup::create` e un futuro `albums::create` produrrebbero due
/// `create`. Gli `operation_id` sono quindi espliciti e con prefisso di area;
/// questo test fallisce se due operazioni tornano a chiamarsi allo stesso modo.
#[test]
#[allow(clippy::unwrap_used)]
fn operation_ids_are_explicit_and_unique() {
    let doc =
        serde_json::to_value(<keeppix_api::openapi::ApiDoc as utoipa::OpenApi>::openapi()).unwrap();

    let mut ids = Vec::new();
    for item in doc["paths"].as_object().unwrap().values() {
        for (method, operation) in item.as_object().unwrap() {
            if !HTTP_METHODS.contains(&method.as_str()) {
                continue;
            }
            ids.push(operation["operationId"].as_str().unwrap().to_owned());
        }
    }

    ids.sort();
    let unique = ids.len();
    ids.dedup();
    assert_eq!(ids.len(), unique, "operationId duplicato: {ids:?}");
    assert_eq!(
        ids,
        [
            "auth_login",
            "auth_logout",
            "auth_me",
            "auth_refresh",
            "setup_create",
            "setup_status"
        ]
    );
}

/// Blocca la specifica su disco: da questo file si generano i client mobile,
/// quindi una modifica va vista prima di essere pubblicata, non dopo.
#[test]
#[allow(clippy::unwrap_used)]
fn openapi_snapshot_matches_the_committed_file() {
    let generated =
        serde_json::to_string_pretty(&<keeppix_api::openapi::ApiDoc as utoipa::OpenApi>::openapi())
            .unwrap();

    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/api/openapi.json");

    if !path.exists() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &generated).unwrap();
        return;
    }

    let committed = std::fs::read_to_string(&path).unwrap();
    assert_eq!(
        committed.trim(),
        generated.trim(),
        "il contratto pubblico è cambiato: il codice non produce più il documento \
         committato in docs/api/openapi.json. Da quel file si generano i client \
         Kotlin, Swift, Dart e TypeScript, e lo spec lo dichiara congelato (solo \
         aggiunte entro /api/v1). Non rigenerarlo per far tornare verde il test: \
         guarda che cosa è cambiato e decidi. Se il cambiamento non è voluto, \
         correggi il codice; se lo è ed è compatibile, aggiorna il file committato \
         di proposito e spiega perché nel messaggio di commit."
    );
}
