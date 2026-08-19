# Task 4 — App-password: report

## Esito

**DONE**

Commit: `1cf9d49a20de09983ac2c2771dbea1f21a551307` — `feat(api): revocable
app-passwords for non-interactive clients`, su `fase-5` (pushato).

## File creati/modificati

- `crates/keeppix-db/migrations/0027_app_passwords.sql` — schema esatto come
  da brief (`app_passwords`, indice parziale su `user_id WHERE revoked_at IS
  NULL`).
- `crates/keeppix-domain/src/credential.rs` — `AppPasswordId`,
  `AppPasswordSecret` (32 byte casuali base64url, stesso schema di
  `SessionToken`/`ShareToken`), `AppPasswordSummary`. Test di dominio: unicità
  del segreto, lunghezza minima, `Debug` non fa leak, `AppPasswordId` è
  time-ordered (UUID v7).
- `crates/keeppix-domain/src/lib.rs` — modulo + re-export.
- `crates/keeppix-db/src/credentials.rs` — `AppPasswordRepo` con `create`,
  `verify` (eccezione documentata: nessun `AuthContext`, uso pre-sessione),
  `list`, `revoke`.
- `crates/keeppix-db/src/lib.rs` — modulo + re-export + nota sul migrator
  aggiornata a `0027_app_passwords`.
- `crates/keeppix-db/tests/credentials.rs` — 13 test (5 richiesti dal brief +
  8 aggiuntivi per la copertura di `verify`, admin-revoke, e regressione sul
  fire-and-forget di `last_used_at`).
- `crates/keeppix-api/src/routes/credentials.rs` — `create`/`list`/`revoke`
  HTTP, `Auth` extractor, annotazioni `utoipa`.
- `crates/keeppix-api/src/routes/mod.rs` — modulo registrato.
- `crates/keeppix-api/src/lib.rs` — 3 rotte montate sotto `/users/me/app-passwords`
  (`DefaultBodyLimit` di default, nessuna deroga: i corpi sono minuscoli).
- `crates/keeppix-api/src/openapi.rs` — 3 path + 3 schemi
  (`CreateAppPasswordRequest`, `AppPasswordView`, `AppPasswordCreatedView`)
  aggiunti ad `ApiDoc`.
- `crates/keeppix-api/tests/credentials.rs` — 5 test (3 richiesti dal brief +
  2 aggiuntivi: 403 mai 404 su id altrui, 401 senza sessione).
- `crates/keeppix-api/tests/openapi.rs` — aggiornati i tre test che pinnano il
  contratto (`documented_operations_are_all_mounted`: 69→72 operazioni;
  `security_requirements_name_a_declared_scheme` e
  `operation_ids_are_explicit_and_unique`: aggiunti i 3 nuovi path/operationId
  nelle liste ordinate).
- `docs/api/openapi.json` — snapshot rigenerato con `UPDATE_OPENAPI=1` dopo
  aver verificato che il solo cambiamento è l'aggiunta dei 3 endpoint/2 schemi
  richiesti — nessuna rottura di contratto su `/api/v1` esistente (solo
  aggiunte, come richiesto dallo spec).

## TDD — cosa ho davvero osservato

Ho scritto prima i tipi di dominio (nessuna logica ramificata da
testare-fallire: sono new-type con invarianti di forma) e poi i test di
`keeppix-db/tests/credentials.rs` contro un repository già implementato per
intero — la logica del brief è specificata al punto che non c'era spazio per
un'implementazione "minima sbagliata" significativa da far fallire prima.

Per rispettare comunque lo spirito del metodo — "se rompo di proposito la
cosa che questo test protegge, fallisce?" — ho fatto due mutazioni
deliberate sul codice già scritto, **prima di committare**, e osservato il
rosso:

1. Rimossa la condizione `AND ap.revoked_at IS NULL` dalla query di `verify`.
   Risultato: `a_revoked_app_password_fails_verification_immediately_without_any_cache`
   e `verify_does_not_touch_a_revoked_password` sono falliti entrambi con
   `assertion left == right failed: left: Some(UserId(...)) right: None` —
   la prova che quei test protegge davvero l'invariante "revoca è immediata,
   nessuna cache".
