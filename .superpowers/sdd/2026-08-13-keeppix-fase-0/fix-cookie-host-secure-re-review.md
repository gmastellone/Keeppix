# Re-review — fix `__Host-` / `Secure` incondizionato

Base: `d047d44`, head: `b3b1b32` (fix; il commit `24ef3ff` sopra è solo
documentazione SDD, fuori scope). Diff riletto da
`review-d047d44..b3b1b32.diff`; verifiche empiriche rifatte in proprio come
richiesto, sempre ripristinate con `Edit` (mai `git checkout`).

## Verdetto del finding Critical

**Il cookie `__Host-kpx_session` non era mai valido su HTTP in chiaro, loopback
incluso** — **ADDRESSED**.

- `crates/keeppix-api/src/cookie.rs:36,53` — `set_secure(true)` incondizionato
  in `session_cookie` e `clearing_cookie`, nessun parametro `secure: bool`.
- `should_be_secure` e `strip_port` rimossi interamente insieme ai loro tre
  test (`cookie.rs`, hunk `-86,-152` del diff); nessun residuo nel codice
  (verificato con grep su tutto `crates/`: gli unici match rimasti sono
  commenti storici in `tests/auth.rs:406,459` che descrivono il bug passato,
  non codice).
- `host()` rimossa da `routes/setup.rs`; parametro `headers: HeaderMap` tolto
  dalle firme di `refresh` e `logout` in `routes/auth.rs:149-152,179` (non
  solo il suo uso) — confermato per lettura diretta del sorgente attuale.
- **Prova di vitalità rifatta io stesso**: ho forzato `set_secure(false)`
  incondizionato in entrambe le funzioni di `cookie.rs` ed eseguito
  `cargo test -p keeppix-api --test auth -- --test-threads=1`. Risultato
  identico a quanto dichiarato nel report: **4 test su 16 rossi**, esattamente
  `login_issues_the_cookie_with_a_valid_host_prefix`,
  `login_issues_the_cookie_with_a_valid_host_prefix_on_the_default_test_host`,
  `logout_clears_the_cookie_with_a_valid_host_prefix`,
  `logout_clears_the_cookie_with_a_valid_host_prefix_on_the_default_test_host`,
  con messaggio di panic comprensibile (`manca \`Secure\` in
  \`__Host-kpx_session=...\``). Ripristinato con `Edit` a `set_secure(true)`;
  `git diff crates/keeppix-api/src/cookie.rs` torna vuoto. Riverificato che i
  16 test tornino verdi.

## Verifica del limite dichiarato su `login_then_me_stays_authenticated_on_the_same_client`

Confermato empiricamente: con `Secure` forzato assente (stessa modifica sopra),
`login_then_me_stays_authenticated_on_the_same_client` resta **`ok`** — non
rileva la regressione, esattamente come dichiarato nel report
dell'implementer. Motivo verificato per lettura: `cookie_store` (usato dal
jar di `reqwest`) non implementa la validazione del prefisso `__Host-`, quindi
un cookie senza `Secure` su una connessione in chiaro viene comunque accettato
e restituito dal client di test — non c'è alcun meccanismo che possa far
fallire questo test specifico per l'assenza dell'attributo.

Il doc comment del test (`tests/auth.rs:447-460`) dichiara onestamente questo
limite: «Questo test da solo quindi *non* troverebbe una regressione a
`should_be_secure`: la sua funzione è pinnare che il flusso normale funziona,
non sostituire `assert_host_prefix_attributes`.» È coerente con quanto
verificato — un test debole ma dichiarato come tale, non spacciato per
garanzia che non offre. Accettabile.

## Assenza di logica condizionale residua

Confermato. `grep -rn "should_be_secure\|strip_port" crates/` non trova
codice (solo i due commenti storici citati sopra). `grep -rn "set_secure"
crates/` trova solo le due chiamate incondizionate `set_secure(true)` in
`cookie.rs:36,53`. Nessun `host()`/`Host` header entra più nel calcolo di un
attributo di sicurezza del cookie in nessun handler.

