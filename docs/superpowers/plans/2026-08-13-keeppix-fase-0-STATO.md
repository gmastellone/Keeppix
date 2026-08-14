# Fase 0 — stato di avanzamento e consegna

**Aggiornato:** 2026-08-14, dopo la review finale del branch e la sua fix wave
**Piano:** [`2026-08-13-keeppix-fase-0.md`](2026-08-13-keeppix-fase-0.md)
**Spec:** [`../specs/2026-08-13-keeppix-design.md`](../specs/2026-08-13-keeppix-design.md)
**Roadmap:** [`2026-08-13-keeppix-roadmap.md`](2026-08-13-keeppix-roadmap.md)
**Branch:** `fase-0`, da `main @ 7b38c1d`

Questo documento è la **consegna della Fase 0**: qui c'è tutto ciò che serve a
riprendere il lavoro da un'altra macchina o da un'altra sessione, senza leggere
il ledger. Il ledger di lavoro completo vive in
`.superpowers/sdd/2026-08-13-keeppix-fase-0/` — in questo repository è
versionato di proposito (vedi **R11**), ma è lungo e cronologico; questo file è
la sintesi che resta vera.

## Metodo di esecuzione

Un subagent implementatore per task, con brief estratto dal piano; a seguire una
review del task (conformità allo spec + qualità); i finding Critical/Important
entrano in un fix round, i Minor vengono differiti e annotati qui. Le review dei
task più delicati (7 e 10) sono state fatte su un modello più capace.

**Nessun finding si è rivelato un falso positivo.** Dodici task su quindici
hanno richiesto almeno un giro di correzione, e la review finale dell'intero
branch ne ha prodotto altri quindici, tutti corretti in un'unica fix wave.

## Avanzamento

**Fase 0 completa.** Tutti i 15 task sono chiusi, la review finale dell'intero
branch è stata eseguita e la sua fix wave è stata applicata (vedi
«Review finale del branch» più sotto).

| # | Task | Stato | Commit finale |
|---|---|---|---|
| 1 | Workspace e toolchain | ✅ review pulita | `fcd5368` |
| 2 | Tipi di dominio | ✅ dopo 1 fix round | `c0382a0` |
| 3 | Hashing Argon2id | ✅ review pulita | `db82b8c` |
| 4 | DB, migrazioni, harness | ✅ review pulita | `ada768e` |
| 5 | `UserRepo` | ✅ dopo 1 fix round | `78be6fc` |
| 6 | Token di sessione e segreti | ✅ dopo 1 fix round | `362af6e` |
| 7 | `SessionRepo` | ✅ dopo 1 fix round | `8835447` |
| 8 | Config, telemetria, CLI | ✅ dopo 1 fix round | `74890b4` |
| 9 | Stato, problem+json, extractor | ✅ dopo 1 fix round | `a040007` |
| 10 | Setup e autenticazione | ✅ dopo 2 fix round | `90f8b82` |
| 11 | Specifica `OpenAPI` | ✅ dopo 1 fix round | `adca7c6` |
| 12 | Frontend | ✅ dopo 2 fix (cookie `__Host-`, `signOut`) | `c6b82f0` |
| 13 | Frontend incorporato | ✅ dopo 1 fix round | `e1f72b3` |
| 14 | Immagine Docker e compose | ✅ dopo 1 fix round | `f6d1e34` |
| 15 | Integrazione continua | ✅ dopo 1 fix round | `91904fb` |
| — | Fix wave della review finale | ✅ 12 commit | `23f9964`…`43f366a` |

Stato attuale della suite: **107 esecuzioni di test Rust** tutte verdi
(22 domain, 41 db, 36 api, 8 server; i 3 unit test dell'harness di `keeppix-db`
girano una volta per binario di integrazione, cioè quattro volte) più **9 test
vitest** nel frontend. Erano 96 esecuzioni Rust prima della fix wave;
`cargo clippy --workspace --all-targets -- -D warnings` pulito;
`cargo fmt --check` pulito; `cargo build --workspace` verde;
`cargo deny check advisories bans licenses` verde;
`npx vue-tsc --noEmit`, `npx vitest run` (9) e `npx eslint .` puliti.

## Verifica Docker, eseguita

I nove comandi che i Task 14 e 15 avevano dichiarato non verificabili sono stati
eseguiti su una macchina con Docker attivo. Esito:

- `docker build -t keeppix:dev .` riesce. Immagine **58,6 MB** (il piano stimava
  sotto 100 MB), multi-stage con il frontend compilato dentro.
- **Nessuna shell**: `/bin/sh` e `/bin/bash` entrambi assenti (exit 127). Il
  binario risponde: `keeppix 0.1.0`. Utente `nonroot:nonroot`, `HEALTHCHECK`
  dichiarato come `["CMD","/usr/local/bin/keeppix","healthcheck"]`.
- `docker compose --profile bundled up -d`: `db` healthy, applicazione pronta in
  **3 s**; entrambi i container `(healthy)` dopo il periodo di start.
- `/health` → `{"status":"ok","version":"0.1.0"}`;
  `/api/v1/setup/status` → `{"initialised":false}`.
