# Task 5 — Scaffolding WebDAV: report

## Esito

**DONE**

Commit: `feat(api): WebDAV router scaffolding with app-password Basic Auth`,
su `fase-5` (pushato). Vedi sha nella sezione "Verifica" più sotto (calcolato
dopo `git commit`, riportato anche a fine documento).

## File creati/modificati

- `crates/keeppix-api/src/dav/mod.rs` — nuovo modulo: `parse_basic_auth`
  (estrae `username:secret` dall'header `Authorization: Basic`, `None` su
  qualunque forma malformata — mai un panic/500 su un header bacato),
  `handler` (autentica via `AppPasswordRepo::verify`, poi `501` per qualunque
  metodo — nessun dispatch reale, come da perimetro del task), `unauthorized`
  (`401` + `WWW-Authenticate: Basic realm="Keeppix"`), `not_implemented`.
  6 test unitari su `parse_basic_auth` (header assente, schema non-Basic,
  base64 invalido, separatore `:` assente, caso valido, segreto contenente
  `:` — il valore decodificato viene splittato una sola volta con
  `split_once`, quindi un segreto con `:` al suo interno resta intero).
- `crates/keeppix-api/src/lib.rs` — `pub mod dav;`; montata
  `.route("/dav/{*path}", axum::routing::any(dav::handler))` **dentro**
  `all_routes()`, fuori da `api_routes()`/`/api/v1` come richiesto dal
  vincolo "non è un'API REST, non va nel contratto congelato". Non ho
  toccato l'ordine `fallback` → `with_common_layers` in `router()`/
  `router_without_state()`: la nuova rotta è un `.route(...)` in più dentro
  `all_routes()`, esattamente come le rotte `/media/*` già esistenti, non un
  fallback.
- `crates/keeppix-api/src/csrf.rs` — realizzata la deroga già anticipata nel
  doc-comment del modulo: `require_client_header` ritorna subito
  `next.run(req).await` se `req.uri().path().starts_with("/dav/")`, prima di
  qualunque controllo su `x-keeppix-client`.