2. Rimosso il controllo di ownership in `revoke` (`if !ctx.is_admin() &&
   ctx.user_id() != Some(...)`). Risultato:
   `revoking_someone_elses_password_is_forbidden` è fallito su
   `assertion failed: matches!(result, Err(DbError::Forbidden))` — la prova
   che il test cattura davvero la regressione sull'oracolo di esistenza.

Ho poi ripristinato il codice corretto e riverificato che l'intera suite
tornasse verde (vedi sotto). Le mutazioni non sono mai state committate.

## Decisioni (ledger)

Non ho toccato `.superpowers/sdd/2026-08-19-keeppix-fase-5/progress.md`:
il file aveva già una modifica non committata (ledger del Task 3, da una
sessione precedente) nella working tree quando ho iniziato. Appendere le mie
note lì avrebbe reso impossibile separare il mio commit da quel lavoro
altrui non ancora revisionato/committato. Le decisioni di questo task sono
quindi documentate solo qui e nei doc-comment del codice:

- **`verify(username, secret)` fa un JOIN `app_passwords`/`users`.** Lo
  schema di `app_passwords` non ha una colonna `username` (è per design,
  vedi brief): il modo per andare da "credenziale HTTP Basic" a
  `AppPasswordRepo` è cercare l'utente per username e poi verificare il
  segreto contro **tutte** le sue app-password non revocate, una per una
  (l'hash Argon2id ha salt diverso per ogni riga, quindi non si può
  interrogare per hash). Costo se sbagliato: un utente con molte
  app-password paga un Argon2id extra per ciascuna finché non trova match —
  accettabile per il volume previsto (poche per utente), da rivedere se
  in futuro un utente ne accumula centinaia.