- Flusso completo contro il container: setup `201` con cookie, `/auth/me` `200`,
  secondo setup `409 keeppix/already-initialised`, logout `204`, `/auth/me`
  `401 keeppix/unauthenticated`, login `200` con username case-insensitive.
- Frontend servito dal binario: `GET /` `200 text/html`, `<title>Keeppix</title>`,
  asset con hash; fallback SPA su `/login` `200`.
- **Header di sicurezza presenti sul 404 API** (`/api/v1/nope`): il fix R5 tiene
  in produzione, non solo nei test.
- Persistenza: `down` + `up` → `{"initialised":true}` e login `200` con le
  credenziali precedenti.
- Verifica empirica del superamento di **R7**: il container emette `Secure` sul
  cookie `__Host-kpx_session` anche su HTTP in chiaro verso `127.0.0.1`, e il
  client lo accetta e lo rimanda.

Difetto trovato da questa verifica e **corretto** nella fix wave:
`docker compose down` senza `--profile bundled` lasciava il database in
esecuzione, e `docs/DEPLOY.md` non documentava affatto come fermare lo stack.
Ora c'è una sezione «Arresto».

## Cosa resta prima del merge

Due criteri di completamento sono **verifiche mai eseguite**, non difetti di
codice, e non sono spuntabili da questa sessione:

1. **La CI non è mai girata.** `.github/workflows/ci.yml` si attiva su push a
   `main` e su pull request: il lavoro vive su `fase-0` e non esiste una PR,
   quindi nessuno dei quattro job è mai stato eseguito su un runner. Aprire la PR
   `fase-0 → main` e far girare la CI **prima** di mergiare, non come effetto del
   merge. Rischi che solo un run reale intercetta: cache npm con
   `cache-dependency-path`, disponibilità di `node 24` e
   `dtolnay/rust-toolchain@1.88.0`, pull di `postgis/postgis:17-3.5` sul runner,
   `cargo-deny-action@v2`, e il job `image` che costruisce il Dockerfile su un
   runner per la prima volta.
2. **Il flusso in browser reale non è stato rieseguito dopo il fix del cookie.**
   Il criterio è «da browser: setup del primo admin, logout, login, ricarica
   pagina con sessione persistente». La suite **non può** coprirlo:
   `cookie_store`/`reqwest` non implementa la validazione del prefisso
   `__Host-`, quindi nessun test osserva l'accettazione da parte di un client
   conforme (vedi il commento su `assert_host_prefix_attributes` in
   `crates/keeppix-api/tests/auth.rs`). La verifica con `curl` contro il
   container mostra l'header corretto, non l'accettazione da browser. Da
   eseguire a mano una volta; una rete permanente sarebbe un piccolo test
   Playwright in `frontend/`.

La fix wave ha inoltre toccato la CSP (rimozione di `style-src 'unsafe-inline'`):
la verifica strutturale è stata fatta (il bundle di Vite non ha stili inline né
inietta `<style>` a runtime), ma la conferma visiva conviene farla nella stessa
sessione del punto 2.

## Come si esegue la suite

I test di integrazione vogliono un Postgres reale. Di norma se lo avviano da
soli con testcontainers e non serve fare nulla:

```bash
cd frontend && npm ci && npm run build   # obbligatorio: senza dist/ il backend non compila
cd .. && cargo test --workspace -- --test-threads=1
```

`frontend/dist` **non è un prerequisito dei test ma della compilazione**:
`rust-embed` incorpora quella cartella a tempo di compilazione, quindi senza di
essa `keeppix-server` non compila affatto. La guardia `frontend_built()` in
`crates/keeppix-server/tests/embed.rs` resta legittima solo nella finestra
«compilato con `dist`, eseguito dopo averla cancellata».

`--test-threads=1` serve ai quattro test di `keeppix-server/tests/config.rs`,
che manipolano l'ambiente di processo e non tollerano il parallelismo: è un
vincolo pre-esistente, documentato nel file stesso, non una regressione.

Dove il registry delle immagini non è raggiungibile — è il caso della sessione
cloud in cui sono stati fatti i commit dei Task 1-15, vedi **R9** — si punta la
suite a un Postgres già in ascolto:

```bash
export KEEPPIX_TEST_DATABASE_URL="postgres://utente:password@127.0.0.1:5432/postgres"
cargo test --workspace -- --test-threads=1
```

**Su quel percorso gli harness non eliminano i database usa-e-getta che creano**
(difetto noto, differito). Il costo è già stato misurato: 1512 database orfani
hanno riempito un filesystem e fatto fallire una suite in un task non correlato.
Pulizia:

```bash
psql "$KEEPPIX_TEST_DATABASE_URL" -tAc \
  "SELECT format('DROP DATABASE %I', datname) FROM pg_database \
    WHERE datname LIKE 'keeppix_test_%'" | psql "$KEEPPIX_TEST_DATABASE_URL"
```

Il percorso predefinito con testcontainers non perde niente: il container muore
con il test.

