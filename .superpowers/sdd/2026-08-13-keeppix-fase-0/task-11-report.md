# Task 11 — Specifica OpenAPI — report di esecuzione

Commit: `9d88cb4 feat(api): generate and freeze the openapi 3.1 document` su `fase-0`
(HEAD precedente `4b5e354`). Albero pulito, nessun push.

## Cosa è stato implementato

- `crates/keeppix-api/Cargo.toml`: `utoipa 5.5.0` con feature `axum_extras,chrono,uuid`
  (aggiunta con `cargo add`, come da step 1). Nessun altra dipendenza, nessuna UI.
- `crates/keeppix-api/src/routes/auth.rs` e `.../setup.rs`: `utoipa::ToSchema` sui sette
  tipi pubblici e `#[utoipa::path(...)]` sui sei handler, con gli status del piano
  (login 200/401, refresh 204/401, logout 204, me 200/401, setup::status 200,
  setup::create 201/409/422). **Nessuna modifica alla logica degli handler**: il diff su
  questi due file è solo derive + attributi + un commento.
- `crates/keeppix-api/src/openapi.rs` (nuovo): `ApiDoc` e `serve()`, come da step 5.
- `crates/keeppix-api/src/lib.rs`: `pub mod openapi;` e la rotta `/api/openapi.json`
  aggiunta **dentro** l'argomento di `common_layers` in entrambi `base_router` e
  `base_router_stateless` (nota N1), con un commento che spiega perché non va spostata.
- `crates/keeppix-api/tests/openapi.rs` (nuovo): tre test — documento servito e completo,
  header di sicurezza sulla nuova rotta, snapshot su disco.
- `docs/api/openapi.json` (nuovo, 295 righe): generato dal test e committato.

### Scelte non ovvie

- **`UserView.role` resta `&'static str`** (nota N3): il derive `ToSchema` non digerisce
  `&'static str`, quindi il campo porta `#[schema(value_type = String)]` con un commento
  che spiega perché il tipo del campo non è stato toccato. Nessuna visibilità di campo è
  stata cambiata: i campi privati vengono espansi correttamente dal derive nello stesso
  modulo, come previsto dalla nota.
- **`assert_security_headers` duplicato** in `tests/openapi.rs` invece di riusato da
  `tests/health.rs`: ogni file in `tests/` è un binario a sé, quindi l'helper non è
  raggiungibile senza introdurre un modulo condiviso. La nota N1 prevedeva esplicitamente
  questo caso e chiedeva come minimo `x-content-type-options` e `content-security-policy`;
  la copia locale asserisce tutti e quattro gli header, non due, e il commento sopra la
  funzione dice perché è una copia.
- **Asserzione degli header in un test separato** (`openapi_document_carries_the_security_headers`)
  invece che dentro il test del piano: così il red-then-green di N1 distingue "la rotta
  non è avvolta dai layer" da "il documento è incompleto", e infatti nella prova sotto il
  primo test resta verde mentre il secondo diventa rosso.
- `//! Documento `OpenAPI` ...`: i backtick sul modulo doc sono richiesti da
  `clippy::doc_markdown` (pedantic), che ha bocciato la prima stesura.

## TDD — output reale

### Fallimento iniziale (step 3), dopo aver scritto solo il test

```
$ cargo test -p keeppix-api --test openapi
running 2 tests
test openapi_document_is_served_and_complete ... FAILED
test openapi_document_carries_the_security_headers ... FAILED

---- openapi_document_is_served_and_complete stdout ----
assertion `left == right` failed
  left: 404
 right: 200
---- openapi_document_carries_the_security_headers stdout ----
assertion `left == right` failed
  left: 404
 right: 200

test result: FAILED. 0 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.15s
```

404 su `/api/openapi.json`, esattamente come previsto dal piano.

### Verde dopo l'implementazione (step 7)

```
running 2 tests
test openapi_document_carries_the_security_headers ... ok
test openapi_document_is_served_and_complete ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

### Red-then-green obbligatorio della nota N1

Rotta spostata temporaneamente **fuori** da `common_layers` in `base_router_stateless`:

```rust
common_layers(Router::new().route("/health", get(routes::health::get)))
    .route("/api/openapi.json", get(openapi::serve))
