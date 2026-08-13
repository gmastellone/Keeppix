# Report — fix `__Host-` / `Secure` incondizionato

Fix fuori piano sul branch `fase-0`, a partire da `1cd6549`.

## File toccati e perché

- `crates/keeppix-api/src/cookie.rs` — `session_cookie` e `clearing_cookie` non
  prendono più il parametro `secure: bool`: impostano `set_secure(true)`
  incondizionato. Rimossi `should_be_secure` e `strip_port` e i loro tre test
  (`localhost_variants_are_not_secure`, `real_hosts_and_missing_host_are_secure`,
  `lookalike_hosts_are_not_treated_as_local`), ormai privi di soggetto.
  Riscritti i doc comment del modulo e delle due funzioni per spiegare la
  distinzione fra "il prefisso `__Host-` impone `Secure` sempre" e "i browser
  (e, verificato empiricamente, `cookie_store`/`reqwest`) esentano il loopback
  solo dal requisito di *trasporto* sicuro per onorare l'attributo già
  presente, non dal requisito di *impostarlo*".
- `crates/keeppix-api/src/routes/setup.rs` — rimosso l'uso di
  `should_be_secure`/`host` in `create`; rimossa del tutto la funzione
  `host()`, diventata morta. `user_agent()` resta invariata, `headers` resta
  nella firma di `create` perché ancora usato per `user_agent(&headers)`.