Un test salta da sé in una condizione sola: `a_database_outage_is_a_503_not_a_401`
(in `crates/keeppix-api/tests/auth.rs`) spegne il container Postgres sotto il
server, e sul percorso `KEEPPIX_TEST_DATABASE_URL` il server è condiviso con gli
altri test, quindi non può spegnerlo. Stampa il motivo ed esce verde.

## Decisioni prese durante l'esecuzione

Ognuna è una scelta fatta senza poter consultare l'autore del piano. Sono
elencate perché siano riesaminabili.

**R1 — Branch invece di worktree.**
Il repository non aveva un remote quando l'esecuzione è iniziata, quindi la
creazione di un worktree con base `origin/<default>` sarebbe fallita. Il vincolo
reale era "non implementare su `main`", che un branch soddisfa.
*Se sbagliato:* si sposta il branch in un worktree con un comando.

**R2 — Toolchain 1.88.0 invece di 1.85.0.**
Il piano fissava 1.85, ma il codice che il piano stesso specifica per i Task 5 e
8 usa i let-chain (`if let ... && ...`), stabili solo da Rust 1.88: con 1.85 quei
due task non compilano. Alzata la toolchain in `rust-toolchain.toml` e in
`rust-version`. **Il Task 14 (Dockerfile) e il Task 15 (CI) devono usare 1.88,
non 1.85 come scritto nel piano.**
*Se sbagliato:* nessun costo noto.

**R3 — `DbError::Corrupted` al posto di `Migration` per dati malformati.**
Il fix round del Task 5 ha introdotto una variante dedicata ai dati già presenti
in database che il codice non riesce a interpretare, distinta da "una migrazione
è fallita". Il Task 6 la prescriveva ancora come `Migration`; ha vinto la
coerenza della tassonomia su cui si fa triage.
*Se sbagliato:* nessuno, entrambe cadono nel ramo generico del `From<DbError>`.
*Esito:* la review finale ha trovato il ruling **applicato a metà** —
`sessions.rs` degradava ancora un ruolo sconosciuto a `SystemRole::User`. Ora i
due siti coincidono.

**R4 — sqlx con le forme funzione, non le macro.**
Lo spec §9.5 dichiara "query verificate a compile-time", ma tutto il codice del
piano usa `sqlx::query(...)` (verificata a runtime), non `sqlx::query!`. Gli step
11-12 del Task 4 (`cargo sqlx prepare`, cache `.sqlx/`) avrebbero prodotto una
cache vuota. Si tengono le forme funzione con parametri bound: la proprietà di
sicurezza che conta — query parametrizzate, mai concatenazione — è pienamente
soddisfatta, e la verifica dello schema è coperta dai test di integrazione contro
Postgres reale. **Il Task 14 (Dockerfile) e il Task 15 (CI) devono rimuovere ogni
riferimento a `SQLX_OFFLINE` e `.sqlx/`.**
*Se sbagliato:* un refuso in un nome di colonna fallisce in test invece che in
build.
*Da riesaminare prima della Fase 1:* la compensazione dichiarata («lo schema è
verificato dai test di integrazione») vale solo per le query che i test
*eseguono*. Su 4 tabelle e ~12 query regge; la Fase 1 porta ~7 tabelle nuove,
join su `ltree` e aggregati della timeline. La scelta è fra
`#[derive(sqlx::FromRow)]` — che elimina le stringhe di colonna scritte a mano
mantenendo le forme funzione, costo quasi nullo — e `query_as!` con cache
`.sqlx/` committata, che reintrodurrebbe nel Dockerfile e in CI ciò che questo
ruling ha rimosso.

**R5 — Ordine di `.fallback()` in `common_layers`.**
In axum 0.8 `Router::fallback` sovrascrive il catch-all invece di fondersi con
quello già avvolto, e `.layer()` avvolge solo il fallback esistente al momento
della chiamata. Il piano metteva `.fallback(not_found)` **dopo** i `.layer(...)`,
con il risultato che ogni 404 usciva senza CSP, nosniff, referrer-policy e
permissions-policy. Spostato prima, con un commento che spiega il meccanismo.
**Il Task 13 ristruttura `common_layers`: deve mantenere quest'ordine.**
*Se sbagliato:* nessuno, la correzione è verificata da test su entrambe le rotte.
*Esito:* l'ordine è mantenuto nei quattro punti di montaggio e verificato per
mutazione su tutti e quattro, `mount()` compreso — la funzione che costruisce il
binario spedito. La fix wave ha aggiunto un quinto percorso con la stessa
trappola, `method_not_allowed_fallback`, che per lo stesso motivo va chiamato
dopo aver registrato le rotte.

**R6 — `dummy_hash()` con un hash Argon2id reale.**
Il piano usava una stringa PHC malformata per pareggiare i tempi di risposta
quando l'utente non esiste. Non funzionava: `verify_password` fallisce il
*parsing* e ritorna subito, senza eseguire Argon2, lasciando intatto il segnale
temporale che doveva mascherare. Sostituita con un hash valido, e il test che lo
inchioda asserisce il successo del parsing in positivo.
*Se sbagliato:* l'esistenza di un utente resta deducibile dai tempi di risposta.

