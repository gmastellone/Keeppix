# Task 13 — Review: Frontend incorporato nel binario

**Diff:** `9c6d380..d6c74d6` (unico commit `d6c74d6`)
**Albero al termine della review:** identico a `d6c74d6` (`git status --short` vuoto, nessun commit creato)

## Spec Compliance

✅ Spec compliant, con due riserve importanti sulla copertura di test (vedi Critical/Important).

- `crates/keeppix-server/src/embed.rs`, `tests/embed.rs`, `lib.rs`, `main.rs`, `Cargo.toml` — tutti creati/modificati come richiesto dal brief (file list rispettata).
- `crates/keeppix-api/src/lib.rs` modificato come da preflight P1 (non secondo il codice letterale del brief, che il preflight dichiara vincolante in caso di contrasto).
- Comportamento richiesto verificato dal vivo:
  - `GET /` → `index.html` reale con `<script type="module" ... src="/assets/index-....js">` incorporato (verificato leggendo `frontend/dist/index.html` dopo `npm run build`, e tramite il test `index_is_served_at_root` che assert-a `html.contains("<script")`). Non è un embed vuoto degradato a 404/200-vuoto (P4 preflight).
  - `/assets/*` → `Cache-Control: public, max-age=31536000, immutable`; `index.html` → `no-cache`. Confermato da `assets_are_served_as_immutable` e `index_is_served_at_root`, eseguiti realmente (vedi sotto).
  - `/api/*` non ricade mai nel fallback SPA: `serve()` in `embed.rs:29-31` intercetta `path.starts_with("api/")` e risponde `Problem::not_found()` prima di guardare gli asset. Confermato da `api_paths_never_fall_back_to_index` (embed.rs) e da `unknown_api_path_returns_problem_json` (keeppix-api/tests/health.rs, preesistente, rimasto verde) — P2 preflight soddisfatto, il test esiste ed è vivo (vedi mutazioni sotto).
- `crates/keeppix-api/tests/harness/mod.rs` non modificato: la firma di `router(state)` non è cambiata, come richiesto da P5 preflight.
- File fuori perimetro (`routes/`, `cookie.rs`, `extract.rs`, `problem.rs`, `openapi.rs`) non toccati — confermato dal diff-stat (P5 preflight).
- `embed::spa_fallback() -> MethodRouter` della sezione "Produces" del brief: **confermo la lettura dell'implementer, non è un requisito reale.** `grep -rn "spa_fallback"` su tutto il repo (esclusi i file SDD) non trova nessun riferimento in `.rs`; il codice d'esempio dello stesso step 4 del brief non la definisce; nessun task successivo del piano la consuma (`grep -n "spa_fallback\|embed::" docs/superpowers/plans/2026-08-13-keeppix-fase-0.md` mostra solo la riga "Produces" e gli usi reali di `embed::mount`/`embed::mount_stateless`). È un refuso della riga riassuntiva, non un'interfaccia mancante.

⚠️ Non verificabile solo dal diff: la sostenibilità a lungo termine dell'invariante "fallback prima dei layer" dipende dalla disciplina dei futuri modificatori — il preflight lo riconosce esplicitamente ("non ti prescrivo la forma"). Il codice sceglie documentazione forte + mutazione dimostrata invece di un vincolo di tipo; è una scelta architetturale ragionevole data l'API di axum 0.8, ma **due dei quattro punti di montaggio non hanno alcun test che la protegga** (vedi Critical/Important).

## Test — verifica indipendente eseguita

Ambiente: Postgres attivo su 127.0.0.1:5432, disco con 14 GB liberi (nessun accumulo di database residui in questa sessione). Frontend ricompilato con `cd frontend && npm run build` prima di ogni run.

```
cargo test -p keeppix-server --test embed -- --test-threads=1 --nocapture
running 4 tests
test api_paths_never_fall_back_to_index ... ok
test assets_are_served_as_immutable ... ok
test client_routes_fall_back_to_index ... ok
test index_is_served_at_root ... ok
```
Nessun messaggio "frontend/dist assente: test saltato" — i 4 test di `embed.rs` girano davvero (P3 preflight), non sono saltati.

### Prova per mutazione, rifatta indipendentemente su ciascuno dei 4 punti di montaggio

Per ciascun punto ho invertito l'ordine fallback/layer con `Edit`, eseguito i test rilevanti, e ripristinato con `Edit` (mai `git checkout`). `git status --short` è vuoto al termine di ogni verifica intermedia e alla fine della review.

