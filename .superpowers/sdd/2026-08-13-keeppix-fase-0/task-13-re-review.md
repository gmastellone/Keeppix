# Task 13 — Re-review del fix round 1 — Frontend incorporato nel binario

**Diff verificato:** `d6c74d6..e1f72b3` (`review-d6c74d6..e1f72b3.diff`)
**Albero al termine della re-review:** codice identico a `e1f72b3` (`git diff --stat crates/` vuoto). HEAD è avanzato a `8f86efd` per un commit di documentazione fatto da un altro processo durante la sessione (non mio, non tocca `crates/`).

## Finding Verdicts

**Critical — `embed::mount()` senza copertura di test (index.html senza CSP in produzione)** — **ADDRESSED**.

`crates/keeppix-server/src/embed.rs:85-96`. `mount` è ora generica su `S: Clone + Send + Sync + 'static'`, e `mount_stateless()` è letteralmente `mount(axum::Router::new())` — non un secondo corpo di funzione. Confermato leggendo il file: nessuna implementazione duplicata resta da nessuna parte (vedi punto 2 sotto).

Prova per mutazione rifatta io stesso, sulla `mount()` reale (quella che `main.rs:62` usa in produzione), non su `mount_stateless()`:
```rust
pub fn mount<S: Clone + Send + Sync + 'static>(router: axum::Router<S>) -> axum::Router<S> {
    keeppix_api::with_common_layers(router).fallback(get(serve))   // ordine invertito
}
```
`cargo test --workspace -- --test-threads=1` → **ROSSO**: `keeppix-server --test embed` termina con `error: test failed`, `client_routes_fall_back_to_index` e `index_is_served_at_root` falliscono (`headers.get("x-content-type-options").unwrap()` = `None`), 2 passed / 2 failed. La suite non resta più verde con l'inversione — a differenza di prima del fix, dove l'intera `cargo test --workspace` restava verde. Ripristinato con `Edit` (`git diff --stat crates/` tornato vuoto).

**Important — `router(state)` in `keeppix-api/src/lib.rs` senza copertura sugli header** — **ADDRESSED**.

`crates/keeppix-api/tests/auth.rs:34-58` aggiunge `router_with_state_carries_the_security_headers`, che passa da `TestServer` (quindi da `keeppix_api::router(state)`) e verifica i quattro header sia su `/api/v1/setup/status` (200) sia sul fallback 404 (`/api/v1/questa-rotta-non-esiste`). L'helper `assert_security_headers` è stato spostato in `crates/keeppix-api/tests/harness/mod.rs:64-73`, condiviso da `auth.rs` e ora anche da `openapi.rs`.

Prova per mutazione rifatta io stesso su `router(state)`:
```rust
pub fn router(state: AppState) -> Router {
    with_common_layers(all_routes()).fallback(not_found).with_state(state)   // ordine invertito
}
```
`cargo test -p keeppix-api --test auth --test openapi -- --test-threads=1` → **ROSSO**: `router_with_state_carries_the_security_headers` fallisce (`headers.get("x-content-type-options").unwrap()` = `None`), 16 passed / 1 failed. Prima del fix nessun test si accorgeva di questa inversione (16/16 + 6/6 verdi); ora il buco è chiuso. Ripristinato con `Edit`.

Entrambe le mutazioni sono state applicate e ripristinate solo su `crates/keeppix-server/src/embed.rs` e `crates/keeppix-api/src/lib.rs`, come richiesto; nessun altro file toccato durante la verifica.

## Genericità di `mount<S>` — verifica del punto 2