**R7 — `should_be_secure` corretto in loco, non spostato in configurazione.**
La funzione faceva prefix-match sull'header `Host`, controllato dal client, per
cui `localhost.evil.com` passava per locale. Corretta con match esatto dopo lo
strip della porta, inclusi i letterali IPv6. La soluzione architetturalmente
migliore — spostare la decisione in `AppState`/`Config` — richiedeva di infilare
un campo lungo `Config → AppState → entrambi i router → funzioni cookie`, cioè
una modifica di interfaccia sproporzionata rispetto a un rischio che il prefisso
`__Host-` già smorza (un cookie non-`Secure` con quel prefisso viene rifiutato
dal browser, quindi il login fallisce invece di far trapelare il cookie).
**Superato e chiuso, non differito.** Il fix fuori piano `b3b1b32` (Task 12) ha
**cancellato la funzione**: `Secure` è ora incondizionato. Il difetto vero l'ha
trovato un browser reale — un cookie `__Host-` privo di `Secure` viene scartato
*per intero* da un browser conforme, anche su loopback in chiaro, quindi il
design condizionale rompeva il login in sviluppo. Il prefisso esige la presenza
letterale dell'attributo indipendentemente dal trasporto, mentre separatamente i
browser esentano le origini loopback dal requisito che un cookie `Secure` viaggi
su TLS. Confermato empiricamente dalla verifica Docker. **Non va riesumato**, e
il difetto «`should_be_secure` case-sensitive» non esiste più.

**R8 — Il fix del Task 7 committato dal controller.**
L'implementatore ha completato il fix ma è morto per limite di sessione prima di
verificare e committare. Il controller ha eseguito la verifica (11/11 test,
clippy pulito) e committato il codice esattamente come lasciato. Stessa cosa per
il fix round del Task 10, interrotto a lavoro concluso ma prima del commit.
*Se sbagliato:* il codice era comunque coperto dalla re-review successiva.

**R9 — Via d'uscita negli harness di test verso un Postgres esistente.**
Nell'ambiente cloud il pull di `postgis/postgis` è bloccato dalla policy di
egress (403 al CONNECT verso `production.cloudfront.docker.com`), quindi
testcontainers non è utilizzabile e l'intera suite di integrazione non è
eseguibile — il primo reviewer ha dovuto patchare gli harness a mano e
ripristinarli. Se `KEEPPIX_TEST_DATABASE_URL` è impostata, i due harness usano
il server già in ascolto a quell'indirizzo e creano un database vergine per
test, con lo stesso isolamento del container; senza la variabile nulla cambia e
il container resta la via predefinita per CI e sviluppo.
*Se sbagliato:* gli harness hanno un ramo in più e la riscrittura dell'URL è
duplicata nei due crate, con unit test solo nella copia di `keeppix-db`.

**R10 — Push su `fase-0`.**
L'harness della sessione cloud impone di default un branch `claude/...`;
l'utente ha chiesto esplicitamente che il lavoro finisca sul branch della fase.
Il branch `claude/keeppix-fase-0-4c0lku` è stato cancellato dopo aver verificato
che i suoi commit fossero tutti in `fase-0`.
*Se sbagliato:* nessun costo, i due branch avevano lo stesso contenuto.

**R11 — Il workspace `.superpowers/` non viene cancellato.**
La skill `subagent-driven-development` impone `rm -rf <workspace>` alla fine,
perché «la storia di git è il record». Questo repository ha deliberatamente fatto
la scelta opposta — `.gitignore` documenta che `.superpowers/` **non** è ignorato
e i file sono force-added — e l'utente ha chiesto esplicitamente di mantenerli.
Vince la scelta del repository.
*Se sbagliato:* ~600 KB di testo versionato. La sola sottrazione che il review
finale consiglia sono i 33 file `review-*.diff`, ricostruibili con `git diff A..B`;
report e ledger sono la parte di valore.

**R12 — Minor pre-giudicati nel dispatch della review del Task 11.**
Nel dispatch ho elencato i Minor già noti chiedendo di non ri-segnalarli. La
skill lo vieta («Do not pre-judge findings for the reviewer»): il rischio era che
il reviewer tacesse un difetto che avrebbe classificato più grave. Mitigato
invitandolo a contestare la classificazione; dai Task 12-15 si usano i template
canonici, che non contengono l'istruzione. Il review finale del branch ha
ricontrollato gli artefatti del Task 11 in proprio: il rischio non si è
materializzato.

**R13 — La metà server-side della difesa CSRF rimandata alla fix wave finale.**
Lo spec §9.5 chiede `SameSite=Lax` **più** `Content-Type: application/json`
**più** un header custom sulle mutazioni. Il Task 12 ha implementato la metà
client-side (`apiFetch` invia `x-keeppix-client`), ma nessun task del backend
verificava quell'header, e infilarlo nel Task 12 — il cui elenco di file è tutto
sotto `frontend/` — avrebbe cambiato l'interfaccia del backend senza review
dedicata.
**Chiuso nella fix wave:** `crates/keeppix-api/src/csrf.rs` è il layer che
pretende l'header su POST/PUT/PATCH/DELETE dentro `/api/v1`, con
`403 keeppix/csrf-check-failed`. Il ruling ha funzionato: la superficie era di 4
rotte invece delle ~40 della Fase 1, e la metà client-side era già pronta.