1. **`keeppix_api::router_without_state()`** (`crates/keeppix-api/src/lib.rs:36-43`) — mutato a `.fallback(not_found)` dopo `with_common_layers`.
   `cargo test -p keeppix-api --test health --test openapi -- --test-threads=1` → **ROSSO**: `unknown_api_path_returns_problem_json` fallisce (panic su `headers.get("x-content-type-options").unwrap()` = `None`). Ripristinato, verde di nuovo. **Copertura corretta.**

2. **`keeppix_api::router(state)`** (`crates/keeppix-api/src/lib.rs:31-33`) — mutato allo stesso modo.
   `cargo test -p keeppix-api --test auth --test openapi -- --test-threads=1` → **16/16 auth.rs verdi, 6/6 openapi.rs verdi. Nessun test diventa rosso.** Ripristinato. **Buco di copertura** — vedi Important #2.

3. **`keeppix_server::embed::mount_stateless()`** (`crates/keeppix-server/src/embed.rs`) — mutato a `with_common_layers(Router::new()).fallback(get(serve))`.
   `cargo test -p keeppix-server --test embed -- --test-threads=1` → **ROSSO**: `index_is_served_at_root` e `client_routes_fall_back_to_index` falliscono (stesso panic, header assenti). Ripristinato, verde di nuovo. **Copertura corretta.**

4. **`keeppix_server::embed::mount(router)`** (`crates/keeppix-server/src/embed.rs`) — la funzione usata da `main.rs` in produzione — mutata allo stesso modo, **lasciando `mount_stateless()` intonso**.
   `cargo test --workspace -- --test-threads=1` → **tutta la suite del workspace resta verde** (config 4/4, embed 4/4, doctest inclusi). Nessun test chiama `embed::mount()`: `grep -rn "embed::mount\b" crates/` trova solo la definizione e l'uso in `main.rs:62`. Ripristinato. **Buco di copertura, il più grave dei quattro** — vedi Critical #1.

Al termine, `git diff --stat` e `git status --short` sono vuoti: l'albero è tornato identico a `d6c74d6`.

### Controlli aggiuntivi eseguiti
- `cargo clippy -p keeppix-server -p keeppix-api --all-targets -- -D warnings` → pulito, conferma la dichiarazione del report.
- `cargo fmt --check -p keeppix-server -p keeppix-api` → pulito.
- Lettura diretta di `frontend/dist/index.html` dopo la build: contiene `<script type="module" crossorigin src="/assets/index-....js">` e il link al CSS con hash — l'embed non è vuoto (P4).

## Strengths

