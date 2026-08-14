# Fase 0 — fix wave della review finale del branch

**Data:** 2026-08-14 · **Branch:** `fase-0` · **Commit di partenza:** `6426cda`
**Base:** `final-review-report.md`, `…-STATO.md`, spec `2026-08-13-keeppix-design.md`

Un solo dispatch, nessuna seconda ondata: le 15 voci assegnate sono state
affrontate tutte. Dodici commit più questo report e l'aggiornamento di
`STATO.md`.

## Esito in una riga

Tutte e 15 le voci sono state chiuse. Due lo sono state con una decisione
diversa da quella «piena» — I4 (ICU) come **deroga registrata** invece che come
implementazione, e la regola `deny.toml` di M13 con un limite dichiarato — e in
entrambi i casi la ragione è scritta sia nel codice sia in `STATO.md`. Nessuna
voce è rimasta a metà.

## Commit

| SHA | Voce | Titolo |
|---|---|---|
| `23f9964` | I1 | `fix(db): enable PostGIS in migration 0001` |
| `6a76811` | I2 | `feat(api): bring axum's built-in rejections into the RFC 9457 contract` |
| `c584c58` | I5 | `fix(api): a database outage is a 503, not a session expiry` |
| `e5bf7fe` | I7 / R13 | `feat(api): enforce the custom-header half of the CSRF defence` |
| `cdc3cd8` | M1 (voce 7) | `test: one shared assert_security_headers that actually asserts the CSP` |
| `df86e4b` | I3 | `feat(api): mark authenticated responses Cache-Control: private` |
| `19b986c` | I6 | `feat(api): drop style-src 'unsafe-inline', add HSTS` |
| `064dfff` | R3 / voce 9 | `fix(db): align the error taxonomy and keep sub-second TTLs` |
| `3720e00` | voci 10-11 | `fix(server): clear every KEEPPIX_* key in tests, and one language per message` |
| `9023059` | voce 12 | `docs: document how to stop the stack, profile gotcha included` |
| `3b7aa1d` | M13 | `build: make the media/db crate boundary structural, not social` |
| `43f366a` | I4 | `chore(frontend): drop the unused @intlify/core-base, document the plural ruling` |

## Voce per voce

### 1 (I1) — PostGIS

`crates/keeppix-db/migrations/0001_users.sql`: aggiunto
`CREATE EXTENSION IF NOT EXISTS postgis;` e riscritto il commento, che
dichiarava una decisione mai implementata. Il commento ora dice *perché* le due
estensioni vanno abilitate insieme e adesso: `pg_trgm` è *trusted* da PG13 e la
crea il proprietario del database, `postgis` no e richiede il superuser.

Aggiunto `required_extensions_are_enabled` in
`crates/keeppix-db/tests/migrations.rs`, che interroga `pg_extension`. La
roadmap della Fase 4 afferma «già abilitata dalla migrazione 0001»: ora
quell'affermazione è coperta da un test invece che da un commento.

Il checksum della `0001` cambia, come previsto dalla review: le uniche istanze
esistenti sono database di test usa-e-getta.

### 2 (I2) — le rejection di axum dentro RFC 9457

Nuovo modulo `crates/keeppix-api/src/json.rs`: `Json<T>` avvolge `axum::Json` e
converte la `JsonRejection` in `Problem`. Avvolge **entrambe** le direzioni
(estrazione e risposta) così che una rotta importi un solo tipo e non possa
estrarre con `axum::Json` e rispondere con l'altro. La conversione vive in
`impl From<JsonRejection> for Problem` (`problem.rs`), dove stanno tutti i
`type` stabili:

- `MissingJsonContentType` → `415 keeppix/unsupported-media-type`
- tutto il resto → `keeppix/invalid-json`, **conservando lo status di axum**
  (400 sintassi, 422 forma inattesa). Per il client è lo stesso problema, e lo
  status distingue già i due casi; `detail` riporta il messaggio di axum
  («missing field `password`»), che è in inglese, rivolto allo sviluppatore e
  descrive il corpo della *richiesta*, quindi non rivela nulla del server.
- `Problem` ha ora anche `with_retry_after`, `method_not_allowed()`,
  `csrf_check_failed()` e `service_unavailable()`.