## Review finale del branch e fix wave

Report completo: `.superpowers/sdd/2026-08-13-keeppix-fase-0/final-review-report.md`;
esito della fix wave: `final-fix-wave-report.md`, nella stessa cartella.

**Verdetto della review:** architettura solida, nessuna riscrittura necessaria
per la Fase 1. Gli invarianti portanti tengono e non per convenzione: `sqlx`
compare fra le `[dependencies]` solo di `keeppix-db` (in `keeppix-api` è solo
`[dev-dependencies]`, per il `CREATE DATABASE` dell'harness), quindi un handler
che provasse a scrivere una query **non compilerebbe**; l'unico
`AuthContext::user(...)` fuori dai test è dentro `SessionRepo::authenticate`;
l'ordine «fallback prima, `with_common_layers` dopo» è applicato in quattro punti
e verificato per mutazione su tutti e quattro.

### Corretto nella fix wave (15 voci)

| Voce | Cosa è cambiato |
|---|---|
| I1 | `CREATE EXTENSION postgis` nella migrazione `0001`, con il commento riscritto e un test che lo verifica. `postgis` non è *trusted* e richiede il superuser: farlo in Fase 4 su un Postgres gestito già popolato sarebbe stato impossibile. Modificare `0001` era gratis solo finché non esiste un rilascio |
| I2 | Le rejection integrate di axum sono dentro RFC 9457: `keeppix_api::Json<T>` (wrapper su `axum::Json` con rejection `Problem`) dà `415 keeppix/unsupported-media-type` e `keeppix/invalid-json` (400 sintassi, 422 forma), e `method_not_allowed_fallback` dà `405 keeppix/method-not-allowed`. È la singola cosa che più riduce il lavoro della Fase 1: ~20 rotte nuove useranno lo stesso stampo |
| I3 | `Cache-Control: private` nello stack comune, con `if_not_present` — **non** `overriding`, altrimenti gli asset hashati perderebbero `public, max-age=31536000, immutable` |
| I4 | ICU MessageFormat: **deroga registrata**, non implementato. Vedi la voce dedicata sotto |
| I5 | `DbError::Connection` → `503 keeppix/service-unavailable` con `Retry-After`, nei **due** siti che consultano una sessione (`extract.rs` e `refresh`), con la decisione in un'unica funzione `extract::session_problem`. Prima un riavvio di Postgres si presentava a tutti i client come «sessione scaduta» |
| I6 | `style-src 'unsafe-inline'` rimosso (la giustificazione era falsa: Vite estrae gli stili in un foglio esterno, e ciò che Vue imposta a runtime passa dal CSSOM, che la CSP non intercetta) e `Strict-Transport-Security: max-age=31536000; includeSubDomains` aggiunto. HSTS incondizionato non rompe l'uso in chiaro in LAN: un browser lo ignora quando arriva su HTTP (RFC 6797 §8.1). `preload` deliberatamente escluso |
| I7 | CSRF, metà server-side: vedi **R13** |
| I10 | Questo documento |
| M1 | `assert_security_headers` da tre copie a una, nel nuovo crate `keeppix-test-support`, e **asserisce davvero la CSP**: direttiva per direttiva, con confronto esatto e divieto di deroghe `unsafe-*`. Le tre copie usavano `is_some()`, quindi sostituire l'intera policy con `default-src *` lasciava la suite verde. Verificato per mutazione: ora fallisce 5 test |
| M5 / R3 | Ruolo sconosciuto in `sessions.rs` → `DbError::Corrupted`, come già in `users.rs` |
| M13 | Regola `[[bans.deny]]` in `deny.toml`: `keeppix-db` ha un'allowlist di dipendenti (`keeppix-api`, `keeppix-server`). Verificato in entrambi i versi — aggiungendo l'arco `media → db`, `cargo deny check bans` esce 2 con `error[banned]` |
| — | `interval()` usa `as_secs_f64()`: un TTL di 500 ms non è più `"0 seconds"`, cioè un token nato scaduto senza errore |
| — | `clear_env()` nei test di config deriva le chiavi dall'ambiente invece di elencarne quattro su sette: non può più restare indietro rispetto a `Config` |
| — | Il messaggio d'errore di `DATABASE_URL` è interamente in inglese (`es.` → `e.g.`) |
| — | `docs/DEPLOY.md` ha una sezione «Arresto» che documenta `--profile bundled down` e spiega perché il profilo serve anche per fermare |

### I4 — deroga registrata: i plurali usano la sintassi nativa di vue-i18n

Lo spec §10.10 chiede `vue-i18n` **con ICU MessageFormat**, motivandolo con
«plurali corretti». La fix wave ha scelto di **non** implementare ICU e di
rimuovere `@intlify/core-base`, che era dichiarato fra le `dependencies` di
produzione e non importato da nessuna parte — l'impronta di un tentativo
abbandonato.

Ragione: italiano e inglese hanno esattamente **due** categorie plurali CLDR
(`one`/`other`), che è precisamente ciò che la sintassi nativa `'una foto | {n} foto'`
esprime. Per le lingue spedite le due sintassi danno lo stesso risultato. ICU
richiederebbe una dipendenza runtime in più (`intl-messageformat`, ~25 KB gzip su
un budget di 150 KB di cui 77 già usati) più un `messageCompiler` custom da
mantenere, per zero differenze osservabili. Le chiavi plurali oggi sono **zero**.

**Quando riaprire:** alla prima lingua con più di due categorie plurali (russo,
polacco, arabo). Allora la scelta giusta è un compilatore ICU a **build time**
(`@intlify/unplugin-vue-i18n`), non a runtime. La decisione è documentata anche
in `frontend/src/i18n/index.ts`, dove la troverà chi scrive il primo plurale.

Nota a credito: il controllo CI sulle chiavi mancanti richiesto da §11.7 c'è ed è
buono (`i18n.spec.ts` confronta gli insiemi di chiavi nei due versi e verifica
che nessuna traduzione sia vuota).

### Deliberatamente differito, con la fase e la ragione

| Voce | Quando | Perché |
|---|---|---|
| `refresh`/`rotate` non ricontrollano `users.disabled_at` | **Fase 3** | Innocuo oggi: `authenticate` fa join su `disabled_at IS NULL`, quindi un account disabilitato non può *usare* nulla, può solo coniare token inerti. Diventa reale quando esisterà un percorso di disabilitazione da interfaccia, che è anche il posto dove va scritto il test «disabilitare un utente termina le sue sessioni» |
| Nessun rate limiting su `/auth/login` e `/api/v1/setup` | **Fase 3** | Spec §9.5 lo lega alla superficie pubblica, che nasce in Fase 3 coi link pubblici, e **è lo stesso middleware**: farlo prima significa scriverlo due volte. I ~100 ms di Argon2 danno un throttling incidentale a ~10 tentativi/s per connessione |
| `logout` risponde `204` anche se `revoke` fallisce | **Fase 3** | Deliberato; il frontend fa la cosa giusta (azzera lo stato locale e segnala `logoutError`). Il residuo — sessione viva lato server dopo un logout apparente — si chiude con la pagina «Dispositivi» (`/auth/devices`), e nel frattempo va nell'audit log, non in un `warn` |
| `sessions.ip` mai popolata | **Fase 3** | Serve all'audit log e va fatta insieme alla lettura di `X-Forwarded-For`, che richiede una configurazione «proxy fidati» che oggi non esiste. Popolarla con l'IP del proxy sarebbe peggio che lasciarla vuota |
| `map_unique_violation` scarta l'errore sqlx sottostante | **Fase 3** | Perdita di segnale di debug, non di correttezza. Serve dove «username preso» ed «email presa» sono messaggi distinti, cioè nella gestione utenti |
| Deadlock 40P01 di due replay concorrenti; re-login occasionale su retry di `refresh`; assenza di `Idempotency-Key` | **Fase 6** | **Sono un unico problema con un'unica soluzione già decisa nello spec** (§9.2, `Idempotency-Key` su tutte le mutazioni, motivato con «un'app mobile ritenta di continuo»), non tre sviste separate. Serve un client che ritenti in parallelo, cioè l'app mobile. L'esito di sicurezza regge già oggi: uno dei due replay aborta e la famiglia viene comunque revocata |
| `Password` non azzera il buffer in `Drop`, deriva `Clone` | **Fase 6** | Hardening reale ma di ordine inferiore: il plaintext vive nel corpo JSON, nel buffer di axum e nell'allocazione di serde molto prima di arrivare a `Password`. Azzerare solo l'ultimo anello dà un falso senso di completezza: va fatto con `zeroize` su tutta la catena, in un intervento unico |
| Gli harness non eliminano i database usa-e-getta sul percorso `KEEPPIX_TEST_DATABASE_URL` | **Differito, documentato** | Il percorso predefinito (testcontainers) non perde nulla, e ora la macchina di sviluppo ha Docker: quel ramo è quasi sempre inattivo. Il comando di pulizia è nella sezione «Come si esegue la suite» |
| I8 — la ricostruzione settimanale aggiorna solo `latest`, non il tag `:1` che gli utenti installano; il job `publish` non esegue test né lint | **Prima della prima release** | Su un evento `schedule` non esistono tag git, quindi i pattern `semver` non producono nulla. Difetto latente: in Fase 0 non c'è ffmpeg e non è stata pubblicata alcuna immagine |
| I9 — `compose.yaml` compila da sorgente invece di scaricare l'immagine | **Dopo la prima release** | La scelta era giusta quando è stata fatta: nessuna immagine è pubblicata, e un compose che punta a un registro vuoto non funzionerebbe. Da rivedere ora che `release.yml` esiste, perché il percorso documentato chiede a un Raspberry Pi 5 di compilare il workspace in release con LTO monolitica |
| `Password::parse` limita a 1024 *caratteri*, non byte | **Fase 1**, con azione | Nessun difetto oggi. Il valore della nota è l'avvertimento: i limiti di dimensione HTTP della Fase 1 (upload, batch) vanno pensati in **byte**. Da riportare nel piano della Fase 1 |
| `with_database` duplicata fra i due harness, con unit test in una copia sola | **Fase 1** | Si chiude spostandola in `keeppix-test-support`, che ora esiste |
| `sha2 = "0.11"` duplica l'albero (0.10.9 via sqlx/argon2); `reqwest` di test con feature `rustls` | **Fase 1** | Toccano solo il tempo di build. Da rivedere presto: la Fase 1 aggiunge molti test HTTP e il tempo di CI è un costo ricorrente |
| Il fallback SPA inghiottirà `/media/*` e `/dav/*` | **Fase 1**, da fare subito | `embed.rs` esclude dal fallback solo i percorsi che iniziano per `api/`: una miniatura mancante restituirebbe `index.html` con `200` a un tag `<img>`, e un client WebDAV riceverebbe HTML dove attende XML. Costa due righe adesso |
| `Auth::from_request_parts` fa una query per ogni richiesta autenticata | **Fase 1**, da progettare | Irrilevante oggi (<1 ms), non in una griglia da centinaia di richieste. La buona notizia: quello è **l'unico** punto da cui passa l'autenticazione, quindi la cache `moka` prevista da §9.4 si inserisce lì e da nessun'altra parte — con invalidazione esplicita in `revoke`/`rotate`, o TTL corto |
| `Db::ping()` usata solo da un test; `healthcheck` fa un bare TCP connect | **Fase 1** | Un container col pool esaurito resta `healthy` per sempre. Da far colpire `/health` includendo `ping()` |
| `M3` — tre blocchi identici di 8 `row.try_get` in `users.rs` | **Fase 1**, decisione su R4 | Prima di iniziare la Fase 1, scegliere fra `#[derive(sqlx::FromRow)]` (elimina le stringhe di colonna scritte a mano mantenendo le forme funzione: costo quasi nullo) e `query_as!` con cache `.sqlx/` committata (reintroduce nel Dockerfile e in CI ciò che R4 ha rimosso). La (a) è quasi certamente la risposta giusta: `assets` ha 25 colonne |
| Dipendenze dichiarate e non usate (`keeppix-server` → `keeppix-domain`, `tower-http/fs`; `keeppix-api` → `thiserror`, `http`, `tower`, `serde_json`; `uuid` ridondante nei dev-dep di `keeppix-db`) | **Fase 1** | `cargo remove`, ma alcune di esse sono provisioning voluto per la Fase 1 (dichiarato dal Task 4). Da fare in una passata sola, guardando il codice che la Fase 1 scrive davvero |
| `Config.data_dir` e `Config.allowed_origins` non sono letti da nessun codice, e `allowed_origins` è documentato come knob funzionante | **Fase 1/6** | Un utente lo configura e non accade niente. Residuo della stessa decisione: la feature `cors` di `tower-http` in due crate, mai usata |
| `SettingsRepo::get_or_create_secret` non è chiamata da nessun percorso di produzione | **Fase 6**, deliberato | **Non è una dimenticanza:** i token opachi hashati non richiedono chiave di firma, quindi in Fase 0 non c'è segreto da generare. Il Global Constraint «la chiave di sessione è generata al primo avvio e persistita» è soddisfatto a vuoto. Servirà davvero per cifrare il segreto TOTP |
| `POST /auth/refresh` non è chiamato da nessun client (`frontend/src/api/auth.ts` non ha `refresh`) | **Fase 6** | Con TTL di 30 giorni la sessione scade e si rifà il login. Da sapere: tutta la macchina di rotazione e rilevamento riuso (§3.5) non è collaudata sul campo, benché sia coperta dai test |
| `M19` — se il backend non risponde, `bootstrap()` propaga l'errore dentro `router.beforeEach` e la pagina resta bianca senza messaggio | **Fase 1** | Con I5 il backend ora distingue `503` da `401`, quindi il frontend *può* distinguere «riprova» da «sessione scaduta»: la correzione ha senso solo insieme a una UI d'errore, che la Fase 1 avrà |
| Tre schemi `OpenAPI` byte-identici (`LoginResponse`, `MeResponse`, `SetupResponse`); `info.version` è la versione del crate; rustdoc `# Errors` come `summary`; snapshot senza newline finale | **Fase 6** | Contano quando dalla specifica si genereranno client veri |
| `index.html` ha `lang="en"` hardcoded; `users.locale` e `UserView.locale` arrivano al frontend e non sono usati (la lingua vive in `localStorage`, non nel profilo come dice §10.10) | **Fase 6** | Con le impostazioni utente |
| Nessun test copre un `..` nel path del fallback SPA; nessun test «servito vs committato» sul documento `OpenAPI`; nessun unit test per `router.ts`/`stores/session.ts`; il job `image` della CI costruisce solo `amd64` | **Fase 1/6** | Reali e di ordine inferiore. Sul `..`: in build **debug** `rust-embed` legge dal filesystem a runtime, quindi è l'unica configurazione in cui la domanda ha senso, le versioni recenti canonicalizzano e l'immagine è release. Non è un'affermazione di vulnerabilità: è l'assenza di un test |
| `rotate_rejects_an_expired_token` non distingue strutturalmente quale orologio venga usato | **Differito** | Chiuderlo richiede l'iniezione dell'orologio, un'astrazione da introdurre quando servirà a più di un test — plausibilmente in Fase 1 coi job schedulati |
| I 33 file `review-*.diff` in `.superpowers/` (~600 KB versionati) | **Scelta dell'utente (R11)** | Sono ricostruibili con `git diff A..B`; report e ledger no, e sono la parte di valore |