```

```
$ cargo test -p keeppix-api --test openapi
running 2 tests
test openapi_document_is_served_and_complete ... ok
test openapi_document_carries_the_security_headers ... FAILED

---- openapi_document_carries_the_security_headers stdout ----

thread 'openapi_document_carries_the_security_headers' panicked at crates/keeppix-api/tests/openapi.rs:15:54:
called `Option::unwrap()` on a `None` value

test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.15s
```

Il punto è proprio la riga 15 dell'helper (`headers.get("x-content-type-options").unwrap()`):
con la rotta fuori dai layer il documento esce comunque 200 e completo — il primo test
resta verde — ma senza nessuno dei quattro header. Rotta rimessa dentro `common_layers`,
test di nuovo verde (vedi la suite completa sotto). L'asserzione è quindi viva e pinna
esattamente la classe di bug del ruling R5.

### Anche lo snapshot è stato verificato vivo

Non richiesto, ma l'avvertenza sulle asserzioni che non provano ciò che dichiarano vale
anche qui: cambiando `"Keeppix API"` in `"Keeppix APIX"` dentro `docs/api/openapi.json`,
`openapi_snapshot_matches_the_committed_file` fallisce con
`la specifica è cambiata: rigenerare con `rm docs/api/openapi.json && cargo test``.
File ripristinato subito dopo.

## Verifica finale

| Comando | Esito |
| --- | --- |
| `KEEPPIX_TEST_DATABASE_URL=... cargo test --workspace -- --test-threads=1` | **tutto verde**: 23 gruppi `test result: ok`, 0 falliti. In dettaglio: keeppix-api lib 4, auth 13, health 3, **openapi 3**, keeppix-db migrations 7 / sessions 14 / settings 6 / users 12, keeppix-domain 22, keeppix-server config 4, doc-test 0. |
| `cargo clippy --workspace --all-targets -- -D warnings` | pulito (`Finished dev profile ... in 13.64s`, nessun warning). Una sola violazione emersa in corsa — `clippy::doc_markdown` su `OpenAPI` senza backtick — corretta. |
| `cargo fmt --check` | pulito (dopo `cargo fmt`, che ha riformattato una chiamata in `tests/openapi.rs`). |
| `git status` dopo la **prima** suite completa | solo i file del task, nessuna riscrittura di `docs/api/openapi.json`. |
| `git status` dopo la **seconda** suite completa (post-commit) | **vuoto**. `git diff --exit-code -- docs/api` → nessuna differenza. |

La doppia esecuzione mirata dello step 9 (`cargo test -p keeppix-api --test openapi` due
volte di fila) ha dato prima creazione del file, poi confronto identico. Il documento è
deterministico: utoipa 5 usa mappe ordinate per `paths` e `components/schemas`, quindi
l'output non dipende dall'ordine di esecuzione.

Ambiente: testcontainers non è utilizzabile (pull delle immagini negato dalla policy di
egress), quindi la suite ha girato contro il PostgreSQL 16 locale via
`KEEPPIX_TEST_DATABASE_URL`, con `--test-threads=1` per i test di `keeppix-server/tests/config.rs`
che manipolano l'ambiente di processo. Nessun harness è stato modificato.

## Scostamenti dal piano

1. **Step 8, i tre comandi shell saltati** — come da nota N2: `mkdir -p docs/api` lo fa già
   il test, e `cargo run --bin keeppix -- --help` non c'entra con la generazione.
2. **Un test in più rispetto al piano** (`openapi_document_carries_the_security_headers`) —
   richiesto dalla nota N1; separato dal test del piano per la ragione spiegata sopra.
3. **`#[schema(value_type = String)]` su `UserView.role`** — nota N3, il tipo del campo non
   è stato toccato.
4. **`Cargo.lock` incluso nel commit** oltre a `crates/keeppix-api` e `docs/api`: l'aggiunta
   di utoipa lo modifica e lasciarlo fuori avrebbe lasciato l'albero sporco.
