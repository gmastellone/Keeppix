# Task 10 — re-review formale del fix round 1/5

**Diff in esame:** `ab19d33..4132af7`, solo `crates/` — salvato in
`review-ab19d33..4132af7.diff`
**Eseguita:** 2026-08-13, sessione cloud (la review era stata saltata al
passaggio di sessione)
**Verdetto spec:** ✅
**Verdetto qualità:** approvato con riserva (vedi N1)

## Nota d'ambiente

Il pull dell'immagine `postgis/postgis:17-3.5` è bloccato dalla policy di
egress: il manifest passa, i blob su `production.cloudfront.docker.com:443`
ricevono 403 al CONNECT (confermato da
`curl http://127.0.0.1:43727/__agentproxy/status` → `connect_rejected`).
Testcontainers non è quindi utilizzabile. Il reviewer ha aggirato l'ostacolo
**senza toccare le asserzioni**: cluster PostgreSQL 16 locale, database vergine
per test, patch temporanea al solo provisioning nei due harness, ripristinata a
fine lavoro (`git status` vuoto, `git diff 4132af7` vuoto). Le migrazioni
richiedono solo `pg_trgm`, disponibile in PG16.

La deviazione è stata poi resa permanente e pulita dal controller — vedi il
ruling R9 nel ledger e in `docs/superpowers/plans/2026-08-13-keeppix-fase-0-STATO.md`.
Unico caveat residuo: i test di integrazione girano su PG16 anziché PG17.

## Comandi di verifica

| Comando | Esito |
|---|---|
| `cargo test --workspace -- --test-threads=1` | **71 test, 0 falliti** (api lib 4, api/auth 11, api/health 3, db/migrations 4, db/sessions 11, db/settings 3, db/users 9, domain 22, server/config 4) |
| `cargo clippy --workspace --all-targets -- -D warnings` | pulito |
| `cargo fmt --check` | pulito |

Senza `--test-threads=1` i 4 test di `keeppix-server/tests/config.rs` falliscono
perché manipolano l'ambiente di processo: vincolo pre-esistente documentato nel
file stesso, non una regressione del Task 10.

## Metodo

Ogni finding è stato verificato **per mutazione**: rompendo il codice di
produzione e controllando che il test corrispondente diventasse rosso, poi
ripristinando. È il metodo che nella fase precedente ha smascherato tre test che
passavano senza provare ciò che il loro nome affermava.

## Finding per finding

### F1 — `clearing_cookie` senza `Secure`/`SameSite` — CHIUSO nel codice, NON pinnato da test

La funzione è ora `clearing_cookie(secure: bool)` e imposta `Secure` +
`SameSite=Lax`. L'unico call site che cancella il cookie è `logout`
(`grep` su `clearing_cookie` in tutto `crates/`: solo `cookie.rs:31` e
`routes/auth.rs:155`), e passa `should_be_secure(host(&headers))`, coerente con
`login`, `refresh` e `setup`. Verificato eseguendo un test usa-e-getta che
stampa gli header reali:

```
SETUP  set-cookie (host locale) = "__Host-kpx_session=R7t0...; HttpOnly; SameSite=Lax; Path=/; Max-Age=3600"
LOGOUT set-cookie (host locale) = "__Host-kpx_session=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0"
LOGOUT set-cookie (host prod)   = "__Host-kpx_session=; HttpOnly; SameSite=Lax; Secure; Path=/; Max-Age=0"
LOGIN  set-cookie (host prod)   = "__Host-kpx_session=ZnRj...; HttpOnly; SameSite=Lax; Secure; Path=/; Max-Age=3600"
LOGOUT set-cookie (lookalike)   = "__Host-kpx_session=; HttpOnly; SameSite=Lax; Secure; Path=/; Max-Age=0"
```

Il cookie cancellante è ora attributo per attributo identico a quello di
sessione, `Max-Age` a parte. **Ma** rimuovendo `set_secure(secure)` e
`set_same_site(SameSite::Lax)` da `clearing_cookie` l'intera suite `keeppix-api`
resta verde (`4 passed`, `11 passed`, `3 passed`). Da cui il finding N1.