### Rimosso dall'elenco dei difetti noti

- **`should_be_secure` case-sensitive.** La funzione **non esiste più**: il fix
  fuori piano `b3b1b32` l'ha cancellata rendendo `Secure` incondizionato. **R7 è
  superato, non differito**, e la verifica Docker l'ha confermato empiricamente.
  Non va riesumato: il prefisso `__Host-` esige la presenza letterale di `Secure`
  indipendentemente dal trasporto, mentre separatamente i browser esentano le
  origini loopback dal requisito che un cookie `Secure` viaggi su TLS.
- **Le risposte `405` senza `application/problem+json`** — corretto (I2).
- **`Auth` mappa qualsiasi errore a `401`** — corretto (I5), in entrambi i siti.
- **CSRF, difesa parziale** — corretto (I7 / R13).
- **`interval()` tronca i TTL sub-secondo**, **ruolo sconosciuto in
  `sessions.rs`**, **`clear_env()` incompleto**, **messaggio `DATABASE_URL`
  bilingue** — tutti corretti.

### Voci lasciate come sono, di proposito

- **`dummy_hash_is_a_valid_argon2id_phc_string` è sovra-pinnato** (mutare il solo
  salt lo fa fallire): è il prezzo consapevole dell'asserzione positiva, che è
  l'unica che distingue il fix dal bug. Il commento nel test lo spiega meglio di
  quanto farebbe una correzione.
