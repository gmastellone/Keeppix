# Task 15 — Integrazione continua — Report

## Cosa è stato implementato

Creati i tre file previsti dal brief, con le quattro correzioni imposte dal
preflight applicate fin da subito (non come fix successivo):

- `.github/workflows/ci.yml`
- `.github/workflows/release.yml`
- `deny.toml`

### Scostamenti dal brief (per ruling del preflight)

1. **`SQLX_OFFLINE: "true"` rimosso dal blocco `env:` di `ci.yml`** (ruling R4).
   Non esiste alcuna cache `.sqlx/` nel repository; il codice usa le forme
   funzione di sqlx (`sqlx::query(...)` a runtime), non le macro
   `query!`/`query_as!` che richiederebbero verifica a compile-time contro un
   DB o una cache offline. La variabile non serve e mentiva su come funziona
   la build.

2. **Toolchain portato a `dtolnay/rust-toolchain@1.88.0`** (ruling R2).
   Verificato che `rust-toolchain.toml` fissa `channel = "1.88.0"` e che
   `Cargo.toml` del workspace dichiara `rust-version = "1.88"`: il codice usa
   let-chain, stabili solo da 1.88. Con 1.85 la build non compilerebbe.

3. **Nessun `git push`.** Step 5 eseguito solo fino al commit; il push
   resta al controller, come da nota P2.

4. **Lista licenze di `deny.toml` corretta e ampliata a valle di
   `cargo deny check` reale** (vedi sezione dedicata sotto), non copiata dal
   brief.

Il resto dei due workflow è stato scritto identico al brief (job `backend`,
`frontend`, `audit`, `image` in `ci.yml`; job `publish` in `release.yml`),
perché non c'erano altri problemi individuati in fase di lettura o di
verifica locale. `ci.yml` include ora un commento nello step "Test" del job
`backend` che spiega dove/come viene usata `KEEPPIX_TEST_DATABASE_URL` (P4):
non è impostata nel workflow, perché su GitHub Actions Docker è disponibile
e l'harness usa testcontainers di default.

## `deny.toml` — verifica reale, non teorica

Ho installato `cargo-deny` in locale (`cargo install cargo-deny --locked`,
completato in ~2m14s — crates.io raggiungibile come previsto) ed eseguito
`cargo deny check advisories bans licenses` iterando fino al verde.

**Primo giro** (con la lista del brief, corretta solo nell'identificatore
AGPL): due errori `rejected: failed to satisfy license requirements`,
entrambi per `CDLA-Permissive-2.0` dichiarata da `webpki-roots` (versioni
0.26.11 e 1.0.9, dipendenza transitiva di `sqlx-postgres` via
`rustls-platform-verifier`/TLS). Nessun altro errore di licenza: la
correzione `AGPL-3.0` → `AGPL-3.0-or-later` (vedi sotto) da sola bastava per
far accettare i crate del progetto.

**Lista finale in `deny.toml`:**

```toml
allow = ["MIT", "Apache-2.0", "Apache-2.0 WITH LLVM-exception", "BSD-2-Clause",
         "BSD-3-Clause", "ISC", "Unicode-3.0", "Zlib", "MPL-2.0",
         "AGPL-3.0-or-later", "CDLA-Permissive-2.0"]
```

Motivazione di ogni voce aggiunta/modificata rispetto al brief:

- **`AGPL-3.0-or-later`** (sostituisce `AGPL-3.0` del brief): è la licenza
  dichiarata da tutti i crate `keeppix-*` del workspace
  (`license = "AGPL-3.0-or-later"` in `Cargo.toml`). `AGPL-3.0` (senza
  `-or-later`) è un identificatore SPDX diverso e non copre i crate del
  progetto stesso — con la lista del brief, `cargo deny` li avrebbe respinti.
  Accettarla è ovviamente coerente con un progetto che è esso stesso AGPL.
- **`CDLA-Permissive-2.0`** (nuova, non nel brief): licenza dei bundle di
  root CA Mozilla ridistribuiti da `webpki-roots`, dipendenza transitiva di
  `sqlx-postgres`/TLS. È una licenza permissiva stile Creative Commons
  pensata proprio per dataset come elenchi di certificati; nessuna clausola
  copyleft, nessun conflitto con AGPL-3.0-or-later del progetto. Compatibile
  con l'uso in un binario proprietario o AGPL indifferentemente.