### F2 — pin su `DUMMY_HASH` — CHIUSO

Il test asserisce ora in positivo `verify_password(DUMMY_HASH_PLAINTEXT,
dummy_hash())`. Mutazione che riproduce il bug originale (ultimo segmento PHC
reso non-base64, `...l9Dhmh!`):

```
thread 'routes::auth::tests::dummy_hash_is_a_valid_argon2id_phc_string' panicked at crates/keeppix-api/src/routes/auth.rs:207:9:
il parsing dell'hash deve riuscire e Argon2 deve girare per intero
test result: FAILED. 3 passed; 1 failed
```

Verificato anche che i parametri pinnati dal test (`m=19456,t=2,p=1`)
coincidano con `ARGON_M_COST/T/P` in `crates/keeppix-domain/src/password.rs:10-12`:
il tempo del calcolo fittizio è quindi davvero comparabile a quello reale, che
è il punto del ruling R6.

### F3 — copertura di `should_be_secure` — CHIUSO

Tre unit test in `crates/keeppix-api/src/cookie.rs:79-106`: varianti locali
(incluse `[::1]:8080` e `::1`), host reali + `None`, e host lookalike.

### F4 — match esatto invece di prefix-match — CHIUSO

Mutazione: rimesso il vecchio
`!matches!(host, Some(h) if h.starts_with("127.0.0.1") || h.starts_with("localhost"))`:

```
test cookie::tests::localhost_variants_are_not_secure ... FAILED
test cookie::tests::lookalike_hosts_are_not_treated_as_local ... FAILED
  assertion failed: !should_be_secure(Some("[::1]:8080"))
  assertion failed: should_be_secure(Some("127.0.0.1.evil.com"))
test result: FAILED. 2 passed; 2 failed
```

Entrambe le proprietà — lookalike respinti, IPv6 riconosciuti — sono realmente
pinnate. `strip_port` gestisce correttamente `[::1]:8080`, `::1` e la porta
semplice.

### F5 — revoca server-side al logout — CHIUSO

Mutazione: `logout` non chiama più `SessionRepo::revoke` (ramo reso
irraggiungibile), continuando a emettere il clearing cookie:

```
test logout_invalidates_the_session ... FAILED
thread 'logout_invalidates_the_session' panicked at crates/keeppix-api/tests/auth.rs:328:5:
assertion `left == right` failed: la sessione deve essere invalidata lato server, non solo dimenticata dal client
  left: 200
 right: 401
test result: FAILED. 10 passed; 1 failed
```

Il client fresco che ripresenta il cookie pre-logout ottiene 200 con la
mutazione: il test prova davvero la revoca server-side, non l'amnesia del
cookie store.

### F6 — morte del vecchio token dopo la rotazione — CHIUSO (entrambi i test)

Mutazione: `refresh` sostituisce `rotate` con `authenticate` + `create`, cioè
emette un figlio *in parallelo* senza consumare il genitore — esattamente lo
scenario che il vecchio test non distingueva:

```
test refresh_rejects_a_reused_token ... FAILED
  assertion `left == right` failed
  left: 204   right: 401
test refresh_rotates_the_session_cookie ... FAILED
  assertion `left == right` failed: il token pre-refresh deve essere stato consumato, non solo affiancato da uno nuovo
  left: 200   right: 401
test result: FAILED. 9 passed; 2 failed
```

### Pulizia — `pub type Ctx` rimosso — CHIUSO

`grep -rn "Ctx\b" crates/ --include=*.rs` non restituisce nulla; l'import
`AuthContext` in `routes/auth.rs` è stato tolto insieme all'alias; build e
clippy puliti.

## Finding nuovi

### N1 — Important — il fix di F1 non ha alcun test di regressione

Rimuovendo `set_secure(secure)` e `set_same_site(SameSite::Lax)` da
`clearing_cookie` (`crates/keeppix-api/src/cookie.rs:31-38`), tutta la suite
`keeppix-api` resta verde (`4 passed` / `11 passed` / `3 passed`). Il bug che F1
correggeva — un `__Host-` cancellante scartato per intero dal browser, con il
cookie di sessione che sopravvive al logout in produzione — può quindi rientrare
in silenzio alla prossima modifica. Non è una regressione ipotetica: è lo stesso
identico difetto appena chiuso.