- **`setup_creates_the_first_admin_and_logs_in` verifica il cookie con
  `contains`**: la proprietà è coperta dai quattro test dedicati aggiunti dopo.
- **I sette `lib.rs` con `\n` invece di 0 byte**: non è un difetto.
- **Log e messaggi interni mescolano italiano e inglese**: non viola «il backend
  non traduce», che riguarda le stringhe *utente* — tutti i `title` di `Problem`
  sono in inglese e le traduzioni vivono in `it.json`/`en.json`.

## Ripresa del lavoro: per chi apre la Fase 1

Il prossimo passo è il **piano della Fase 1**. La review finale non ha trovato
nulla che la Fase 1 debba smontare. Tre punti di frizione da conoscere *prima* di
scrivere venti handler, perché ognuno costa poco adesso e molto poi:

1. **Il fallback SPA inghiottirà `/media/*` e `/dav/*`** (`embed.rs` esclude solo
   `api/`): una miniatura mancante restituirebbe `index.html` con `200` a un tag
   `<img>`, e un client WebDAV riceverebbe HTML dove attende XML. Due righe
   adesso, un sintomo illeggibile poi.
2. **`Auth::from_request_parts` è l'unico punto da cui passa l'autenticazione**,
   e fa una query per ogni richiesta autenticata. È anche la buona notizia: la
   cache `moka` prevista da §9.4 si inserisce lì e da nessun'altra parte, ma va
   progettata con l'invalidazione (una sessione revocata non deve sopravvivere).