5. **Messaggio di commit esteso** con un corpo che documenta il vincolo di N1 sulla
   posizione della rotta; il titolo è quello previsto dallo step 10, verbatim.

## Difetti notati e deliberatamente non corretti (Minor differiti)

1. **I rustdoc `# Errors` finiscono nel contratto pubblico.** utoipa usa i doc comment
   degli handler come `summary`/`description`, quindi per esempio l'operazione `login`
   ha `"summary": "# Errors\n`401 invalid-credentials` per utente inesistente, ..."`.
   È il testo che un generatore di client riverserebbe nella documentazione del metodo:
   informativo ma malformato, con un heading Markdown fuori contesto. Si correggerebbe
   con `summary = ...` e `description = ...` espliciti in ogni `#[utoipa::path]`; non l'ho
   fatto perché il piano non lo chiede e cambierebbe lo snapshot di sei operazioni.
   I `# Errors` in sé non si possono togliere: `clippy::missing_errors_doc` (pedantic) li
   pretende.
2. **Le risposte d'errore non hanno schema.** 401/409/422 sono dichiarate senza
   `body`, e il tipo `Problem` (RFC 7807, `application/problem+json`) non compare tra i
   `components/schemas`. Un client generato non conoscerebbe la forma dell'errore che il
   server restituisce davvero. Aggiungere `ToSchema` a `Problem` e `body = Problem` alle
   risposte d'errore sarebbe additivo, ma è oltre gli step del piano.
3. **L'autenticazione a cookie non è descritta.** Il documento non ha `securitySchemes`
   né `security` sulle operazioni protette: da `/api/v1/auth/me` non si capisce che serve
   il cookie `__Host-keeppix_session`. Stessa motivazione della voce precedente.
4. **Il ramo "scrivi se manca" dello snapshot test passa a vuoto.** Su un checkout senza
   `docs/api/openapi.json` il test crea il file e ritorna verde senza confrontare nulla:
   di per sé non protegge la CI, che dipende dal `git diff --exit-code` del Task 15 per
   accorgersi del file rigenerato. È il design previsto dal piano e con il file committato
   il ramo non si attiva più, ma vale la pena saperlo.
5. **Il messaggio di fallimento dello snapshot non è un diff.** Il docstring promette
   "mostra il diff", mentre `assert_eq!` stampa due volte l'intero documento su una riga
   con gli escape: per un file di 295 righe è quasi illeggibile. Un confronto riga per riga
   (o `pretty_assertions`) renderebbe utile il fallimento; è cosmetico, quindi differito.
6. **`docs/api/openapi.json` non termina con newline.** È `std::fs::write(&path, &generated)`
   dal codice del piano; git segnala `\ No newline at end of file`. Aggiungere il newline a
   mano lo renderebbe incoerente con ciò che il test riscrive dopo un
   `rm docs/api/openapi.json && cargo test`, quindi ho lasciato il file esattamente come il
   test lo genera.

---

# Task 11 — fix round 1/5 — report

Commit: `adca7c6 fix(api): tie the openapi document to the routes it claims to describe`
su `fase-0` (HEAD precedente `990a512`). Albero pulito, nessun push.
Chiusi tutti e cinque gli Important della review; nessun Minor differito è stato
riaperto, con l'eccezione dichiarata sotto (m1).

## I1 — Documento e rotte montate non possono più divergere in silenzio

**Test nuovo: `documented_operations_are_all_mounted`** in
`crates/keeppix-api/tests/openapi.rs`. Avvia `TestServer` (Postgres reale,
`keeppix_api::router(state)` — il **router vero con stato**, non
`router_without_state()`), scarica `/api/openapi.json` dal server, e per ogni
coppia (path, method) letta dal documento esegue la richiesta corrispondente
asserendo che lo status non sia 404 (percorso inesistente) né 405 (metodo
sbagliato). Il file di test ha ora `mod harness;` come `tests/auth.rs`.

Due dettagli non ovvi:

- Le chiavi di un Path Item non sono tutte operazioni (`summary`, `parameters`,
  `servers`, …): il ciclo filtra sugli otto metodi HTTP tramite `HTTP_METHODS`.