Confermato che non resta alcuna seconda implementazione dell'invariante:
- `grep -n "with_common_layers(.*)\.fallback" crates/` trova **una sola** occorrenza, in `keeppix-api/src/lib.rs:32` (`router(state)`), che è il pattern corretto per quel punto di montaggio (fallback impostato **dentro** l'argomento di `with_common_layers`, non concatenato dopo). Non c'è nessun residuo del pattern sbagliato (`with_common_layers(...).fallback(...)`) in nessun file.
- `grep -n "embed::mount\b" crates/` trova solo la definizione in `embed.rs` e l'uso reale in `main.rs:62`.
- `embed.rs:94-96`: `mount_stateless()` è `mount(axum::Router::new())`, corpo di una riga, nessuna logica propria.

Il fix chiude davvero il buco: un solo punto di mutazione (`mount<S>`) copre sia il binario reale sia i 4 test di `tests/embed.rs`.

## Verifica ambiente — build frontend e guardia dei test

`cd frontend && npm run build` eseguito prima di ogni verifica (bundle rigenerato con successo, hash nuovi negli asset). Confermato che i test non sono stati auto-saltati:
```
cargo test -p keeppix-server --test embed -p keeppix-api --test auth -- --test-threads=1 --nocapture
```
→ `auth.rs`: 17/17 (incluso il nuovo test), nessun messaggio di skip.
→ `embed.rs`: 4/4, nessun messaggio "frontend/dist assente: test saltato" — i test hanno esercitato l'embed reale.

Ambiente: Postgres attivo su 127.0.0.1:5432 (`pg_lsclusters` → online), disco con 11 GB liberi su `/` prima e dopo le verifiche — nessun segnale di accumulo di database di test residui, nessuna anomalia da segnalare su `keeppix-db`.

## New Breakage in the Fix Diff

**Nessuna rottura nuova.** In particolare, sul cambio a `crates/keeppix-api/tests/openapi.rs` (fuori dall'elenco esplicito dei finding):

Il diff rimuove la copia locale di `assert_security_headers` (13 righe) e fa usare a `openapi.rs` la stessa funzione condivisa in `harness/mod.rs`, dichiarando `mod harness;` (già presente prima) e importando `assert_security_headers` insieme a `TestServer`. Verificato:
- I tipi sono compatibili: `harness::assert_security_headers` prende `&reqwest::header::HeaderMap`; in `openapi.rs` è chiamata su `response.headers()` dove `response` è un `axum::http::Response` — un solo crate `http` (v1.5.0) nel lockfile (`grep -A2 '^name = "http"$' Cargo.lock`), quindi i tipi sono davvero interscambiabili, non solo "quasi uguali".
- `cargo test -p keeppix-api --test openapi -- --test-threads=1` → 6/6 verdi (invariato).
- `cargo clippy -p keeppix-api -p keeppix-server --all-targets -- -D warnings` → pulito.
- Motivazione dichiarata (evitare `dead_code` sulla `assert_security_headers` di `harness/mod.rs` nel binario `openapi.rs`, che dichiara `mod harness;` ma prima non la chiamava) è plausibile e coerente con come cargo compila ogni file `tests/*.rs` come binario/crate separato: un item `pub` inutilizzato in un modulo condiviso genera `dead_code` per-binario. La modifica è minimale (18 righe, quasi tutta rimozione), non cambia alcuna asserzione di test esistente, non introduce comportamento nuovo, ed è strettamente necessaria per chiudere I1 senza un `#[allow(dead_code)]` di comodo. Non la giudico refactoring autonomo travestito da necessità: è la conseguenza diretta e minima dello spostamento dell'helper richiesto da I1.
- `health.rs` mantiene volutamente la propria copia locale (non dichiara `mod harness;`), confermato leggendo il file — coerente con quanto dichiarato nel report e nel commento del diff.

Nessun'altra modifica fuori scope: il diff tocca solo `auth.rs`, `harness/mod.rs`, `openapi.rs`, `embed.rs` — esattamente i quattro file dichiarati nel report di fix.

## Out-of-Scope Observations

Nessuna. (M2, unica osservazione minore rimasta dalla review precedente, non richiedeva azione ed è stata correttamente lasciata invariata dall'implementer.)

## Verdict

**Fix round:** Tutti i finding risolti, nessuna rottura Critical/Important nuova nel diff di fix.

- Critical (C1, `embed::mount()`) — ADDRESSED, verificato per mutazione indipendente.
- Important (I1, `router(state)`) — ADDRESSED, verificato per mutazione indipendente.
- Nessuna nuova rottura introdotta dal fix, incluso il tocco fuori-lista a `openapi.rs` (legittimo, minimale, verificato).
- Build frontend eseguita prima della suite; confermato che i test di `embed.rs` e il nuovo test di `auth.rs` sono girati davvero, non saltati.
- Nessuna anomalia di disco/database rilevata durante questa verifica.