3. **La funzione unica di visibilità (`visibility_scope`, spec §4.2) non esiste
   ancora**, e il modo in cui verrà scritta decide la Fase 3. `AuthContext` porta
   `{ id, role }` e non i gruppi: derivarli da `user_id` con un join su
   `group_members` (che esiste già e vuota) è probabilmente la scelta giusta — un
   elenco di gruppi trasportato è un elenco che può essere stantio. Da decidere
   nel piano, non scoprendolo alla prima query.

Da decidere prima di iniziare, su **R4**: `#[derive(sqlx::FromRow)]` oppure
`query_as!` con cache `.sqlx/`. `users.rs` ripete già tre volte lo stesso blocco
di 8 `row.try_get("nome")`, e `assets` in Fase 1 ha 25 colonne.

Nuovo strumento disponibile: il crate `keeppix-test-support`, nato nella fix wave
per l'asserzione sugli header. È il posto dove far confluire `with_database`,
oggi duplicata fra i due harness.

## Nota sul metodo, per chi riprende

I finding più utili di questa fase non sono venuti dalla lettura del codice ma
da reviewer che hanno **eseguito** qualcosa: chi ha costruito un test usa-e-getta
per osservare gli header mancanti sul 404, chi è andato a leggere il sorgente di
`rand` nel registry per confermare che il generatore di token fosse davvero un
CSPRNG, chi ha provato a corrompere una costante per vedere se il test che
doveva proteggerla se ne accorgeva. Tre dei test scritti seguendo il piano
passavano senza provare ciò che il loro nome affermava.
