# Task 11 — Specifica OpenAPI

Estratto da `docs/superpowers/plans/2026-08-13-keeppix-fase-0.md` (sezione
"## Task 11: Specifica OpenAPI", riga ~3740), con le note di pre-volo del
controller in fondo. **Leggi prima gli step verbatim, poi le note: le note
correggono il piano in tre punti e ignorarle fa fallire il task.**

La specifica è ciò da cui verrà generato il client mobile: nasce dal codice,
così non può divergere.

**Files:**
- Create: `crates/keeppix-api/src/openapi.rs`
- Create: `crates/keeppix-api/tests/openapi.rs`
- Modify: `crates/keeppix-api/src/routes/setup.rs`,
  `crates/keeppix-api/src/routes/auth.rs`, `crates/keeppix-api/src/lib.rs`,
  `crates/keeppix-api/Cargo.toml`
- Generate + commit: `docs/api/openapi.json`

**Interfaces:**
- Consumes: gli handler del Task 10.
- Produces: `openapi::ApiDoc` con `ApiDoc::openapi() -> utoipa::openapi::OpenApi`,
  servita da `GET /api/openapi.json`.

---

## Step verbatim dal piano

- [ ] **Step 1: Aggiungere le dipendenze**

```bash
cargo add utoipa --features axum_extras,chrono,uuid -p keeppix-api
```

- [ ] **Step 2: Scrivere il test che fallisce**

`crates/keeppix-api/tests/openapi.rs`:

```rust
use axum::body::Body;
use axum::http::Request;
use tower::ServiceExt as _;

#[tokio::test]
async fn openapi_document_is_served_and_complete() {
    let response = keeppix_api::router_without_state()
        .oneshot(Request::builder().uri("/api/openapi.json").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

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
```

- [ ] **Step 3: Eseguire e verificare il fallimento**

Run: `cargo test -p keeppix-api --test openapi`
Expected: FAIL — 404 su `/api/openapi.json`.

- [ ] **Step 4: Annotare i tipi e gli handler**

In `routes/auth.rs`, aggiungere `ToSchema` alle strutture pubbliche:

```rust
#[derive(Serialize, utoipa::ToSchema)]
pub struct UserView { /* campi invariati */ }

#[derive(Deserialize, utoipa::ToSchema)]
pub struct LoginRequest { /* invariato */ }

#[derive(Serialize, utoipa::ToSchema)]
pub struct LoginResponse { /* invariato */ }

#[derive(Serialize, utoipa::ToSchema)]
pub struct MeResponse { /* invariato */ }
```

E annotare gli handler, per esempio `login`:

```rust
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
pub async fn login(/* invariato */) { /* invariato */ }
```

Ripetere per `refresh` (204/401), `logout` (204), `me` (200 `MeResponse` /
401), `setup::status` (200 `SetupStatus`), `setup::create` (201
`SetupResponse` / 409 / 422). Aggiungere `ToSchema` anche a `SetupStatus`,
`SetupRequest`, `SetupResponse`.

- [ ] **Step 5: Implementare `openapi.rs`**

```rust
use utoipa::OpenApi;

use crate::routes::{auth, setup};

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Keeppix API",
        version = env!("CARGO_PKG_VERSION"),
        description = "API di Keeppix. Contratto congelato: solo aggiunte entro /api/v1."
    ),
    paths(
        setup::status,
        setup::create,
        auth::login,
        auth::refresh,
        auth::logout,
        auth::me,
    ),
    components(schemas(
        auth::UserView,
        auth::LoginRequest,
        auth::LoginResponse,
        auth::MeResponse,
        setup::SetupStatus,
        setup::SetupRequest,
        setup::SetupResponse,
    )),
    tags(
        (name = "setup", description = "Configurazione iniziale dell'istanza"),
        (name = "auth", description = "Autenticazione e sessioni")
    )
)]
pub struct ApiDoc;

pub async fn serve() -> axum::Json<utoipa::openapi::OpenApi> {
    axum::Json(ApiDoc::openapi())
}
```

- [ ] **Step 6: Montare la rotta in `lib.rs`**

Aggiungere `pub mod openapi;` e, in **entrambi** `base_router` e
`base_router_stateless`, la rotta:

```rust
.route("/api/openapi.json", get(openapi::serve))
```

- [ ] **Step 7: Eseguire i test**

Run: `cargo test -p keeppix-api --test openapi`
Expected: PASS.

- [ ] **Step 8: Congelare la specifica per il controllo di compatibilità**

Aggiungere un piccolo test che scrive il file quando manca e lo confronta
quando esiste, in `crates/keeppix-api/tests/openapi.rs`:

```rust
/// Blocca la specifica su disco. Se cambia, il test fallisce e mostra il diff:
/// aggiornare `docs/api/openapi.json` è una scelta consapevole, non un effetto
/// collaterale di un refactoring.
#[test]
fn openapi_snapshot_matches_the_committed_file() {
    let generated = serde_json::to_string_pretty(
        &<keeppix_api::openapi::ApiDoc as utoipa::OpenApi>::openapi(),
    )
    .unwrap();

    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/api/openapi.json");

    if !path.exists() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &generated).unwrap();
        return;
    }

    let committed = std::fs::read_to_string(&path).unwrap();
    assert_eq!(
        committed.trim(),
        generated.trim(),
        "la specifica è cambiata: rigenerare con `rm docs/api/openapi.json && cargo test`"
    );
}
```

- [ ] **Step 9: Generare e verificare il file**

Run: `cargo test -p keeppix-api --test openapi && cargo test -p keeppix-api --test openapi`
Expected: la prima esecuzione crea `docs/api/openapi.json`, la seconda lo
trova identico.