Aggravante metodologica: contro l'harness locale `secure` vale comunque `false`,
quindi un test scritto ingenuamente non proverebbe nulla; serve un test su
`logout` con `Host` di produzione contraffatto che asserisca `Secure`,
`SameSite=Lax`, `Path=/`, `Max-Age=0` — dimostrato fattibile con la sonda
usa-e-getta riportata in F1. Simmetricamente manca anche l'asserzione su
`Secure` per il cookie *di sessione* con host di produzione.

**Esito:** chiuso dal fix round 2/5, commit `90f8b82`.

### N2 — Minor — `refresh_rejects_a_reused_token` non copre la revoca della famiglia

Il commento (`crates/keeppix-api/tests/auth.rs:246-251`) afferma che il test
copre il ramo "revoca l'intera famiglia... la cui copertura HTTP era nulla", ma
il corpo asserisce solo `reused.status() == 401`. Mutazione: in
`SessionRepo::rotate` tolto l'`UPDATE ... SET revoked_at` sulla famiglia
lasciando il `return Err(DbError::Forbidden)`:

```
test refresh_rejects_a_reused_token ... ok
test result: ok. 11 passed; 0 failed          # keeppix-api/tests/auth.rs
```

mentre a livello DB la stessa mutazione viene intercettata:

```
test reusing_a_consumed_token_kills_the_whole_family ... FAILED   # crates/keeppix-db/tests/sessions.rs:89
```

La proprietà è coperta, ma non al livello HTTP e non da questo test: il commento
sovradichiara.

**Esito:** chiuso dal fix round 2/5 nella forma forte (asserzione aggiunta).

### N3 — Minor — `dummy_hash_is_a_valid_argon2id_phc_string` è sovra-pinnato

Mutando il solo salt (`BKjMC3FKz54nTDnFf9fLRQ` → `...fLRR`) — hash ancora PHC
valido, Argon2 gira comunque per intero, proprietà di sicurezza intatta — il
test diventa rosso. È il prezzo inevitabile dell'asserzione positiva ed è
documentato; l'unica conseguenza è che ruotare la costante impone di rigenerarla
dal plaintext dichiarato. Nessuna azione richiesta.

### N4 — Minor — `logout` risponde 204 anche quando `revoke` fallisce

`crates/keeppix-api/src/routes/auth.rs:148-155`: l'errore viene solo loggato a
`warn`, il clearing cookie parte comunque. Con un blip del database l'utente
crede di essere uscito mentre la sessione resta viva lato server fino alla
scadenza. Coerente con il commento "Sempre 204" e imparentato con il difetto già
accettato "`Auth` mappa qualsiasi errore a 401", ma distinto: qui il fallimento
è invisibile al client. Da triare nel review finale del branch.

### N5 — Minor (informativo) — forme equivalenti non coperte da `should_be_secure`

Il confronto è case-sensitive (`Host: LOCALHOST`) e non riconosce la forma
estesa `0:0:0:0:0:0:0:1` né `[127.0.0.1]`. Tutti questi casi cadono sul lato
**sicuro** (`Secure = true`): al massimo rompono uno sviluppo locale esotico.
Nessuna azione.

### Osservazione di spec (non un finding del Task 10)

§9.5 richiede, come difesa CSRF, `SameSite=Lax` **più** obbligo di
`Content-Type: application/json` e di un header custom sulle mutazioni. Le
mutazioni di questo task (`/auth/refresh`, `/auth/logout`) non hanno corpo né
header custom richiesto. Non è nel contratto del Task 10 (il piano non lo
elenca) e `SameSite=Lax` da solo già impedisce l'invio del cookie su POST
cross-site, ma va registrato per il review finale del branch.

## Difetti già accettati, confermati e non ri-segnalati

`Auth` → 401 indiscriminato (incluso DB down); `refresh`/`rotate` che non
ricontrollano `users.disabled_at`; assenza di rate limiting; 405 non
problem+json; deadlock su replay concorrenti; `should_be_secure` basato su un
header client-controlled (ruling R7).