- La diagnosi della deviazione (report + commenti nel codice) è precisa e verificabile: ho controllato personalmente in axum 0.8.9 lo stesso meccanismo che l'implementer descrive (fallback sovrascritto, layer che avvolgono solo il fallback presente al momento della chiamata) tramite la prova per mutazione, e il comportamento osservato conferma esattamente la sua spiegazione.
- I commenti su `with_common_layers`, `mount` e `mount_stateless` (`keeppix-api/src/lib.rs:53-63`, `keeppix-server/src/embed.rs:69-79`) sono espliciti sull'invariante e dicono "non riordinare" con la motivazione, non solo l'istruzione — buona difesa documentale per un futuro modificatore.
- Le due asserzioni P1-punto-2 richieste dal preflight (`GET /` e un percorso client-side con gli header di sicurezza) sono state aggiunte esattamente come richiesto, e sono dimostrabilmente vive (vedi mutazione #3).
- `serve()` include difesa in profondità sui percorsi `/api/*` anche se irraggiungibile nel binario reale, con commento che ne spiega il perché — scelta difensiva ragionevole, non over-engineering: costa tre righe ed è testata.
- La lettura di `spa_fallback()` come refuso del brief è corretta e ben verificata (vedi Spec Compliance).
- Test manuale del binario reale (step 7, curl su `/`, `/api/v1/nope`, `/health`) copre la classe di rischio più diretta, anche se non sostituisce copertura automatica (vedi Critical #1).

## Issues

### Critical (Must Fix)

**C1 — `embed::mount()`, la funzione usata realmente da `main.rs`, non ha alcuna copertura di test automatica.**
`crates/keeppix-server/src/embed.rs:80-82`, usata in `crates/keeppix-server/src/main.rs:62`. Tutti i 5 test di `tests/embed.rs` chiamano `embed::mount_stateless()`, mai `embed::mount()`. Ho invertito personalmente l'ordine fallback/layer in `mount()` (lasciando `mount_stateless()` intatto) e rieseguito `cargo test --workspace -- --test-threads=1`: **la suite intera resta verde**. Questo è esattamente l'invariante centrale del task — l'`index.html` che carica l'intera applicazione servito senza CSP in produzione — e nel binario reale nessun test automatico lo protegge; solo la verifica manuale via `curl` nel report dell'implementer (non eseguita in CI, non riproducibile automaticamente). Poiché `mount()` e `mount_stateless()` sono due corpi di funzione scritti separatamente (non factored attraverso un'unica implementazione parametrica), un futuro refactor di `mount()` da solo può reintrodurre il bug senza che nessun test se ne accorga.
**Fix suggerito:** far condividere a `mount()` e `mount_stateless()` la stessa implementazione (es. `mount_stateless()` come `mount(Router::new())` con adattamento di tipo, oppure un helper interno comune), così un solo punto di mutazione copre entrambi; oppure aggiungere ad `embed.rs` un test che costruisca `AppState` (anche minimale, senza toccare il DB se possibile) e chiami `embed::mount(router)` direttamente, verificando gli header sul fallback.

### Important (Should Fix)

**I1 — `keeppix_api::router(state)` non ha alcuna copertura sugli header di sicurezza; la mutazione non fa fallire nulla.**
`crates/keeppix-api/src/lib.rs:31-33`. È uno dei quattro punti di montaggio che l'invariante del preflight P1 nomina esplicitamente da riverificare. Ho invertito l'ordine fallback/layer in questa sola funzione e rieseguito `cargo test -p keeppix-api --test auth --test openapi -- --test-threads=1`: **16/16 + 6/6 verdi**. `router(state)` è usato solo da `crates/keeppix-api/tests/harness/mod.rs:35` (`TestServer::start()`), a sua volta usato da `auth.rs` (16 test) e da `documented_operations_are_all_mounted` in `openapi.rs`; nessuno di questi controlla gli header di sicurezza — solo `router_without_state()` è coperto (da `health.rs` e `openapi.rs::openapi_document_carries_the_security_headers`). Severità Important e non Critical perché `router(state)` non entra mai nel binario spedito (il suo stesso doc-comment lo dichiara: "montato dai test"), ma resta un buco reale sull'invariante che il task doveva blindare, ed è il tipo di funzione che un domani potrebbe essere promossa a uso di produzione senza che nessuno se ne accorga.
**Fix suggerito:** aggiungere in `crates/keeppix-api/tests/harness/mod.rs` o in un nuovo test che usa `TestServer` un'asserzione sugli header di sicurezza su una risposta reale (es. `GET /health` attraverso `TestServer`), così la stessa garanzia di `health.rs` copre anche il router con stato.

### Minor (Nice to Have)

**M1 — Triplicazione dell'helper `assert_security_headers`.**
Presente identico (stesso corpo di 8 righe) in `crates/keeppix-api/tests/health.rs`, `crates/keeppix-api/tests/openapi.rs` (preesistente, non introdotto da questo diff) e ora anche in `crates/keeppix-server/tests/embed.rs:26-34` (nuovo). È coerente con la convenzione già presente nel repo (i crate non condividono codice di test, commentato esplicitamente in tutti e tre i file), quindi non è un difetto introdotto ora, ma la terza copia aumenta il costo di mantenerle allineate se i quattro header o i loro valori cambiassero.

**M2 — `router_parts()` è ora un alias a una riga di `all_routes()`.**
`crates/keeppix-api/src/lib.rs:49-51`. Necessario perché `all_routes()` è privata e `main.rs` deve poterla chiamare da un altro crate — non è un problema, ma vale la pena osservare che il nome "router_parts" (plurale, suggerisce più pezzi assemblati) descrive meno bene la nuova funzione ("le rotte, senza layer né fallback") di quanto avrebbe fatto un nome tipo `routes()`. Il brief impone letteralmente questo nome nell'interfaccia `main.rs`, quindi non è un'azione richiesta — solo un'osservazione per un futuro rename se mai si rompesse la compatibilità con quella riga del brief.

## Assessment

**Task quality:** Needs fixes (Critical C1 va risolto prima di considerare l'invariante di sicurezza effettivamente blindata dai test; I1 dovrebbe essere risolto nella stessa passata).

**Reasoning:** L'implementazione risolve correttamente, nel codice e nel comportamento a runtime osservato (manuale e su 2 dei 4 punti di montaggio), esattamente il difetto che il preflight temeva — l'ho riprodotto e verificato io stesso per mutazione su tutti e quattro i punti di montaggio indicati nel task. Ma la rete di test che dovrebbe rendere quell'invarianza permanente ha due buchi concreti e dimostrati: la funzione realmente usata dal binario (`embed::mount`) non è testata affatto, e `router(state)` non è testato sugli header. Senza correggerli, un futuro refactor può far ripresentare esattamente il bug del Task 9/R5 — questa volta sull'`index.html` di produzione — senza che `cargo test --workspace` se ne accorga.