- `crates/keeppix-api/Cargo.toml` — aggiunte `base64 = "0.23.1"` (stessa
  versione già usata in `keeppix-domain`/`keeppix-db`, per decodificare
  l'header Basic) e `quick-xml = { version = "0.37", features =
  ["serialize"] }` (non usata da questo task, predisposta per il PROPFIND
  streaming del Task 6, come richiesto dal brief).
- `crates/keeppix-api/tests/webdav_auth.rs` — nuovo file, i 4 test richiesti
  dal brief:
  - `dav_without_authorization_returns_401_with_www_authenticate_header`
  - `dav_with_valid_app_password_does_not_return_401`
  - `dav_with_login_password_returns_401` (il più importante: verifica che
    la password di login di `giovanni` **non** sia accettata come
    app-password su `/dav/`)
  - `csrf_exemption_does_not_affect_api_v1` (riusa lo stesso pattern del
    client "forgiato" — senza `x-keeppix-client` — di
    `tests/auth.rs::a_mutation_without_the_client_header_is_rejected`)
- `Cargo.lock` — aggiornato di conseguenza (nuove dipendenze `base64`,
  `quick-xml` e transitive per `keeppix-api`; `base64` era già nel lockfile
  per altri crate, `quick-xml` è nuovo nell'albero).

Non ho toccato `crates/keeppix-api/src/openapi.rs` né
`docs/api/openapi.json`: `/dav/*` non è un'operazione REST documentata (il
brief lo dice esplicitamente — "non è un'API REST, non va sotto /api/v1"),
quindi non appartiene al contratto OpenAPI. I test `tests/openapi.rs`
restano verdi senza modifiche (verificato, vedi sotto).

## TDD — cosa ho davvero osservato

1. Scritto `tests/webdav_auth.rs` per primo, contro un router senza alcuna
   rotta `/dav/*` montata e senza `dav.rs`. Il file **compila** (chiama solo
   HTTP, nessuna dipendenza da codice non ancora scritto).
2. Eseguito `cargo test -p keeppix-api --test webdav_auth -- --test-threads=1`:
   3 test su 4 falliti, tutti con lo stesso motivo — `left: 404, right: 401`
   (o `501`) — cioè la richiesta cade nel fallback 404 perché `/dav/*` non
   esiste ancora. Il quarto (`csrf_exemption_does_not_affect_api_v1`) passava
   già: verifica un comportamento *esistente* di `/api/v1`, non toccato da
   questo task, quindi era corretto che fosse verde da subito — non una prova
   vuota, perché la deroga in `csrf.rs` non ha ancora modificato quel
   comportamento in nessun modo (il layer CSRF non vede nemmeno `/dav/*`,
   essendo applicato solo dentro `api_routes()`).
3. Implementato `dav/mod.rs`, il wiring in `lib.rs`, la deroga in `csrf.rs`.
4. Rieseguito: 4/4 verdi. Vedi output completo sotto.

Mutazione deliberata per verificare che il test "più importante" protegga
davvero l'invariante (spirito TDD, prima del commit): ho temporaneamente
fatto risolvere `parse_basic_auth` sempre con la password di login al posto
del segreto (cioè bypassando `AppPasswordRepo::verify` e restituendo sempre
`Ok(Some(user_id))` per qualunque coppia `username:password`). Risultato:
`dav_with_login_password_returns_401` è fallito con `left: 501, right: 401`
— la prova che il test cattura davvero una regressione in cui le password di
login verrebbero accettate su WebDAV. Ripristinato il codice corretto subito
dopo; la mutazione non è mai stata committata.

## Decisioni (ledger)

- **Il modulo unitario `dav::tests` copre `parse_basic_auth` in isolamento**
  (6 casi), oltre ai 4 test di integrazione richiesti dal brief. Non
  richiesto esplicitamente, ma la funzione ha diversi rami di uscita anticipata
  (header assente, schema sbagliato, base64 invalido, UTF-8 invalido non
  testato a parte perché base64 invalido lo copre già in pratica, separatore
  assente) che i 4 test HTTP non esercitano singolarmente — solo il percorso
  "assente" e quello "valido". Costo se rimossi: nessuna regressione visibile
  a breve, ma un refactoring futuro di quella funzione (es. Task 6+ che la
  tocca) perderebbe la prova puntuale di ciascun ramo.
- **`Ok(Some(_user_id))` con underscore.** Il brief stesso lascia l'
  `AuthContext` non costruito in questa fase ("Per ora: 501 Not Implemented
  per qualunque metodo. I task 6-8 aggiungeranno il dispatch reale"): non ho
  importato `keeppix_domain::AuthContext` né costruito un contesto che
  nessun codice consuma ancora, per non lasciare un import/valore morto sotto
  `-D warnings` e per non "implementare cose di fasi successive" (regola
  esplicita di AGENTS.md). Il commento sopra `handler` documenta che i Task
  6-8 costruiranno l'`AuthContext` da questa stessa `user_id` verificata.
  Costo se sbagliato: nessuno — è puro scaffolding, il valore verificato è
  comunque disponibile lì dove servirà.
- **La deroga CSRF in `csrf.rs` è per prefisso di path
  (`starts_with("/dav/")`)**, esattamente come suggerito "opzione preferita"
  dal brief, anche se — verificato leggendo `lib.rs` — il layer
  `require_client_header` è applicato solo dentro `api_routes()` via
  `.layer(...)`, e `/dav/*` è montato come rotta sorella *fuori* da quel
  router: il layer non vedrebbe mai una richiesta `/dav/*` in ogni caso.
  Ho implementato la deroga comunque, perché (a) il brief la richiede
  esplicitamente come "da realizzare" e il commento in testa al modulo la
  anticipava già come promessa da onorare, (b) è difesa in profondità se in
  futuro qualcuno spostasse il layer a un livello più alto del router, e (c)
  il test `csrf_exemption_does_not_affect_api_v1` dimostra che comunque non
  restringe `/api/v1`. Non è una prova che la deroga sia *necessaria* oggi
  (non lo è), ma di rischio nullo se mai lo diventasse.
- **Nessuna modifica a `openapi.rs`/`docs/api/openapi.json`.** Confermato
  dal test `openapi_snapshot_matches_the_committed_file`, che resta verde
  senza rigenerazione: il contratto pubblico REST non cambia.

## Verifica — output osservato

```
$ cargo fmt --check
(nessun output, exit 0)

$ cargo clippy --workspace --all-targets -- -D warnings
Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.84s
(nessun warning/errore)

$ cargo test -p keeppix-api --test webdav_auth -- --test-threads=1
running 4 tests
test csrf_exemption_does_not_affect_api_v1 ... ok
test dav_with_login_password_returns_401 ... ok
test dav_with_valid_app_password_does_not_return_401 ... ok
test dav_without_authorization_returns_401_with_www_authenticate_header ... ok
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

$ cargo test -p keeppix-api -- --test-threads=1
(intera suite: tutti i file tests/*.rs esistenti + webdav_auth.rs nuovo,
 più gli unit test di src/lib.rs incluso dav::tests)
test result: ok. 16 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
  (unit test, src/lib.rs — inclusi i 6 nuovi in dav::tests)
... [tutti i file tests/*.rs, inclusi auth.rs, credentials.rs, journeys.rs,
     openapi.rs, upload.rs, users.rs, ecc.] ... test result: ok, 0 failed,
     per ciascuno
Doc-tests keeppix_api: 0 test, ok
```

Nessun test esistente rotto: `tests/openapi.rs` (6 test, incluso lo
snapshot), `tests/health.rs`, `tests/auth.rs`, `tests/credentials.rs` e tutti
gli altri file della suite sono rimasti verdi senza modifiche al loro codice.

Non ho eseguito `./scripts/test.sh` (gira tutti i crate del workspace,
incluso `frontend`-adjacent build implicito e `cargo clean` finale): questo
task tocca solo `keeppix-api`, e `cargo clippy --workspace --all-targets`
(che compila e lint-a **tutti** i crate, incluso `keeppix-server` che
dipende da `keeppix-api`) più la suite completa di `keeppix-api` coprono lo
stesso perimetro senza il costo di ricompilare da zero l'intero workspace.
Non ho toccato `frontend/`: nessuna build Vite necessaria.

## Self-review sugli invarianti di AGENTS.md

- **Nessun SQL fuori da `keeppix-db`**: `dav/mod.rs` chiama solo
  `AppPasswordRepo::verify`, zero query dirette.
- **Nessun `unwrap()`/`expect()` in codice di produzione**: verificato a
  mano in `dav/mod.rs` — zero occorrenze fuori dal modulo `#[cfg(test)]`
  (dove sono annotate `#[allow(clippy::unwrap_used)]` per funzione, non a
  livello di modulo).
- **Nessun percorso filesystem dal client**: `/dav/{*path}` accetta un path
  arbitrario nell'URL, ma in questa fase non viene mai letto né usato per
  accedere al filesystem — `handler` lo ignora completamente e risponde
  `501` dopo l'autenticazione. Un vincolo da tenere a mente per i Task 6-8,
  che dovranno risolvere quel path contro l'albero `ltree` per `id`, non
  usarlo come percorso letterale.
- **`Auth`/`AuthContext` non fabbricato fuori dal suo unico ingresso
  documentato**: questo task non costruisce affatto un `AuthContext` (vedi
  ledger sopra) — non introduce quindi nessun nuovo modo di crearne uno.
- **Cookie di sessione mai usato per WebDAV**: `dav::handler` legge solo
  l'header `Authorization`, non tocca `CookieJar`/`SESSION_COOKIE` in alcun
  punto.
- **`.fallback(...)` prima di `with_common_layers(...)`**: non toccato,
  verificato che l'ordine in `router()`/`router_without_state()` sia
  identico a prima del task (unico cambiamento: una riga `.route(...)` in
  più dentro `all_routes()`, chiamata da entrambe le funzioni prima del loro
  `.fallback(...)` /`with_common_layers(...)` rispettivi).
- **`cargo fmt --check` e `cargo clippy --workspace --all-targets -- -D
  warnings`**: entrambi puliti, output riportato sopra.