- [ ] **Step 10: Commit**

```bash
git add crates/keeppix-api docs/api
git commit -m "feat(api): generate and freeze the openapi 3.1 document"
```

---

## Note di pre-volo del controller (vincolanti)

### N1 — La rotta va montata **dentro** `common_layers`, non dopo

Questo è l'errore facile del task, ed è la stessa classe di bug del ruling R5
del Task 9 (`.fallback()` dopo i `.layer()`, che lasciava i 404 senza header di
sicurezza).

`lib.rs` oggi ha questa forma:

```rust
fn base_router() -> Router<AppState> {
    common_layers(
        Router::new()
            .route("/health", get(routes::health::get))
            .nest("/api/v1", api_routes()),
    )
}

fn base_router_stateless() -> Router {
    common_layers(Router::new().route("/health", get(routes::health::get)))
}
```

`common_layers` applica i `.layer(...)` a ciò che gli viene passato. Una rotta
aggiunta **dopo** la chiamata — `common_layers(...).route("/api/openapi.json", ...)`
— non sarebbe avvolta dai layer e uscirebbe senza CSP, nosniff,
referrer-policy e permissions-policy. La rotta va aggiunta al `Router::new()`
**dentro** l'argomento di `common_layers`, in entrambe le funzioni.

**Aggiungi al test un'asserzione che lo inchioda**: la risposta di
`/api/openapi.json` deve portare gli stessi quattro header di sicurezza.
`crates/keeppix-api/tests/health.rs` ha già un helper condiviso
`assert_security_headers` — riusalo se è accessibile dal nuovo file di test,
altrimenti asserisci almeno `x-content-type-options` e
`content-security-policy` in `tests/openapi.rs`. E **verifica che l'asserzione
sia viva**: sposta temporaneamente la rotta fuori da `common_layers`, controlla
che il test diventi rosso, poi rimettila a posto. Riporta l'output reale nel
report.

### N2 — Step 8 del piano: ignora i due comandi shell

Il piano, prima del blocco di codice dello step 8, prescrive:

```bash
mkdir -p docs/api
cargo run --bin keeppix -- --help >/dev/null 2>&1 || true
cargo test -p keeppix-api --test openapi -- --nocapture >/dev/null
```

`cargo run --bin keeppix -- --help` non c'entra nulla con la generazione del
documento (è residuo di una stesura precedente) e `mkdir -p docs/api` è già
fatto dal test stesso. Salta tutti e tre: quello che conta è il test dello
step 8 e la doppia esecuzione dello step 9.

### N3 — `UserView.role` è `&'static str`

```rust
pub struct UserView {
    pub role: &'static str,
    ...
}
```

Se il derive `ToSchema` non gestisce `&'static str`, **non cambiare il tipo del
campo** (rompere il Task 10 per far contento un derive è il verso sbagliato):
annota il campo con `#[schema(value_type = String)]`. Stessa regola per
qualunque altro campo che il derive non digerisca — `value_type` prima di
toccare il modello.

Nota che quasi tutti i campi di queste struct sono **privati**
(`SetupStatus.initialised`, `LoginRequest.username`, …). È voluto e il derive
espande nello stesso modulo, quindi funziona; non cambiare le visibilità.

### N4 — Vincoli di stile del workspace già in vigore

- `cargo clippy --workspace --all-targets -- -D warnings` deve restare pulito.
  I lint di workspace hanno `unwrap_used` ed `expect_used` a `warn`, quindi si
  applicano anche ai test: la convenzione del repository è
  `#[allow(clippy::unwrap_used)]` sulla singola funzione di test (vedi
  `crates/keeppix-api/tests/auth.rs`), non un `allow` di file.
- `cargo fmt --check` deve restare pulito.
- Commenti e messaggi in italiano, come il resto del codebase. I nomi di test e
  di funzione in inglese.

### N5 — Contesto per lo snapshot

`docs/api/openapi.json` sarà confrontato in CI dal Task 15 con
`git diff --exit-code`, quindi il file **deve essere committato** e la seconda
esecuzione del test deve trovarlo identico byte per byte (a meno del `trim()`).
Verifica esplicitamente che `git status` sia pulito dopo aver eseguito la suite
completa una seconda volta: un test che riscrive il file a ogni esecuzione
farebbe fallire la CI.

Attenzione anche a `version = env!("CARGO_PKG_VERSION")`: oggi vale `0.1.0`.
È corretto così — un bump di versione **deve** far fallire lo snapshot, è il
punto del test.

### N6 — Cosa non fare

- Non toccare `common_layers` oltre all'aggiunta della rotta, e non riordinare
  `.fallback()` rispetto ai `.layer()` (ruling R5).
- Non cambiare la logica degli handler del Task 10: questo task è puramente
  additivo (annotazioni + una rotta).
- Non aggiungere `utoipa-swagger-ui` né altre UI: fuori dalla Fase 0.
- Non usare `SQLX_OFFLINE` né generare cache `.sqlx/` (ruling R4): non
  c'entrano con questo task ma sono citati altrove nel piano.

---

## Verifica finale attesa nel report

```
cargo test --workspace                                        # tutti verdi
cargo clippy --workspace --all-targets -- -D warnings         # pulito
cargo fmt --check                                             # pulito
git status                                                    # pulito dopo la suite
```

Docker è disponibile: i test di integrazione di `keeppix-api` e `keeppix-db`
avviano Postgres con testcontainers. Usa timeout generosi.

Nel report indica: cosa hai eseguito con quale esito (output reale, non
parafrasato), la prova red-then-green dell'asserzione sugli header di N1,
eventuali scostamenti dal piano con la motivazione, e i difetti minori che hai
notato ma deliberatamente non corretto.