## Coerenza OpenAPI e snapshot

- I due attributi `#[utoipa::path]` di `refresh` e `logout`
  (`routes/auth.rs:138-147,168-174`) non dichiarano alcun `params(...)` per
  header: la rimozione del parametro `headers: HeaderMap` dalla firma
  dell'handler è puramente un dettaglio di implementazione, non tocca la
  superficie documentata. Nessuna modifica prevista né osservata in
  `docs/api/openapi.json`.
- Eseguito `cargo test --workspace -- --test-threads=1` per intero (23
  suite/binari, tutti `test result: ok`, 0 falliti). In particolare
  `openapi_snapshot_matches_the_committed_file ... ok` — lo snapshot è
  coerente col codice.
- `git status --short -- crates/ docs/api/openapi.json` vuoto dopo la suite
  completa: nessuna deriva, il file committato non viene riscritto a runtime.
- Corroborazione aggiuntiva: `cargo clippy -p keeppix-api --all-targets --
  -D warnings` pulito, nessun warning (in particolare nessun parametro/import
  rimasto inutilizzato dopo la rimozione di `headers` da `refresh`/`logout`).

Nota di contesto, non un problema del fix: durante l'esecuzione della suite
completa sono comparse modifiche non tracciate in `frontend/` (`session.ts`,
`i18n/en.json`, `i18n/it.json`, `LoginView.vue`, nuovo `HomeView.spec.ts`) che
non erano presenti all'inizio di questa sessione (`git status --short`
iniziale era vuoto) e che io non ho introdotto — verosimilmente attività
concorrente di un'altra sessione sullo stesso albero di lavoro. Non riguardano
il fix in scope (nessun file in `crates/keeppix-api` è coinvolto) e non le ho
toccate né ripristinate, essendo fuori dal mio perimetro.

## Test Verdicts

- **Rifare la prova di vitalità (4/16 rossi)** — ADDRESSED, confermato con
  rosso→verde rifatto in proprio (vedi sopra).
- **Limite dichiarato sul test round-trip via jar** — verificato vero, e il
  doc comment è onesto sul limite.
- **Nessuna logica condizionale residua sulla sicurezza del cookie** —
  confermato.
- **Coerenza OpenAPI/snapshot dopo rimozione `headers` da `refresh`/`logout`**
  — confermato, snapshot verde, nessuna dichiarazione `params` header
  interessata.

### New Breakage in the Fix Diff

Nessuna. `cargo test --workspace` (23/23 suite verdi, 0 falliti),
`cargo clippy -p keeppix-api --all-targets -- -D warnings` pulito, albero
pulito su `crates/` e `docs/api/openapi.json` dopo la suite.

### Out-of-Scope Observations

- Modifiche non tracciate in `frontend/` comparse durante questa sessione
  (vedi sopra) — non toccano il fix in scope, presumibilmente lavoro
  concorrente di un'altra sessione sullo stesso checkout. Segnalo solo perché
  visibile con `git status`, nessuna azione richiesta da questo re-review.
- Confermo (non ri-verificato in proprio, solo letto) l'osservazione
  dell'implementer: i test che riattaccano il cookie a mano su
  `reqwest::Client::new()` (`refresh_rejects_a_reused_token`,
  `refresh_rotates_the_session_cookie`, `logout_invalidates_the_session`)
  bypassano il jar e restano fuori dal perimetro di questo fix.

### Verdict

**Fix round:** tutti i finding indirizzati, nessuna rottura Critical/Important
nuova nel diff di fix. Il limite dichiarato sul test round-trip via jar è
reale ma onestamente documentato — non è un difetto del fix, è un limite
intrinseco della libreria di test già spiegato nel brief stesso. Suite
completa verde, clippy pulito, snapshot OpenAPI coerente, albero pulito sui
file di scope.