Le altre voci del brief (MIT, Apache-2.0, Apache-2.0 WITH LLVM-exception,
BSD-2-Clause, BSD-3-Clause, ISC, Unicode-3.0, Zlib, MPL-2.0) sono risultate
tutte necessarie e sufficienti: rimuovendole una alla volta in una prova
locale (non committata) l'albero delle dipendenze le richiede davvero
(es. Unicode-3.0 per `unicode-ident`/`icu_*`, MPL-2.0 per `webpki-roots`
stesso in alcune versioni precedenti, Zlib per `foldhash`). Non ho aggiunto
nessun'altra licenza oltre a queste due modifiche: il comando tace con
esattamente questa lista.

**Esito finale:**

```
$ cargo deny check advisories bans licenses
...
advisories ok, bans ok, licenses ok
```

**Advisories:** `advisories ok` — nessuna vulnerabilità RUSTSEC, nessuna
licenza yanked segnalata (`yanked = "deny"` non ha trovato nulla da
respingere). Nessun finding di sicurezza da riportare.

**Bans:** `bans ok`. Ci sono numerosi warning `duplicate` (più versioni
della stessa crate nell'albero: `base64`, `windows_*`, `syn`, `rand`, ecc.),
tutti attesi e non bloccanti perché `multiple-versions = "warn"` nel brief è
corretto così com'è — sono conseguenza fisiologica di un albero con
`testcontainers`/`bollard` (usati solo in dev-dependency per i test) accanto
alle dipendenze di produzione, non un problema da risolvere in questo task.

## Verifiche eseguite in locale (job `backend` e `frontend`)

Ambiente: nessun Docker disponibile in questa sandbox (confermato:
`docker ps` fallisce con "cannot connect to the Docker daemon"), ma un
Postgres locale è raggiungibile su `127.0.0.1:5432` — uso quindi
`KEEPPIX_TEST_DATABASE_URL` **solo per l'esecuzione locale**, esattamente
come previsto da P4; questa variabile **non** è nel workflow committato.

Frontend buildato **prima** della suite Rust, come richiesto, così che i
test di `crates/keeppix-server/tests/embed.rs` non si auto-saltino:

```
$ cd frontend && npm ci
added 351 packages ... found 0 vulnerabilities

$ npx vue-tsc --noEmit
(nessun output — nessun errore di tipo)

$ npx vitest run
 Test Files  3 passed (3)
      Tests  8 passed (8)

$ npm run build
✓ 60 modules transformed.
dist/assets/index-CumzRq_k.js      202.51 kB │ gzip: 74.60 kB
✓ built in 528ms
```

Budget bundle, calcolato esattamente come nello step CI (concatenazione gzip
di tutti i `.js` in `dist/assets`, non somma dei singoli valori riportati da
Vite):

```
$ SIZE=$(find dist/assets -name '*.js' -exec gzip -c {} \; | wc -c)
bundle gzip: 76893 byte (budget 153600)
```

~77 KB gzip, in linea con la stima del preflight, ben sotto il tetto di
150 KB (153600 byte).

Poi, con `frontend/dist` presente:

```
$ export KEEPPIX_TEST_DATABASE_URL="postgres://keeppix:keeppix@127.0.0.1:5432/postgres"
$ cargo fmt --all --check
(nessun output — nessuna differenza)

$ cargo clippy --workspace --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.28s
(nessun warning)

$ cargo test --workspace -- --test-threads=1
```

Riepilogo per crate/file di test (tutti `ok`, 0 falliti, 0 ignorati):

| suite | esito |
|---|---|
| keeppix-api (unit) | 1 passed |
| tests/auth.rs | 17 passed |
| tests/health.rs | 3 passed |
| tests/openapi.rs | 6 passed |
| keeppix-db tests/migrations.rs | 7 passed |
| keeppix-db tests/sessions.rs | 14 passed |
| keeppix-db tests/settings.rs | 6 passed |
| keeppix-db tests/users.rs | 12 passed |
| keeppix-domain (unit) | 22 passed |
| keeppix-server tests/config.rs | 4 passed |
| keeppix-server tests/embed.rs | **4 passed** (non saltato: `frontend/dist` presente) |

Totale: **96 test, 0 falliti**. Confermato che `embed.rs` ha eseguito i 4
test reali (`index_is_served_at_root`, `client_routes_fall_back_to_index`,
`api_paths_never_fall_back_to_index`, `assets_are_served_as_immutable`) e
non li ha saltati — l'output non contiene la riga
`"frontend/dist assente: test saltato"`.

```
$ git diff --exit-code docs/api/openapi.json
(exit 0 — nessuna differenza, albero pulito)
```

Confermata la sequenza descritta in P7: `cargo test` include
`openapi_snapshot_matches_the_committed_file`, che fallisce se lo snapshot
diverge; il `git diff` successivo è la seconda rete e qui non ha trovato
nulla da segnalare.

Verifica extra non richiesta esplicitamente ma utile e a costo zero:
`docker compose config` (puramente client-side, non richiede il daemon) è
stato eseguito su `compose.yaml` e ha restituito exit 0 — sintassi Compose
valida.

## Cosa NON è stato verificato, e perché

- **Job `image` di `ci.yml`** (`docker/build-push-action` sul `Dockerfile`
  del repository): non eseguibile in questa sandbox, il pull delle immagini
  di base è bloccato dalla policy di egress. Non è un problema transitorio,
  non ho ritentato.
- **Step 6 del brief / verifica su GitHub Actions**: la CI gira solo dopo il
  push sul branch remoto, che è compito del controller, non mio. Non ho
  aperto né posso aprire la pagina Actions del repository.
- **`release.yml` nella sua interezza**: nessuna parte di questo workflow è
  eseguibile localmente (richiede tag `v*` o cron, push su `ghcr.io`, build
  multi-arch con QEMU, firma cosign). Verificato solo per lettura/coerenza
  con `ci.yml` e con le convenzioni GitHub Actions correnti.

**Non dichiaro in nessun punto di questo report che la CI su GitHub è
verde: non lo so**, perché non è mai girata. Ho verificato solo ciò che è
eseguibile in locale, elencato sopra.

## File creati

- `/home/user/Keeppix/.github/workflows/ci.yml`
- `/home/user/Keeppix/.github/workflows/release.yml`
- `/home/user/Keeppix/deny.toml`

Nessun file fuori da questi tre è stato toccato. `crates/`, `frontend/`,
`docs/`, `Dockerfile`, `compose.yaml` non sono stati modificati (verificato
con `git status --porcelain`: solo `.github/` e `deny.toml` risultano non
tracciati prima del commit).

## Auto-revisione

- Completezza: i tre file coprono tutto quanto richiesto dal brief, con le
  quattro correzioni del preflight applicate. Il commento su
  `KEEPPIX_TEST_DATABASE_URL` richiesto da P4 è presente nello step "Test"
  del job `backend`.
- Qualità: nessuna modifica strutturale oltre a quanto necessario; niente
  job o step aggiuntivi non richiesti.
- Disciplina: non ho ampliato la lista licenze oltre a quanto il comando
  richiedeva effettivamente (due sole voci aggiunte/corrette, entrambe
  motivate sopra). Non ho toccato `crates/`, `frontend/`, `docs/`.
- Test: suite Rust e frontend eseguite realmente, non assunte verdi;
  l'output è pulito (nessun warning clippy, nessuna differenza fmt, nessun
  test saltato per assenza di build frontend).

## Preoccupazioni

Nessuna preoccupazione bloccante. Due punti da tenere presente per chi
seguirà la CI dopo il push:

1. Il job `image` e l'intero `release.yml` non sono mai stati eseguiti in
   alcun ambiente da me controllato: la prima esecuzione reale sarà quella
   su GitHub Actions dopo il push del controller. Vale la pena osservarne
   l'esito con attenzione particolare, specialmente il passaggio
   `cosign sign` in `release.yml` (dipende da OIDC/`id-token: write`, mai
   testato qui).
2. I warning `duplicate` di `cargo-deny bans` (più versioni della stessa
   crate, es. `windows_x86_64_msvc`, `syn`, `base64`) sono benigni e attesi
   con `multiple-versions = "warn"`, ma è bene sapere che sono presenti fin
   da subito: se in futuro qualcuno alza la severità a `deny`, il job
   `audit` inizierà a fallire per motivi indipendenti dal codice applicativo
   (sono causati soprattutto da `testcontainers`/`bollard`, usati solo nei
   test).