- `assert_eq!(checked, 6)` chiude la vacuità: senza quel contatore un documento
  vuoto — o un `paths` che smette di essere un oggetto di operazioni — farebbe
  passare il test a ciclo mai eseguito. È esattamente il modo in cui questo test
  potrebbe mentire.

**Direzione non coperta, dichiarata nel commento del test** come da ruling:
rotta montata e non documentata. Servirebbe enumerare le rotte, e axum 0.8 non
espone la propria tabella (`Router` non ha API di introspezione). Il commento
dice che il controllo va fatto in review o in CI quando axum lo renderà
leggibile — nessuna finzione di copertura.

Ho anche corretto il commento di `fn app()` in cima al file: dice ora
esplicitamente che `router_without_state()` **non monta `/api/v1`**, la trappola
che aveva reso ingannevole il test dei percorsi.

**Messaggio dello snapshot riscritto.** Prima:
`"la specifica è cambiata: rigenerare con `rm docs/api/openapi.json && cargo test`"`
— cioè le istruzioni per disattivare il controllo. Ora dice che il contratto
pubblico è cambiato, che da quel file si generano i client Kotlin/Swift/Dart/TS,
che lo spec lo dichiara congelato, e — testualmente — «Non rigenerarlo per far
tornare verde il test: guarda che cosa è cambiato e decidi»; la rigenerazione è
presentata come decisione condizionata al fatto che il cambiamento sia voluto e
compatibile, da motivare nel commit. Nessun comando pronto da copiare.

## I2 — `responses` allineate a ciò che gli handler possono restituire

| Operazione | Prima | Ora | Perché |
| --- | --- | --- | --- |
| `setup_status` | 200 | 200, **500** | `count()` → `Problem::from(DbError)` |
| `setup_create` | 201, 409, 422 | 201, 409, 422, **500** | `Problem::internal()` sull'hashing + `DbError` non-Conflict |
| `auth_login` | 200, 401 | 200, 401, **500** | `find_by_username` e `SessionRepo::create` propagano `?` |
| `auth_refresh` | 204, 401 | 204, 401 (invariato) | ogni errore di `rotate` è mappato su 401 via `map_err`: **non** c'è 500 da dichiarare, e ora un commento nel codice lo dice |
| `auth_logout` | 204 | 204 (invariato) | l'errore di revoca è loggato, non restituito |
| `auth_me` | 200, 401 | 200, 401, **404**, **500** | `find_by_id` → `DbError::NotFound` → 404, altri `DbError` → 500 |

`me` può in teoria produrre anche 403 (`DbError::Forbidden`), ma solo se
`ctx.user_id() != id`: l'id **viene** da `ctx.user_id()`, quindi il ramo è
irraggiungibile e dichiararlo descriverebbe un comportamento inesistente. Non
l'ho aggiunto; lo segnalo perché è una scelta, non una svista.

## I3 — `operation_id` espliciti

`setup_status`, `setup_create`, `auth_login`, `auth_refresh`, `auth_logout`,
`auth_me`. Nuovo test `operation_ids_are_explicit_and_unique`: raccoglie gli id
dal documento, verifica che non ci siano duplicati e li confronta con l'elenco
atteso, così un `albums::create` futuro non può reintrodurre la collisione senza
che qualcuno se ne accorga.

## I4 — `Problem` nei components e come corpo degli errori

`#[derive(utoipa::ToSchema)]` su `Problem` (`crates/keeppix-api/src/problem.rs`)
e `body = Problem` su tutte e otto le risposte d'errore. Lo schema generato è
esattamente ciò che va sul filo:

```json
"Problem": { "required": ["type","title","status"],
  "properties": { "type": {"type":"string","example":"keeppix/unauthenticated"},
                  "title": {"type":"string"}, "status": {"type":"integer","format":"int32","minimum":0},
                  "detail": {"type":["string","null"]} } }
```

