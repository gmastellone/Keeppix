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
        "/api/v1/timeline/buckets",
        "/api/v1/timeline",
        "/api/v1/folders/tree",
        "/api/v1/folders/{id}",
        "/api/v1/folders/{id}/children",
        "/media/thumb/{hash}",
        "/media/preview/{hash}",
        "/media/full/{hash}",
        "/media/original/{id}",
        "/api/v1/viewport",
        "/api/v1/search",
        "/api/v1/search/suggest",
        "/api/v1/places/reverse",
        "/api/v1/places/suggest",
        "/api/v1/map/clusters",
        "/api/v1/saved-searches",
        "/api/v1/ws",
        "/api/v1/ws/ticket",
        "/api/v1/problems",
        "/api/v1/duplicates",
        "/api/v1/duplicates/{content_hash}",
        "/api/v1/duplicates/{content_hash}/resolve",
        "/api/v1/assets/{id}",
        "/api/v1/assets/{id}/restore",
        "/api/v1/assets/{id}/stack",
        "/api/v1/assets/{id}/stack/primary",
        "/api/v1/trash",
        "/api/v1/trash/empty",
        "/api/v1/assets/{id}/metadata",
        "/api/v1/metadata/batch",
        "/api/v1/metadata/batch/copy-location",
        "/api/v1/metadata/batch/import-gpx",
        "/api/v1/metadata/batch/recalculate-timezones",
        "/api/v1/metadata/batch/recalculate-timezones/preview",
        "/api/v1/metadata/batch/shift-taken-at",
        "/api/v1/metadata/batch/{batch_id}/undo",
        "/api/v1/assets/{id}/flags",
        "/api/v1/flags/batch",
        "/api/v1/users/me/app-passwords",
        "/api/v1/users/me/app-passwords/{id}",
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
    assert_eq!(
        checked, 72,
        "il documento deve descrivere settantadue operazioni"
    );
}

/// I nomi degli schemi di sicurezza sono letterali dentro `#[utoipa::path]` e
/// non possono riferirsi a `openapi::SESSION_SCHEME`: se le due scritture
/// divergono, il documento dichiara `security` verso uno schema inesistente e
/// un generatore di client non sa quale credenziale mandare. Qui si verifica
/// che ogni requisito punti a uno schema dichiarato, che le due rotte protette
/// lo abbiano, e che il cookie descritto sia davvero quello che l'extractor
/// legge.
#[test]
#[allow(clippy::unwrap_used, clippy::too_many_lines)]
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
    assert_eq!(
        protected,
        [
            "/api/v1/assets/{id}",
            "/api/v1/assets/{id}",
            "/api/v1/assets/{id}/flags",
            "/api/v1/assets/{id}/flags",
            "/api/v1/assets/{id}/metadata",
            "/api/v1/assets/{id}/metadata",
            "/api/v1/assets/{id}/restore",
            "/api/v1/assets/{id}/stack",
            "/api/v1/assets/{id}/stack/primary",
            "/api/v1/auth/me",
            "/api/v1/auth/refresh",
            "/api/v1/duplicates",
            "/api/v1/duplicates/{content_hash}",
            "/api/v1/duplicates/{content_hash}/resolve",
            "/api/v1/flags/batch",
            "/api/v1/folders/tree",
            "/api/v1/folders/{id}",
            "/api/v1/folders/{id}/children",
            "/api/v1/libraries",
            "/api/v1/libraries",
            "/api/v1/libraries/preview",
            "/api/v1/libraries/{id}",
            "/api/v1/libraries/{id}",
            "/api/v1/libraries/{id}",
            "/api/v1/libraries/{id}/scan",
            "/api/v1/libraries/{id}/scan",
            "/api/v1/map/clusters",
            "/api/v1/map/regions",
            "/api/v1/map/regions",
            "/api/v1/map/regions/{id}",
            "/api/v1/map/regions/{id}/cancel",
            "/api/v1/map/tiles/{region}/{z}/{x}/{y}",
            "/api/v1/metadata/batch",
            "/api/v1/metadata/batch/copy-location",
            "/api/v1/metadata/batch/import-gpx",
            "/api/v1/metadata/batch/recalculate-timezones",
            "/api/v1/metadata/batch/recalculate-timezones/preview",
            "/api/v1/metadata/batch/shift-taken-at",
            "/api/v1/metadata/batch/{batch_id}/undo",
            "/api/v1/places/reverse",
            "/api/v1/places/suggest",
            "/api/v1/problems",
            "/api/v1/saved-searches",
            "/api/v1/saved-searches",
            "/api/v1/search",
            "/api/v1/search/suggest",
            "/api/v1/timeline",
            "/api/v1/timeline/buckets",
            "/api/v1/trash",
            "/api/v1/trash/empty",
            "/api/v1/users",
            "/api/v1/users",
            "/api/v1/users/me/app-passwords",
            "/api/v1/users/me/app-passwords",
            "/api/v1/users/me/app-passwords/{id}",
            "/api/v1/users/me/home",
            "/api/v1/users/me/home",
            "/api/v1/users/me/password",
            "/api/v1/users/{id}",
            "/api/v1/users/{id}/disable",
            "/api/v1/users/{id}/enable",
            "/api/v1/viewport",
            "/api/v1/ws",
            "/api/v1/ws/ticket",
            "/media/full/{hash}",
            "/media/original/{id}",
            "/media/preview/{hash}",
            "/media/thumb/{hash}",
        ]
    );
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
            "app_passwords_create",
            "app_passwords_list",
            "app_passwords_revoke",
            "assets_delete",
            "assets_get",
            "assets_restore",
            "assets_stack_get",
            "assets_stack_set_primary",
            "auth_login",
            "auth_logout",
            "auth_me",
            "auth_refresh",
            "duplicates_list",
            "duplicates_members",
            "duplicates_resolve",
            "flags_batch_set",
            "flags_get",
            "flags_set",
            "folders_children",
            "folders_move",
            "folders_tree",
            "libraries_create",
            "libraries_delete",
            "libraries_get",
            "libraries_list",
            "libraries_patch",
            "libraries_preview",
            "libraries_scan_start",
            "libraries_scan_status",
            "map_clusters",
            "map_regions_cancel",
            "map_regions_delete",
            "map_regions_download",
            "map_regions_list",
            "map_tile_archive",
            "media_full",
            "media_original",
            "media_preview",
            "media_thumb",
            "metadata_apply",
            "metadata_apply_batch",
            "metadata_copy_location",
            "metadata_effective",
            "metadata_import_gpx",
            "metadata_recalculate_timezones_apply",
            "metadata_recalculate_timezones_preview",
            "metadata_shift_taken_at",
            "metadata_undo_batch",
            "places_reverse",
            "places_suggest",
            "problems_list",
            "saved_searches_create",
            "saved_searches_list",
            "search_run",
            "search_suggest",
            "setup_create",
            "setup_status",
            "timeline_buckets",
            "timeline_page",
            "trash_empty",
            "trash_list",
            "users_change_password",
            "users_create",
            "users_delete_home",
            "users_disable",
            "users_enable",
            "users_list",
            "users_patch",
            "users_set_home",
            "viewport_promote",
            "ws_connect",
            "ws_ticket"
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

    if std::env::var("UPDATE_OPENAPI").as_deref() == Ok("1") {
        std::fs::write(&path, generated.trim_end().to_string() + "\n").unwrap();
        return;
    }

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