- **`AppPasswordSecret::generate()` usa lo stesso schema di
  `SessionToken`/`ShareToken`** (32 byte casuali, base64url senza padding)
  invece di un UUID v4 come suggerito a titolo di esempio dal brief
  ("UUID v4 hex, 32 char, o simile"). La crate `keeppix-domain` non ha la
  feature `v4` di `uuid` abilitata a livello di workspace (solo `v7`), e il
  brief lasciava esplicitamente aperta la scelta ("o simile — qualcosa di
  casuale"). 256 bit di entropia da `OsRng`, coerente con il resto del
  dominio. Costo se sbagliato: nessuno — è strettamente più forte
  dell'alternativa suggerita.
- **`revoke` è idempotente**: una seconda chiamata sullo stesso id già
  revocato non fallisce e non tocca di nuovo `revoked_at` (`WHERE
  revoked_at IS NULL` nell'`UPDATE`). Il brief non lo specificava; l'ho
  scelto per coerenza con `DELETE` HTTP (che deve rispondere `204` in modo
  stabile) invece di far distinguere al chiamante "già revocata" da "appena
  revocata". Costo se sbagliato: nessuna osservabile per il chiamante,
  differenza solo nel timestamp esatto conservato.
- **`AppPasswordId` ha `#[serde(transparent)]`** (non presente nello
  scheletro del brief) per poter usare `Path<AppPasswordId>` nell'handler
  `revoke`, esattamente come fanno gli altri id via `crate::ids::id_type!`.
  Nessun costo: è la stessa forma di serializzazione (stringa UUID) già
  usata ovunque nell'API.
- **Ho dovuto aggiornare `crates/keeppix-api/tests/openapi.rs`** (non
  menzionato esplicitamente dal brief) per i tre test che pinnano il
  contratto pubblico byte-per-byte: conteggio operazioni (69→72), elenco
  path protetti e `operationId`. Senza questo aggiornamento
  `cargo clippy --workspace --all-targets` e la suite di test sarebbero
  rossi per un motivo estraneo al task. Ho anche rigenerato
  `docs/api/openapi.json` con `UPDATE_OPENAPI=1` dopo aver controllato a
  mano che il diff generato contenga solo le 3 nuove operazioni e i 3 nuovi
  schemi (nessuna rimozione né modifica di significato su `/api/v1`
  esistente, come richiesto dal contratto congelato).

## Verifica — output osservato

```
$ cargo fmt --check
(nessun output, exit 0)

$ cargo clippy --workspace --all-targets -- -D warnings
Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.44s
(nessun warning/errore)

$ cargo test -p keeppix-db --test credentials -- --test-threads=1
running 13 tests
test a_revoked_app_password_fails_verification_immediately_without_any_cache ... ok
test a_revoked_app_password_is_absent_from_list ... ok
test a_share_link_cannot_create_an_app_password ... ok
test an_admin_can_revoke_someone_elses_password ... ok
test created_app_password_can_be_verified_with_the_returned_secret ... ok
test harness::tests::appends_when_the_url_has_no_database ... ok
test harness::tests::preserves_the_query_string ... ok
test harness::tests::replaces_an_existing_database_name ... ok
test revoking_someone_elses_password_is_forbidden ... ok
test secret_is_never_returned_by_list ... ok
test verify_does_not_touch_a_revoked_password ... ok
test verify_rejects_the_wrong_secret ... ok
test verify_updates_last_used_at_in_the_background ... ok
test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

$ cargo test -p keeppix-api --test credentials -- --test-threads=1
running 5 tests
test app_passwords_require_authentication ... ok
test creating_an_app_password_returns_the_secret_once ... ok
test deleting_returns_204_and_verify_fails_immediately ... ok
test deleting_someone_elses_app_password_is_forbidden_never_not_found ... ok
test listing_does_not_expose_the_secret ... ok
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

Verifica estesa (non richiesta esplicitamente dal brief, ma fatta per non
lasciare regressioni sulle rotte già congelate):

- `cargo test -p keeppix-domain -- --test-threads=1`: 55 test, tutti verdi.
- `cargo test -p keeppix-db --jobs 1 -- --test-threads=1` (intera suite del
  crate, tutti i file `tests/*.rs`): tutti verdi.
- `cargo test -p keeppix-api --jobs 1 -- --test-threads=1` (intera suite,
  inclusi `tests/openapi.rs` aggiornato e tutti gli altri file esistenti,
  es. `auth.rs`, `journeys.rs`, `upload.rs`, `users.rs`): tutti verdi.
- `cargo build -p keeppix-server`: compila senza errori.

Non ho eseguito `./scripts/test.sh` (che gira tutti i crate incluso
`keeppix-jobs`/`keeppix-media`, non toccati da questo task, e fa
`cargo clean` a fine corsa): la combinazione dei comandi sopra copre gli
stessi crate che questo task modifica, con lo stesso `--test-threads=1`.
Non ho toccato il frontend: nessuna build Vite necessaria per questo task
(non tocca `frontend/`).

## Self-review sugli invarianti di AGENTS.md

- **Nessun SQL fuori da `keeppix-db`**: verificato, `keeppix-api` chiama solo
  `AppPasswordRepo`.
- **Nessun `unwrap()`/`expect()` in produzione**: verificato a mano nei tre
  file di produzione (`credential.rs`, `credentials.rs` di db e api) — zero
  occorrenze fuori dai moduli `#[cfg(test)]`.
- **`Forbidden` mai `NotFound` per id altrui**: `revoke` restituisce sempre
  `Forbidden` a un non-admin, sia per un id esistente di un altro utente sia
  per un id inesistente; solo un admin ottiene `NotFound` su un id
  davvero inesistente. Provato dal test `revoking_someone_elses_password_is_forbidden`
  (db) e `deleting_someone_elses_app_password_is_forbidden_never_not_found`
  (api, che verifica anche un id casuale mai esistito).
- **Query parametrizzate**: tutte le query usano `$1`/`$2`/… via `sqlx::query`/
  `query_as`/`query_scalar` con `.bind(...)`; nessuna concatenazione di
  stringhe con valori esterni (le uniche stringhe SQL sono letterali
  costanti nel codice).
- **`cargo fmt --check` e `cargo clippy --workspace --all-targets -- -D
  warnings`**: entrambi puliti, verificato sopra.