Il campo privato `status_code: StatusCode` non compare: utoipa rispetta
`#[serde(skip)]`, quindi non è servito nessun `#[schema(ignore)]`. `type_slug`
compare come `type` grazie a `#[serde(rename)]`, sempre letto dal derive.
L'`example` sul campo `type` è l'unica aggiunta non richiesta: rende evidente al
generatore che il campo porta gli slug stabili su cui §9.2 dice che il client
ramifica.

## I5 — Schema di sicurezza a cookie

`SecurityAddon` (un `utoipa::Modify`) registra lo schema
`apiKey` / `in: cookie` con il nome preso da `crate::extract::SESSION_COOKIE` —
il documento riporta `"name": "__Host-kpx_session"`, che è il cookie reale (la
review lo chiamava `__Host-keeppix_session`: prendere la costante invece di
riscriverla ha evitato l'errore). `security(("session_cookie" = []))` su
`auth_me` e `auth_refresh`; `auth_logout` resta pubblica di proposito, con un
commento che spiega perché (funziona anche senza cookie, 204 in ogni caso).

Il nome dello schema è un letterale dentro le macro e non può riferirsi a
`openapi::SESSION_SCHEME`: il nuovo test
`security_requirements_name_a_declared_scheme` verifica che ogni requisito punti
a uno schema dichiarato in `components`, che le rotte protette siano esattamente
`me` e `refresh`, e che il cookie descritto sia quello che l'extractor legge.

## Prove che i nuovi test sono vivi (mutazione → rosso → ripristino)

Quattro mutazioni, una alla volta, ognuna ripristinata subito dopo. Output
reale:

**M1 — `post` → `put` nel solo `#[utoipa::path]` di `login`** (la prova B della
review, quella che prima tornava verde dopo la rigenerazione):

```
test documented_operations_are_all_mounted ... FAILED
thread '...' panicked at crates/keeppix-api/tests/openapi.rs:147:13:
assertion `left != right` failed: il documento dichiara put /api/v1/auth/login, ma la rotta non accetta quel metodo
  left: 405
 right: 405
```

**M2 — `path = "/api/v1/auth/me"` → `"/api/v1/auth/whoami"`** (rotta invariata):

```
test documented_operations_are_all_mounted ... FAILED
assertion `left != right` failed: il documento dichiara get /api/v1/auth/whoami, ma quel percorso non è montato
  left: 404
 right: 404
```

Da notare: questa mutazione ora è intercettata dal **router**, non più solo
dalla lista di percorsi scritta a mano nel test.

**M3 — nome dello schema divergente** (`security(("session" = []))` in `me`,
addon invariato):

```
test security_requirements_name_a_declared_scheme ... FAILED
get /api/v1/auth/me richiede lo schema session, che non è dichiarato in components
```

**M4 — `operation_id` collidente** (`setup_status` → `setup_create`):

```
test operation_ids_are_explicit_and_unique ... FAILED
assertion `left == right` failed: operationId duplicato: ["auth_login", "auth_logout", "auth_me", "auth_refresh", "setup_create"]
  left: 5
 right: 6
```

## Esito dei comandi

| Comando | Esito |
| --- | --- |
| `cargo test -p keeppix-api --test openapi` (file toccato: `tests/openapi.rs`) | **6/6 ok**: `openapi_document_is_served_and_complete`, `openapi_document_carries_the_security_headers`, `documented_operations_are_all_mounted`, `security_requirements_name_a_declared_scheme`, `operation_ids_are_explicit_and_unique`, `openapi_snapshot_matches_the_committed_file` |
| `cargo test -p keeppix-api` (copre anche `problem.rs`, `routes/auth.rs`, `routes/setup.rs`, `openapi.rs`) | lib 4/4, `tests/auth.rs` 13/13, `tests/health.rs` 3/3, `tests/openapi.rs` 6/6 |
| `cargo test --workspace -- --test-threads=1` | **23 gruppi `test result: ok`, 0 falliti**, eseguito due volte dopo il commit |
| `cargo clippy --workspace --all-targets -- -D warnings` | pulito. Una violazione emersa e corretta in corsa: `clippy::doc_markdown` su «Path Item Object di OpenAPI 3.1» nel commento di `HTTP_METHODS` |
| `cargo fmt --check` | pulito |
| `git status --porcelain` dopo ognuna delle due suite | **0 righe** |
| `git diff --exit-code -- docs/api` | nessuna differenza: lo snapshot non si riscrive da solo |

Postgres locale era spento all'inizio del round (`no response`): riavviato con
`pg_ctlcluster 16 main start` (aveva lasciato un pid file stantio). I test di
`keeppix-api` che toccano il DB ora includono anche `documented_operations_are_all_mounted`.

## Snapshot rigenerato di proposito

`docs/api/openapi.json` passa da 295 a 428 righe. Il documento committato ora è:

```
POST  /api/v1/auth/login       auth_login    [200, 401, 500]        sec=[]
POST  /api/v1/auth/logout      auth_logout   [204]                  sec=[]
GET   /api/v1/auth/me          auth_me       [200, 401, 404, 500]   sec=[session_cookie]
POST  /api/v1/auth/refresh     auth_refresh  [204, 401]             sec=[session_cookie]
POST  /api/v1/setup            setup_create  [201, 409, 422, 500]   sec=[]
GET   /api/v1/setup/status     setup_status  [200, 500]             sec=[]
schemas: LoginRequest, LoginResponse, MeResponse, Problem, SetupRequest, SetupResponse, SetupStatus, UserView
securitySchemes: {"session_cookie": {"type":"apiKey","in":"cookie","name":"__Host-kpx_session", …}}
```

Le modifiche al contratto sono tutte additive tranne il rename degli
`operationId`, che è il punto di I3: si fa adesso perché nessun client è ancora
generato.

## Scostamenti e decisioni da mettere a verbale

1. **m1 toccato, ma non come lo propone la review.** Chiudendo I4 il blocco
   `components(schemas(...))` andava comunque guardato: ho aggiunto
   `crate::problem::Problem` all'elenco **e verificato che sia ridondante** —
   togliendo quella voce il documento resta identico byte per byte, perché
   `body = Problem` basta a far raccogliere lo schema (`openapi_snapshot_matches_the_committed_file`
   passa senza di essa). Ho tenuto il blocco (m1 resta differito) aggiungendoci
   il commento che la review stessa proponeva come alternativa: dice che è un
   indice leggibile, non configurazione, e che elencare lì un tipo non
   referenziato da nessuna operazione **non** lo fa comparire — la trappola che
   la review indicava.
2. **403 non dichiarato su `me`**: ramo irraggiungibile, vedi I2.
3. **`auth_logout` senza `security(...)`**: è pubblica per costruzione.
4. **Incidente di percorso, senza conseguenze sul risultato.** Durante le prove
   di mutazione un mio helper di shell faceva `git checkout -- crates/keeppix-api/src`
   per ripristinare il file mutato: ha cancellato **tutte** le modifiche ai
   sorgenti di questo round (i test e lo snapshot, fuori da `src/`, sono
   sopravvissuti). Le ho riscritte e la prova che il ripristino è esatto è
   oggettiva: `docs/api/openapi.json` non è stato più toccato e
   `openapi_snapshot_matches_the_committed_file` passa, quindi i sorgenti
   riprodotti generano lo stesso documento byte per byte. Le mutazioni
   successive (M2-M4) usano backup con `cp`, non `git checkout`.

## Difetti noti non toccati in questo round

Restano differiti e confermati: m2 (nessun test confronta il documento *servito*
con quello *committato* — nota però che `documented_operations_are_all_mounted`
ora **legge** il documento servito dal server reale, quindi un `serve()` che
inventasse percorsi verrebbe intercettato da lì se quei percorsi non fossero
montati), m3 (nessun `enum` su `role`, nessun `format: uuid` su `id`, nessun
`servers`), m4 (`info.version` è la versione del crate), e i sei difetti già
dichiarati nel report precedente — fra cui il più fastidioso resta il n. 1, i
rustdoc `# Errors` pubblicati come `summary`: ora che le `responses` sono
complete quei summary sono anche **ridondanti**, il che rende la correzione più
attraente di prima, ma resta fuori dal perimetro di questo round.

Nessuna preoccupazione aperta sul codice consegnato.