`method_not_allowed_fallback` aggiunto in `all_routes()` e in
`router_without_state()`, con il commento sulla trappola: va chiamato **dopo**
aver registrato le rotte, perché imposta il fallback dei `MethodRouter` già
presenti. Verificato che funzioni **attraverso `nest`**: il 405 su
`GET /api/v1/auth/login` porta il `type` corretto.

I quattro handler e `health` usano il nuovo `Json`. Le annotazioni `utoipa` di
`login` e `setup_create` dichiarano 400/415/422; snapshot rigenerato.

Test: `a_wrong_content_type_is_rejected_as_problem_json` (415),
`a_malformed_json_body_is_rejected_as_problem_json` (400 e 422, con
l'asserzione che `detail` nomini il campo mancante),
`a_wrong_method_is_rejected_as_problem_json` (405 + header di sicurezza, perché
nasce da un fallback di `MethodRouter` e non da una rotta),
`wrong_method_returns_problem_json` in `tests/health.rs` per il router senza
stato.

### 3 (I5) — un blip del database non è un logout

La decisione vive ora in **una** funzione, `extract::session_problem`, usata dai
due siti che consultano un token (l'extractor `Auth` e l'handler `refresh`):

- `DbError::Connection` → `503 keeppix/service-unavailable` con `Retry-After: 5`
- `NotFound` / `Forbidden` → `401 keeppix/unauthenticated` (indistinguibili di
  proposito: un riuso rilevato non deve essere distinguibile da un token
  scaduto)
- tutto il resto (`Corrupted`, `Migration`) → `500` loggato.

**Scostamento dichiarato dalla brief.** La brief diceva «keep 401 for the
genuine cases (`NotFound`, `Forbidden`, and anything else that means the session
is not valid)». Ho mandato `Corrupted` e `Migration` su `500` invece che su
`401`: una riga che il server non riesce a leggere (per esempio un `role` fuori
dal CHECK) non è una sessione non valida, è un difetto del server, e spacciarla
per scadenza manderebbe l'utente a rifare un login che non risolve niente. Non
concede comunque accesso: fallisce chiuso come prima. Se si preferisce l'altra
lettura, è una riga in `session_problem`.

Il commento di `refresh` che razionalizzava «ogni errore è 401» è stato
corretto: l'argomento vale fra `NotFound` e `Forbidden`, non per un database
irraggiungibile.

Test, due livelli:
1. unit test `a_database_outage_is_transient_a_bad_session_is_not` sulla
   tassonomia, deterministico;
2. integrazione `a_database_outage_is_a_503_not_a_401`: **spegne il container
   Postgres** sotto il server con una sessione autenticata viva e asserisce
   `503` + `Retry-After`. Nessun mock sta fra l'handler e il pool, quindi è
   l'unico modo di osservare la proprietà. Salta (stampando il motivo) sul
   percorso `KEEPPIX_TEST_DATABASE_URL`, dove il server è condiviso.

Entrambi verificati **per mutazione**: rimettendo `Problem::unauthenticated()`
nel ramo `Connection`, entrambi falliscono.

`docs/api/openapi.json` rigenerato: `refresh` e `me` dichiarano `503`.

### 4 (I7) — CSRF

Nuovo modulo `crates/keeppix-api/src/csrf.rs`, applicato come
`axum::middleware::from_fn` su `api_routes()`: le mutazioni
(POST/PUT/PATCH/DELETE) prive di `x-keeppix-client` ricevono
`403 keeppix/csrf-check-failed`. I metodi sicuri passano sempre.

Il modulo documenta la proprietà comprata (un `<form>` cross-site non può
impostare header custom senza preflight CORS) e le deroghe da prevedere: WebDAV
in Fase 5 e gli upload tus, che vivranno fuori da `/api/v1` e quindi fuori dal
layer — scritte adesso perché siano una decisione e non una scoperta.

**Frontend verificato:** ogni chiamata all'API passa da `apiFetch`
(`getSetupStatus`, `setupAccount`, `login`, `me`, `logout` in
`frontend/src/api/auth.ts`), e `apiFetch` imposta l'header su *tutte* le
richieste. Nessun call site da correggere. Il commento di `client.ts`, che
prometteva l'enforcement «in una fix wave successiva», ora descrive il
comportamento reale. Aggiunto un test vitest che asserisce che `apiFetch`
continui a mandare header e `credentials`: senza quell'header ogni login, setup
e logout smetterebbe di funzionare.

L'harness dei test manda l'header di default, esattamente come il frontend;
`plain_client()` è un client legittimo senza cookie store. I due test dedicati
costruiscono a mano un client *senza* header.

### 5 (I3) — `Cache-Control: private`

`SetResponseHeaderLayer::if_not_present(CACHE_CONTROL, "private")` in
`with_common_layers`. Il commento spiega perché **non** `overriding`: gli asset
hashati escono con `public, max-age=31536000, immutable` da `embed.rs`, e
sovrascriverli annullerebbe la prima voce della stessa §9.4.

Test: `authenticated_responses_are_marked_private` su `GET /auth/me`;
`assets_are_served_as_immutable` in `keeppix-server/tests/embed.rs` è la
controprova e ora lo dichiara nel commento (con `overriding` fallirebbe).

### 6 (I6) — CSP e HSTS

(a) `'unsafe-inline'` rimosso da `style-src`, e il commento che lo giustificava
sostituito con la verifica. Ricontrollato sul bundle ricostruito:
`dist/index.html` carica `<link rel="stylesheet" href="/assets/index-5Zgkpkbu.css">`,
non contiene un solo `<style>` né un attributo `style=`, e i bundle non
contengono `createElement("style")` né `insertRule` (grep su `dist/assets/*.js`).
Gli stili che Vue e Reka UI impostano a runtime passano dal CSSOM, che la CSP non
intercetta.

(b) `Strict-Transport-Security: max-age=31536000; includeSubDomains`
incondizionato. Non rompe l'uso in chiaro in LAN: un browser **ignora** HSTS
ricevuto su HTTP (RFC 6797 §8.1), quindi l'header ha effetto solo dove il TLS che
pretende esiste già. `preload` **escluso** di proposito: iscriverebbe il dominio
dell'utente a una lista globale difficilmente reversibile, e non è una decisione
che Keeppix possa prendere per lui. Il ragionamento è nel commento su `HSTS`.

Asserzioni estese nel punto unico (voce 7): HSTS con il valore esatto, e nessuna
deroga `unsafe-*` in *nessuna* direttiva.

**Non fatto:** la conferma visiva in un browser reale. È il Critical C2 della
review, non assegnato a questa fix wave, e richiede lo stack compose in piedi.
La verifica strutturale sopra copre la classe di difetto («qualcosa richiede
stili inline»); resta da guardare la pagina.

### 7 — `assert_security_headers`

Nuovo crate `crates/keeppix-test-support` (`publish = false`), dev-dependency di
`keeppix-api` e `keeppix-server`.

**Perché un crate e non un'alternativa più leggera:** due binari di test in crate
diversi non possono condividere codice se non attraverso un crate. Le opzioni
erano (a) un `#[path = "../../keeppix-api/tests/…"]` che attraversa il confine fra
crate — fragile e sorprendente; (b) duplicare comunque; (c) un crate di
test-support. La (c) è anche ciò che la Fase 1 vorrà per `with_database`, oggi
duplicata fra i due harness (M2). Il tipo accettato è `http::HeaderMap`, che è
lo stesso tipo che `axum::http` e `reqwest::header` ri-esportano: una funzione
sola serve i test `reqwest` e quelli `oneshot`.

L'asserzione sulla CSP ora verifica la **sostanza**: cinque direttive
(`default-src 'self'`, `script-src 'self'`, `frame-ancestors 'none'`,
`base-uri 'none'`, `form-action 'self'`) confrontate **per direttiva esatta**
dopo lo split su `;` — `default-src 'self' *` non soddisfa `default-src 'self'`
— più il divieto di `unsafe-inline`/`unsafe-eval` in qualunque direttiva e la
presenza di `style-src` (senza la quale la rimozione della deroga non sarebbe
osservabile, perché vincerebbe `default-src`).

**Verificato per mutazione:** con `const CSP = "default-src *"` fallivano **5
test** in due crate (`security_headers_are_present`,
`wrong_method_returns_problem_json`, `unknown_api_path_returns_problem_json`,
`index_is_served_at_root`, `client_routes_fall_back_to_index`). Prima non ne
falliva nessuno.

Effetto collaterale necessario: `#![allow(dead_code)]` in
`keeppix-api/tests/harness/mod.rs`, perché il modulo è incluso da due binari e
`stop_database()` serve solo a uno.

### 8 (R3) — tassonomia

`sessions.rs`: ruolo sconosciuto → `DbError::Corrupted`, con il commento che
richiama R3 e `users.rs`. Irraggiungibile grazie al CHECK, ma le due tassonomie
ora coincidono.

### 9 — `interval()`

`as_secs_f64()` invece di `as_secs()`. Unit test su quattro casi: 500 ms →
`"0.5 seconds"`, 1500 µs → `"0.0015 seconds"`, zero → `"0 seconds"` (proprietà
su cui poggiano i tre test esistenti di scadenza immediata), 2 592 000 s →
`"2592000 seconds"` senza decimali.

### 10 — `clear_env()`

Derivata dall'ambiente invece che da un elenco: rimuove ogni variabile
`KEEPPIX_*` più `DATABASE_URL`. Non può restare indietro rispetto ai campi di
`Config` — che era il punto della segnalazione — perché non li elenca. Esclude
`KEEPPIX_TEST_*`, che è configurazione dell'harness (R9) e che `Config` ignora
come campo sconosciuto: cancellarla romperebbe i test di integrazione se un
giorno finissero nello stesso binario.

Non ho derivato l'elenco dai campi di `Config` come la brief suggeriva «se
possibile senza contorsioni»: farlo richiederebbe di serializzare un `Config`
già caricato, cioè di caricare la configurazione prima di poter pulire
l'ambiente. Derivarlo dall'ambiente è più forte, non solo più semplice: copre
anche chiavi che `Config` non ha.

### 11 — messaggio `DATABASE_URL`

`(es. …)` → `(e.g. …)`, con un commento che dice perché il messaggio è in
inglese (errore di avvio rivolto all'operatore; la localizzazione è del
frontend).

### 12 — `docs/DEPLOY.md`

Nuova sezione «Arresto» con `docker compose --profile bundled down` e `-v`, la
spiegazione in una frase del perché il profilo serve anche per fermare, e la nota
che con un Postgres esterno basta `down`.

**Riverificato prima di scriverlo**, con un `compose.yaml` minimo che riproduce
la stessa struttura profilo + `depends_on: required: false`:

```
--- dopo up --profile bundled: kpxcheck-app-1 running / kpxcheck-db-1 running
--- docker compose down:       Network kpxcheck_default Resource is still in use
--- dopo down:                 kpxcheck-db-1 Up 20 seconds (healthy)
--- dopo --profile bundled down -v: (vuoto)
```

### 13 — confine `media`/`db` in `deny.toml`

```toml
[[bans.deny]]
crate = "keeppix-db"
wrappers = ["keeppix-api", "keeppix-server"]
```

`keeppix-jobs` e `keeppix-dav` **non** sono pre-elencate benché lo spec §3.2 le
preveda come consumatori legittimi: quando la Fase 1 collegherà `jobs → db` la CI
si fermerà, e aggiungere la voce sarà una decisione presa guardando l'arco invece
che infilarsi in un'allowlist già larga. Le avevo elencate nel primo tentativo e
`cargo deny` emetteva due `warning[unused-wrapper]`: rumore permanente in CI per
nessun beneficio.

**Test negativo eseguito**, come richiesto:

```
$ cargo deny check bans                       → bans ok            (exit 0)
$ # aggiunto keeppix-db = { path = "../keeppix-db" } a keeppix-media
$ cargo deny check bans                       → exit 2
   error[banned]: crate 'keeppix-db = 0.1.0' is explicitly banned
   warning[unmatched-wrapper]: direct parent 'keeppix-media = 0.1.0' of banned
                              crate 'keeppix-db = 0.1.0' was not marked as a wrapper
$ # arco rimosso
$ cargo deny check bans                       → exit 0
$ cargo deny check advisories bans licenses   → advisories ok, bans ok, licenses ok
```

Limite dichiarato nel commento di `deny.toml`: il controllo è sui dipendenti
**diretti**, quindi un percorso `media → jobs → db` non verrebbe intercettato
una volta autorizzato `jobs`. È però l'inverso della direzione dello spec
(`jobs → media`), che il grafo aciclico di cargo rende impossibile una volta
esistente. `cargo-deny` non offre una regola «non nell'albero di X»: `wrappers` è
lo strumento disponibile.

### 14 (I4) — ICU

**Scelta: rimuovere la dipendenza e registrare la deroga.** `@intlify/core-base`
è stato rimosso da `dependencies` (era importato da nessuna parte: verificato con
grep su `src/`, `vite.config.ts`, `vitest.config.ts`; resta nel lock come
dipendenza transitiva di `vue-i18n`, che è corretto).

Perché non implementarlo: ICU vero richiede `intl-messageformat` (~25 KB gzip su
un budget di 150 KB di cui 77 già usati) più un `messageCompiler` custom da
mantenere; `@intlify/core-base` non fa ICU, quindi non c'era «solo da collegare
cinque righe». E il *beneficio* dichiarato dallo spec è «plurali corretti»:
italiano e inglese hanno esattamente due categorie plurali CLDR (`one`/`other`),
che è precisamente ciò che la sintassi nativa `'una foto | {n} foto'` esprime.
Per le lingue spedite le due sintassi sono osservabilmente identiche. Le chiavi
plurali oggi sono zero.

La decisione, la ragione e **quando riaprirla** (prima lingua con più di due
categorie plurali: allora un compilatore ICU a *build time*,
`@intlify/unplugin-vue-i18n`) sono scritte in `frontend/src/i18n/index.ts`, dove
le troverà chi scrive il primo plurale, e in `STATO.md`.

Dopo la rimozione: build ok, bundle iniziale **76 472 byte** gzip su 153 600.

### 15 (I10) — `STATO.md`

Riscritto. Tabella completa fino al Task 15 con i commit finali
(`adca7c6`, `c6b82f0`, `e1f72b3`, `f6d1e34`, `91904fb`), conteggio reale dei
test, esito della verifica Docker, `should_be_secure` **rimosso** dai difetti
noti con R7 marcato superato, R11-R13 aggiunti ai ruling, e due sezioni nuove:
«Cosa resta prima del merge» (i due Critical: CI mai eseguita, browser) e
«Review finale del branch e fix wave», con il triage completo — cosa è stato
corretto e cosa è differito, a quale fase e perché.

Aggiunta alla sezione «Come si esegue la suite» la nota che `frontend/dist` è un
prerequisito della *compilazione* (non dei test) e il comando di pulizia dei
database orfani sul percorso `KEEPPIX_TEST_DATABASE_URL`.

## Verifica

Eseguita sull'albero al commit `43f366a` più `STATO.md` e questo report.

```
$ cd frontend && npm ci && npm run build
  dist/index.html                 0.45 kB │ gzip:  0.29 kB
  dist/assets/index-5Zgkpkbu.css  9.51 kB │ gzip:  2.71 kB
  dist/assets/index-CumzRq_k.js 202.51 kB │ gzip: 74.60 kB
  ✓ built in 194ms
  → bundle iniziale (soli asset referenziati da index.html): 76 472 / 153 600 byte

$ npx vue-tsc --noEmit
  (nessun output)

$ npx vitest run
  Test Files  3 passed (3)
       Tests  9 passed (9)

$ npx eslint . --max-warnings 0
  (nessun output)

$ cargo test --workspace -- --test-threads=1
  keeppix-api      unit  2 · auth 24 · health  4 · openapi 6      = 36
  keeppix-db       unit  1 · migrations 8 · sessions 14 · settings 6 · users 12 = 41
  keeppix-domain   unit 22                                        = 22
  keeppix-server   config 4 · embed 4                             =  8
  keeppix-dav / -jobs / -media / -test-support: 0 (crate vuote o di supporto)
  → 107 passed; 0 failed; 0 ignored   (exit 0)

  Erano 96 prima della fix wave. Gli 11 nuovi: 3 rejection di axum (415, 400+422,
  405 con stato) + 1 405 senza stato + 1 outage 503 end-to-end + 1 unit sulla
  tassonomia di `session_problem` + 2 CSRF + 1 `Cache-Control: private` +
  1 estensioni della migrazione `0001` + 1 unit su `interval()`.
  Nel frontend: 9 vitest (erano 8; il nono asserisce l'header CSRF).

$ cargo clippy --workspace --all-targets -- -D warnings
  Finished `dev` profile [unoptimized + debuginfo] target(s)

$ cargo fmt --check
  (nessun output)

$ cargo deny check advisories bans licenses
  advisories ok, bans ok, licenses ok

$ git diff --exit-code docs/api/openapi.json
  (nessun output, exit 0 — lo snapshot committato è quello che il codice produce)
```

## Cose che non ho fatto, e perché

- **La verifica in browser reale (C2)** e **la CI su un runner (C1)**: sono i due
  Critical della review, non assegnati a questa fix wave. Restano l'unica cosa
  che separa la fase dal merge, e sono registrati in `STATO.md` sotto «Cosa resta
  prima del merge». La rimozione di `style-src 'unsafe-inline'` conviene
  guardarla nella stessa sessione.
- **Il frontend non tratta ancora il `503` come «riprova»**. La review lo
  suggeriva insieme a I5, ma è M19 (pagina bianca quando `bootstrap()` propaga
  l'errore dentro `router.beforeEach`), che non è fra le 15 voci e che ha senso
  correggere insieme a una UI d'errore. Oggi il backend distingue i due casi, che
  è la metà che non si poteva fare dopo; il frontend può distinguerli quando
  avrà dove mostrarlo. Registrato come Fase 1.
- **Nessuna delle voci differite dalla review** (ricontrollo di `disabled_at`,
  rate limiting, deadlock 40P01, azzeramento della password, pulizia dei database
  dell'harness, policy dei tag di release, compose che compila da sorgente): la
  brief le escludeva esplicitamente e ognuna ha la sua ragione registrata in
  `STATO.md`.
- **`Retry-After` è un valore fisso (5 s)** invece di derivare da uno stato del
  pool. Un backoff informato richiederebbe di sapere quanto durerà l'indisponibilità,
  che è esattamente ciò che non si sa.

## Preoccupazioni

1. **Il checksum della migrazione `0001` è cambiato.** Qualunque database creato
   con la versione precedente — comprese eventuali prove locali dell'utente in
   `./pgdata` — fallirà l'avvio con un errore di checksum di `sqlx::migrate`. È
   la conseguenza accettata dalla review («oggi le uniche istanze sono database di
   test»), ma se esiste una `./pgdata` a cui si è affezionati va cancellata.
2. **Il layer CSRF è su `api_routes()`, non su tutto.** È deliberato (copre
   `/api/v1`, dove vivono le mutazioni) ma significa che una rotta mutante
   montata **fuori** da `api_routes()` non sarebbe coperta. Se la Fase 1 aggiunge
   `POST /media/...` fuori da `/api/v1`, va deciso lì. Il commento in `csrf.rs`
   lo dice.
3. **`a_database_outage_is_a_503_not_a_401` dura ~15 s** (l'`acquire_timeout` del
   pool) e spegne un container: è il test più lento e più «fisico» della suite.
   Se in CI si rivelasse instabile, la proprietà resta coperta dall'unit test
   deterministico, e il test di integrazione può essere marcato `#[ignore]`
   invece di essere cancellato — ma non l'ho fatto in anticipo, perché in locale
   passa e la prova end-to-end è quella che vale.
4. **Le rejection di `Path`/`Query` non sono coperte.** `Json<T>` risolve la
   classe che esiste oggi; la Fase 1 aggiungerà `Path<Uuid>` e `Query<T>`, che
   hanno le loro rejection e vanno avvolte allo stesso modo. Il modulo `json.rs`
   è il posto dove farlo e lo stampo è già lì, ma non ho creato wrapper per
   extractor che nessuna rotta usa ancora.