- `crates/keeppix-api/src/routes/auth.rs` — stesso taglio in `login`. In
  `refresh` e `logout` `headers: HeaderMap` serviva solo per `host(&headers)`:
  rimosso il parametro dalla firma di entrambi gli handler (non solo l'uso),
  come richiesto dal brief. L'import `HeaderMap` resta perché `login` lo usa
  ancora.
- `crates/keeppix-api/tests/auth.rs` — vedi sezione dedicata sotto.

Non toccati (come da brief): `frontend/`, `docs/`, `Cargo.toml` di workspace,
`crates/keeppix-api/src/openapi.rs` e gli attributi `utoipa` sugli handler.
La rimozione del parametro `headers` da `refresh`/`logout` non ha richiesto
alcuna modifica a `openapi.rs`: quegli handler non dichiaravano parametri
`header` nell'attributo `#[utoipa::path]`, quindi lo snapshot OpenAPI
(`openapi_snapshot_matches_the_committed_file`) passa invariato.

## Decisione su `PRODUCTION_HOST`

Tenuto, ma ridotto a guardia di regressione pura: il suo doc comment è stato
riscritto per dire chiaramente che, con il fix, quel test non distingue più
un'implementazione corretta da una condizionata dall'host (entrambe
produrrebbero `Secure` per quell'host contraffatto) — la proprietà che era
rotta è provata dai test gemelli aggiunti contro l'host reale dell'harness
(`*_on_the_default_test_host`). L'ho tenuto perché resta un test economico
contro un futuro tentativo di reintrodurre logica condizionale basata
sull'header `Host` (es. "sicuro solo se Host è nell'allowlist"), un pattern
diverso da quello originale ma con la stessa forma di bug.

Aggiunti quattro test nuovi:
- `logout_clears_the_cookie_with_a_valid_host_prefix_on_the_default_test_host`
- `login_issues_the_cookie_with_a_valid_host_prefix_on_the_default_test_host`

  Stesse asserzioni letterali di `assert_host_prefix_attributes`, ma contro il
  client di default della `TestServer` (host reale `127.0.0.1:<porta>`,
  nessun `Host` contraffatto). Sono i due test che rilevano davvero il difetto
  originale (vedi prova di vitalità sotto).

- `login_then_me_stays_authenticated_on_the_same_client`

  Round-trip comportamentale: login col client di default (cookie-jar
  automatico), poi `GET /api/v1/auth/me` sullo *stesso* client → 200. Prova
  la sequenza "login → richiesta successiva resta autenticata" del criterio
  di completamento della Fase 0.

Corretto anche il doc comment di `assert_host_prefix_attributes` (righe
399-407 originali), che ripeteva la stessa premessa falsa su un presunto
scarto di `reqwest` sui cookie `Secure` in chiaro su loopback: ora spiega che
il vero motivo per cui bisogna leggere l'header grezzo è che `cookie_store`
non implementa affatto la validazione del prefisso `__Host-`, non che scarti
il cookie per il trasporto.

## Nota importante sul test di round-trip

Il brief chiedeva di verificare che il test end-to-end (round-trip via jar)
**trovasse** il bug se reintrodotto, "per almeno due delle asserzioni
chiave" insieme al test letterale di default. La prova di vitalità (sotto)
mostra che **non lo trova**: `login_then_me_stays_authenticated_on_the_same_client`
resta verde anche con `Secure` reintrodotto come assente. Questo è coerente
con l'analisi che il brief stesso fa nella sezione "Perché nessun test
esistente l'ha mai preso": un cookie *senza* `Secure` è sempre accettato da
un client su HTTP in chiaro (non c'è nulla da rifiutare), e `cookie_store` non
implementa comunque la regola del prefisso `__Host-`. Nessuna libreria HTTP
generica può quindi far fallire un round-trip solo perché `Secure` manca.
Ho lasciato il test così com'è (il brief stesso lo prevede come "test
comportamentale che affianca quello letterale", non come sostituto) e ho reso
esplicito nel suo doc comment che da solo non copre la regressione — la
copertura di regressione reale è nei due test letterali contro l'host di
default. Lo segnalo come scostamento dalla lettera dell'istruzione (che
chiedeva il rosso su "almeno due" incluso il round-trip), non come lavoro
lasciato a metà: verificarlo per davvero, come chiesto, ha prodotto questo
risultato, e forzare il test a diventare rosso avrebbe richiesto di fargli
ispezionare l'header invece del jar — cioè renderlo un doppione del test
letterale, snaturandone lo scopo dichiarato dal brief stesso.

## Prova di vitalità (rosso → verde)

Procedura: in `cookie.rs`, in entrambe `session_cookie` e `clearing_cookie`,
`set_secure(true)` → `set_secure(false)` (omissione forzata e incondizionata,
equivalente per queste asserzioni alla vecchia logica su loopback dato che
l'host di test è sempre `127.0.0.1`). Eseguito
`cargo test -p keeppix-api --test auth -- --test-threads=1`.

**Rosso** (4 test falliti su 16, messaggio comprensibile):

```
test login_issues_the_cookie_with_a_valid_host_prefix ... FAILED
test login_issues_the_cookie_with_a_valid_host_prefix_on_the_default_test_host ... FAILED
test logout_clears_the_cookie_with_a_valid_host_prefix ... FAILED
test logout_clears_the_cookie_with_a_valid_host_prefix_on_the_default_test_host ... FAILED
test login_then_me_stays_authenticated_on_the_same_client ... ok   (vedi nota sopra)
test result: FAILED. 12 passed; 4 failed; 0 ignored; 0 measured; 0 filtered out

thread 'login_issues_the_cookie_with_a_valid_host_prefix_on_the_default_test_host' panicked at crates/keeppix-api/tests/auth.rs:533:9:
manca `Secure` in `__Host-kpx_session=yTlcbWTlBovgu0GpcAbIiAq5hqUhFXi_j_0aYCV2xuQ; HttpOnly; SameSite=Lax; Path=/; Max-Age=3600`

thread 'logout_clears_the_cookie_with_a_valid_host_prefix_on_the_default_test_host' panicked at crates/keeppix-api/tests/auth.rs:533:9:
manca `Secure` in `__Host-kpx_session=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0`
```

Ripristinato `set_secure(true)` in entrambe le funzioni.

**Verde**:

```
running 16 tests
test a_fresh_instance_reports_not_initialised ... ok
test login_fails_identically_for_unknown_user ... ok
test login_fails_with_wrong_password ... ok
test login_issues_the_cookie_with_a_valid_host_prefix ... ok
test login_issues_the_cookie_with_a_valid_host_prefix_on_the_default_test_host ... ok
test login_succeeds_with_correct_credentials ... ok
test login_then_me_stays_authenticated_on_the_same_client ... ok
test logout_clears_the_cookie_with_a_valid_host_prefix ... ok
test logout_clears_the_cookie_with_a_valid_host_prefix_on_the_default_test_host ... ok
test logout_invalidates_the_session ... ok
test me_requires_authentication ... ok
test refresh_rejects_a_reused_token ... ok
test refresh_rotates_the_session_cookie ... ok
test setup_can_only_run_once ... ok
test setup_creates_the_first_admin_and_logs_in ... ok
test setup_rejects_a_weak_password ... ok
test result: ok. 16 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 12.36s
```

## Verifica finale (tre comandi)

Ambiente: `KEEPPIX_TEST_DATABASE_URL=postgres://keeppix:keeppix@127.0.0.1:5432/postgres`,
Postgres già raggiungibile (nessun bisogno di `pg_ctlcluster`).

1. `cargo test --workspace -- --test-threads=1` → **tutti verdi**. Tutte le
   suite del workspace (`keeppix-api` incluso `tests/auth.rs` 16/16,
   `keeppix-db`, `keeppix-domain`, `keeppix-server`) a 0 falliti, 0 ignorati.
2. `cargo clippy --workspace --all-targets -- -D warnings` → pulito, nessun
   warning.
3. `cargo fmt --check` → inizialmente ha segnalato una riformattazione
   dovuta all'`assert_eq!` con messaggio aggiunto in
   `login_then_me_stays_authenticated_on_the_same_client`; applicato
   `cargo fmt`, poi `--check` pulito.

Dopo `cargo fmt` ho rieseguito `cargo test -p keeppix-api --test auth` per
sicurezza: ancora 16/16.

## Cosa ho notato ma non corretto

- I test che riattaccano un cookie a mano su un `reqwest::Client::new()`
  (`refresh_rejects_a_reused_token`, `refresh_rotates_the_session_cookie`,
  `logout_invalidates_the_session`) continuano a funzionare perché
  impostano l'header `cookie` direttamente, bypassando qualunque logica di
  jar — non toccati, fuori perimetro del brief.
- Non ho toccato `docs/` né `openapi.rs` come richiesto; non ho verificato se
  esista documentazione utente/deployment (fuori repo, es. Compose) che
  menzioni esplicitamente l'assenza di `Secure` su loopback: il brief non lo
  chiedeva e non ho trovato riferimenti nei file toccati.

## Commit

Un solo commit sul branch `fase-0`, albero pulito a parte questo file di
report (che verrà aggiunto in un commit separato se richiesto dal
controller).
