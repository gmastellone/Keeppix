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

---

## Fix round 1/5 — risposta alla review

Review completa: `.superpowers/sdd/2026-08-13-keeppix-fase-0/task-15-review.md`.
Esito: 2 Critical, 1 Important, 2 Minor. Nessun problema su `deny.toml`
(validato empiricamente dal reviewer, nessuna modifica in questo round).

### Critical 1 — il job `backend` non compila (frontend/backend non condividono `dist/`)

**Diagnosi confermata**, non solo accettata: ho riprodotto io stesso il
fallimento prima di correggere.

```
$ rm -rf frontend/dist
$ cargo check -p keeppix-server
error[E0599]: no function or associated item named `get` found for struct `Assets`
  --> crates/keeppix-server/src/embed.rs:33:19
...
error: could not compile `keeppix-server` (lib) due to 3 previous errors
```

**Scelta:** ho costruito il frontend **dentro** il job `backend`
(`actions/setup-node` + `npm ci` + `npm run build`), non un artifact
condiviso fra `backend` e `frontend`. Motivo: la review stessa segnala che è
l'opzione "più semplice e non accoppia i job" — vero, e in questo repository
il costo del doppio build (~30 s di npm install + build) è trascurabile
rispetto al beneficio di **non** introdurre una dipendenza `needs:` fra i
due job (che serializzerebbe l'esecuzione, oggi in parallelo) né la
complessità aggiuntiva di `actions/upload-artifact`/`download-artifact` con
relativa gestione di retention/nome. Il commento aggiunto nel workflow
spiega esplicitamente perché la build è duplicata e non condivisa.

Posizionata **prima** del toolchain Rust e di `Swatinem/rust-cache`, così
che compaia per prima nell'ordine di lettura del job e sia impossibile
scambiarla per uno step opzionale.

**Prova che la sequenza scritta produce davvero `frontend/dist` prima della
compilazione del backend** (non dedotta, eseguita nell'ordine esatto del
job aggiornato):

```
$ rm -rf frontend/dist && ls frontend/dist
ls: cannot access '/home/user/Keeppix/frontend/dist': No such file or directory

$ cd frontend && npm ci && npm run build
...
dist/assets/index-CumzRq_k.js      202.51 kB │ gzip: 74.60 kB
✓ built in 474ms

$ cd .. && cargo fmt --all --check
(nessun output, exit 0)

$ cargo clippy --workspace --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.33s
(nessun warning, exit 0 — compila, perché frontend/dist ora esiste)

$ export KEEPPIX_TEST_DATABASE_URL="postgres://keeppix:keeppix@127.0.0.1:5432/postgres"
$ cargo test --workspace -- --test-threads=1
...
     Running tests/embed.rs (target/debug/deps/embed-84d4bb8e498d4b35)
running 4 tests
test api_paths_never_fall_back_to_index ... ok
test assets_are_served_as_immutable ... ok
test client_routes_fall_back_to_index ... ok
test index_is_served_at_root ... ok
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

96/96 test verdi in totale (stesso conteggio del primo giro), i 4 test di
`embed.rs` eseguiti realmente (non saltati — nessuna riga "test saltato"
nell'output). `git diff --exit-code docs/api/openapi.json` → exit 0.

Questa è la prova richiesta esplicitamente dal contratto del round: la
sequenza scritta nel workflow, eseguita da capo con `dist/` assente
all'inizio, produce `frontend/dist` **prima** di ogni comando `cargo`.

### Critical 2 — `cosign sign` puntava a un repository con case sbagliato

**Non verificabile end-to-end qui**, come previsto dal contratto del round:
richiede un push reale su `ghcr.io` con OIDC (`id-token: write`), impossibile
in questa sandbox. Dichiaro esplicitamente che questa correzione non è stata
testata contro un registry reale.

**Base su cui ritengo corretta la forma scritta:** il difetto originale era
che `cosign sign` ricostruiva `ghcr.io/${{ github.repository }}` da zero,
senza passare dalla normalizzazione minuscola che `docker/metadata-action`
applica internamente al campo `images:` per generare i tag effettivamente
pubblicati. La correzione elimina la causa alla radice invece di aggirarla:
un singolo step calcola `IMAGE=ghcr.io/${GITHUB_REPOSITORY,,}` (bash,
lowercase parameter expansion, sintassi POSIX-compatibile e disponibile di
default sui runner Ubuntu con `bash` come shell predefinita) e lo scrive in
`$GITHUB_ENV`; sia `docker/metadata-action` (`images: ${{ env.IMAGE }}`) sia
`cosign sign` (`${{ env.IMAGE }}@${{ steps.build.outputs.digest }}`) leggono
la **stessa** variabile. Non c'è più una seconda ricostruzione indipendente
del nome immagine che possa disallinearsi dalla prima: il riferimento
firmato è per costruzione lo stesso repository path che `metadata-action`
usa per generare i tag pubblicati da `build-push-action`, non solo "anche
lui minuscolo per un'altra via".

Verificata solo l'espansione bash in isolamento, con il nome reale del
repository (confermato via `git remote -v`: `gmastellone/Keeppix`):

```
$ GITHUB_REPOSITORY="gmastellone/Keeppix"; echo "ghcr.io/${GITHUB_REPOSITORY,,}"
ghcr.io/gmastellone/keeppix
```

Coerente con quanto il reviewer ha già confermato (fuori diff) essere il
path realmente pubblicato da `metadata-action`. Il digest
(`steps.build.outputs.digest`) non è toccato dal fix: proviene invariato da
`docker/build-push-action`, unico produttore di quel valore.

**Resta non verificato**, e lo dichiaro: che `cosign sign --yes` completi
con successo contro un `ghcr.io` reale con le credenziali OIDC del workflow
— nessuna parte di questo passaggio è eseguibile in locale.

### Important — budget bundle: misurava tutto `dist/assets/*.js`, inclusi i chunk lazy

Non mi sono fermato a documentare: la correzione costava poche righe ed era
a basso rischio, quindi l'ho applicata.

**Approccio:** `dist/index.html` referenzia direttamente, per costruzione di
Vite, solo gli asset che il browser carica al primo render — lo script
d'ingresso (`<script type="module" src="...">`) e il foglio di stile
(`<link rel="stylesheet" href="...">`). I chunk lazy per-rotta (creati dagli
`import()` dinamici in `frontend/src/router.ts:8-10`) non compaiono in
`index.html`: vengono richiesti dal router solo alla navigazione. Ho quindi
sostituito `find dist/assets -name '*.js'` con un'estrazione degli asset
referenziati in `dist/index.html` via `grep`, sommandone il gzip uno per
uno (stessa metodologia "somma di gzip indipendenti" dello script
originale, non un unico stream gzip concatenato — mantiene il numero
confrontabile con la baseline già misurata).

Non è un'euristica fragile: è il meccanismo con cui Vite inietta gli entry
point in `index.html` in ogni build di produzione, non un'assunzione su
nomi di file o struttura di cartelle.

Verificato sulla build reale, con `dist/` ricostruita da zero in questo
stesso round:

```
$ grep -oE '(src|href)="/assets/[^"]+\.(js|css)"' dist/index.html
src="/assets/index-CumzRq_k.js"
href="/assets/index-5Zgkpkbu.css"

$ # somma gzip di ciascuno dei due file sopra
bundle iniziale gzip: 76339 byte (budget 153600)
```

76.339 byte (contro 76.893 dello script precedente): la differenza è
`- 6 chunk lazy per-rotta (~2,6 KB) + il CSS d'ingresso (2.746 byte gzip,
prima ignorato)` — coerente con l'analisi della review. Il beneficio
strutturale è che un chunk lazy grande in una fase futura (es. MapLibre)
non farà mai fallire questo step, perché non compare in `index.html`.

Il commento nello step spiega esplicitamente la definizione di "iniziale" e
cita il design (§10.9) per chi lo rileggerà in una fase futura.

### Minor 1 — nessun controllo che gli asset esistano: risolto come parte del fix Important

La riscrittura sopra include due controlli espliciti che il vecchio script
non aveva:

- `test -f dist/index.html || { echo "::error::..."; exit 1; }` prima di
  tutto — sostituisce il controllo su `dist/assets` suggerito dalla review
  con un controllo equivalente più a monte (se `index.html` manca, l'intera
  build è mancante, non solo la cartella assets);
- `test -f "$FILE" || { echo "::error::..."; exit 1; }` per **ogni** asset
  referenziato, prima di provare a gzipparlo.

**Prova che questi controlli falliscono rumorosamente invece di passare in
silenzio a 0 byte** (riprodotta in due scenari isolati, non nel repository):

```
# scenario 1: dist/index.html assente
$ test -f dist/index.html || { echo "::error::dist/index.html non trovato"; exit 1; }
::error::dist/index.html non trovato
(exit 1)

# scenario 2: index.html presente, ma l'asset che referenzia è assente
$ echo '<script src="/assets/index-XXXX.js"></script>' > dist/index.html
$ ...
::error::asset referenziato in index.html assente: dist/assets/index-XXXX.js
(exit 1)
```

Entrambi gli scenari terminano con `exit 1` e un messaggio esplicito, non
con un falso "sotto budget" silenzioso.

### Minor 2 — il budget non contava il CSS: risolto come parte dello stesso fix

La stessa riscrittura include `\.(js|css)` nel pattern di estrazione: il
foglio di stile d'ingresso (`dist/assets/index-*.css`, 2.746 byte gzip nella
build reale) è ora incluso nella somma. Non serviva un fix separato: era la
stessa manciata di righe del fix Important.

### Verifiche eseguite in questo round (riepilogo)

Tutte le verifiche del primo giro sono state ripetute da zero dopo le
modifiche, non riutilizzate dalla sessione precedente:

- `rm -rf frontend/dist` → `cargo check -p keeppix-server` fallisce
  (riproduzione del Critical 1, prova che la diagnosi è corretta prima del
  fix).
- Sequenza corretta del job `backend` eseguita da capo (`npm ci` + `npm run
  build` prima di `cargo fmt`/`clippy`/`test`): tutti gli step verdi, 96/96
  test, `embed.rs` eseguito realmente (4/4), `git diff` su `openapi.json`
  pulito.
- `npx vue-tsc --noEmit` → pulito. `npx vitest run` → 8/8.
- Script di budget riscritto, eseguito sulla build reale: 76.339 byte gzip,
  sotto i 153.600 del tetto.
- Due scenari di fallimento del nuovo script di budget (index.html assente,
  asset referenziato ma mancante) riprodotti in isolamento: entrambi
  falliscono con `exit 1` e messaggio esplicito.
- Espansione bash `${GITHUB_REPOSITORY,,}` verificata in isolamento con il
  nome reale del repository (`gmastellone/Keeppix` → `gmastellone/keeppix`).

### Non verificabile in questo round (dichiarato esplicitamente)

- **Job `image`** di `ci.yml`: stesso motivo del giro precedente, policy di
  egress.
- **`release.yml` per intero, incluso il fix del Critical 2**: nessuna parte
  eseguibile in locale. In particolare **non ho potuto confermare che
  `cosign sign` completi con successo** contro un `ghcr.io` reale — ho
  verificato solo che il riferimento immagine che il comando costruirebbe è,
  per costruzione della variabile condivisa, identico a quello che
  `metadata-action`/`build-push-action` pubblicano. Non affermo che la CI
  sia verde: lo saprà solo la prima esecuzione reale su GitHub Actions dopo
  il push del controller.

### File toccati in questo round

- `/home/user/Keeppix/.github/workflows/ci.yml` (Critical 1, Important,
  Minor 1, Minor 2)
- `/home/user/Keeppix/.github/workflows/release.yml` (Critical 2)

`deny.toml` non modificato in questo round (nessun problema segnalato lì).
Nessun file fuori da `.github/` toccato.
