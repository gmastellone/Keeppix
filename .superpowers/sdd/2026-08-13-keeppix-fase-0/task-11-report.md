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
