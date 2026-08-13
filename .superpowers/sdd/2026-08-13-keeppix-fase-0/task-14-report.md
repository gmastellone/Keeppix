# Task 14 — Immagine Docker e compose — Report

## Stato

DONE_WITH_CONCERNS. Tutti gli artefatti sono stati scritti e verificati
**staticamente** in ogni punto verificabile senza Docker. **Nessuna build,
nessun avvio di container e nessun healthcheck reale sono stati eseguiti**:
il pull delle immagini di base è bloccato dalla policy di egress (P2 del
preflight), non è transitorio e non ho tentato di aggirarlo. Vedi la sezione
"Cosa NON è stato verificato" in fondo per l'elenco puntuale di cosa resta
da fare su una macchina con Docker.

## File creati

- `/home/user/Keeppix/Dockerfile`
- `/home/user/Keeppix/.dockerignore`
- `/home/user/Keeppix/compose.yaml`
- `/home/user/Keeppix/docs/DEPLOY.md`

## Cosa ho corretto rispetto al testo letterale del brief

### Ruling P1 (vincolanti, già applicati)

- **R2**: `FROM rust:1.85-bookworm` → `FROM rust:1.88-bookworm`. Verificato
  che il codice usa davvero let-chain stabili solo da 1.88: in
  `crates/keeppix-server/src/config.rs:60-62` c'è
  `if let Some(path) = config_path && path.exists() { … }`.
  `rust-toolchain.toml` fissa `channel = "1.88.0"` e
  `Cargo.toml [workspace.package] rust-version = "1.88"`.
- **R4**: rimossi `ENV SQLX_OFFLINE=true` e `COPY .sqlx/ .sqlx/`. Verificato
  che non esiste alcuna directory `.sqlx/` nel repository (né mai committata)
  e che `keeppix-db`/`keeppix-api` usano le forme funzione di sqlx
  (`sqlx::query(...)`, `sqlx::query_scalar::<_, i32>(...)`), non le macro
  `query!`. Ho sostituito le due righe con un commento onesto: la build non
  verifica query a compile-time, quindi non serve un database né una cache.

### Correzioni di coerenza (P3/P4, di mia iniziativa dopo verifica)

1. **Commento sul layer delle dipendenze — era falso, corretto.** Il brief
   diceva "strato di dipendenze, invalidato solo dai manifest", ma
   `COPY crates/ crates/` avviene comunque prima dell'unico `cargo build`, e
   in un workspace Cargo dipendenze e sorgenti si compilano in un solo
   passaggio: modificare `crates/` invalida quel layer esattamente come
   modificare i manifest. Ho riscritto il commento per dire la verità: la
   vera cache tra build ripetute è quella montata da BuildKit
   (`--mount=type=cache` su `cargo/registry` e `target`), che sopravvive
   indipendentemente dai layer dell'immagine. Non ho introdotto il trucco
   del "build fittizio" (dummy `main.rs` per ogni crate) per separare
   davvero le due fasi: per un workspace a 7 crate sarebbe un investimento
   sproporzionato per Fase 0, e avrebbe introdotto la sua stessa classe di
   bug silenziosi (dipendenze scordate nel finto build). Nota per la
   revisione: se in una fase successiva le build diventeranno un collo di
   bottiglia reale, quella tecnica resta l'opzione.
2. **`docs/DEPLOY.md`, riga `RUST_LOG` — il default in tabella non
   corrispondeva al codice.** Il brief (e `.env.example`, che non ho
   toccato) riportano `info,sqlx=warn`, ma il default reale in
   `crates/keeppix-server/src/telemetry.rs` è
   `"info,sqlx=warn,tower_http=info"` (usato quando `RUST_LOG` non è
   impostata). Ho corretto la tabella in `DEPLOY.md` per riportare il valore
   vero. **`.env.example` resta disallineato con il codice** — non è un file
   che questo task crea né che ho il permesso di toccare (fuori dai
   confini); lo segnalo come finding per la coda, vedi sotto.
3. **Requisiti — versione di Compose imprecisa.** "Compose v2" generico non
   basta: la combinazione `depends_on: … condition: service_healthy` +
   `required: false` (per rendere `db` opzionale quando il profilo
   `bundled` non è attivo) è stata introdotta nella Compose Specification e
   richiede **Docker Compose v2.20.0+** (agosto 2023). Corretto in
   `docs/DEPLOY.md`.
4. **Sezione "Aggiornamento" — il flusso descritto non corrispondeva al
   `compose.yaml` scritto.** Il brief promette `docker compose pull &&
   docker compose up -d` e un "tag `:1`" che segue la versione major. Ma
   `compose.yaml` costruisce l'immagine **in locale** (`build: .`, `image:
   keeppix:dev`) e non punta ad alcun registro: `docker compose pull` non
   avrebbe nulla da recuperare, e nessun tag `:1` esiste in questo file (lo
   pubblicherà, forse, la pipeline di release che il Task 15 introduce, con
   tag semver su `ghcr.io/<repo>` — cosa che questo task non deve
   anticipare). Ho riscritto la sezione per descrivere quello che
   `compose.yaml` fa davvero oggi: `git pull` + `docker compose … up -d
   --build`, con una nota esplicita che spiega perché `pull` non serve a
   niente finché non esisterà un'immagine remota pubblicata.
5. **Nota sull'healthcheck e `DATABASE_URL` — aggiunta, mancava.** Il
   preflight (P3) chiede di verificare che `docs/DEPLOY.md` dica che
   l'healthcheck dipende da `DATABASE_URL`. Nel brief originale non c'era.
   L'ho aggiunta nella sezione "Diagnosi": spiega che `keeppix healthcheck`
   passa da `Config::load` come il resto del binario, quindi richiede
   `DATABASE_URL`, e che senza quella variabile il container risulta
   `unhealthy` per un errore di configurazione, non di rete.
6. **Tag `:1-debug` per ispezionare l'immagine — rimosso, era una promessa
   senza copertura.** Il brief chiudeva "Diagnosi" con "per ispezionarla,
   usare il tag `:1-debug`", ma nessuna pipeline di questo repository (né
   questo task né `task-15-brief.md`, che ho letto per controllare) pubblica
   una variante "-debug". Promettere un tag che nessuna build produce è
   peggio di non promettere nulla. Ho sostituito con un'istruzione che
   funziona davvero senza shell nell'immagine: condividere il namespace di
   processo del container da un'immagine con shell separata (`docker run
   --pid container:<nome> --network container:<nome> busybox sh`), col nome
   container reale che Compose assegnerebbe (`keeppix-keeppix-1`, dato
   `name: keeppix` + servizio `keeppix`).

Nessun'altra riga del `Dockerfile` o del `compose.yaml` del brief è stata
toccata oltre a quanto sopra: sono risultati corretti alla verifica.

## Controlli statici eseguiti, uno per uno

1. **`FROM rust:1.85-bookworm` non compilerebbe** → verificato (vedi R2
   sopra), corretto.
2. **`.sqlx/` non esiste** → verificato con ricerca nel repository, nessun
   risultato; confermato che sqlx è usato in forma funzione ovunque
   (`grep` su `crates/keeppix-db/src`, `crates/keeppix-api/src`). Corretto
   per R4.
3. **`frontend/dist` copiato prima di `cargo build`** → il brief lo fa già
   correttamente (`COPY --from=frontend /app/frontend/dist frontend/dist`
   prima del `RUN cargo build`). Verificato che il percorso combacia con
   quello letto da `rust-embed`:
   `crates/keeppix-server/src/embed.rs:13` ha
   `#[folder = "$CARGO_MANIFEST_DIR/../../frontend/dist"]`; con `WORKDIR
   /app` e il manifest di `keeppix-server` in `/app/crates/keeppix-server`,
   `../../frontend/dist` risolve esattamente in `/app/frontend/dist`, dove
   il Dockerfile lo mette. Nessuna correzione necessaria.
4. **Nome del binario `keeppix` e sottocomandi reali** → verificato con
   `cargo run -p keeppix-server -- --help`: esistono davvero `serve`,
   `migrate`, `healthcheck` (oltre a `help`), con `--config` di default
   `/data/config.toml`. `crates/keeppix-server/Cargo.toml` ha
   `[[bin]] name = "keeppix"`, quindi `cargo build --release --bin keeppix`
   e `cp target/release/keeppix …` sono corretti. `HEALTHCHECK CMD
   ["/usr/local/bin/keeppix", "healthcheck"]` e `CMD ["serve"]` combaciano
   coi sottocomandi reali.
5. **Percorsi nei `COPY`** → tutti esistono: `Cargo.toml`, `Cargo.lock`,
   `rust-toolchain.toml`, `crates/` (7 crate), `frontend/package.json`,
   `frontend/package-lock.json`, `frontend/` (intera directory). Confermato
   con `ls`.
6. **`.dockerignore` non esclude nulla che serva alla build** → verificato
   riga per riga: esclude `target`, `frontend/node_modules`,
   `frontend/dist`, `data`, `pgdata`, `.git`, `docs`, `*.md`. Nessuna di
   queste è richiesta dal Dockerfile (che copia `frontend/` selettivamente
   e `crates/` per intero). In particolare `frontend/package-lock.json`
   **non** è escluso (solo `frontend/node_modules` e `frontend/dist` lo
   sono): confermato che il file esiste davvero nel repository
   (committato dal Task 12, 197 KB) e che `npm ci` lo troverà. Nessuna
   correzione necessaria.
7. **Nomi di variabili/porte contro `config.rs` e `.env.example`** →
   confrontati uno per uno: `KEEPPIX_BIND` default `0.0.0.0:5673`,
   `KEEPPIX_DATA_DIR` default `/data`, `KEEPPIX_DB_MAX_CONNECTIONS`
   default `10`, `KEEPPIX_SESSION_TTL_SECS` default `2592000` (= 30 giorni
   in secondi, calcolo verificato), `KEEPPIX_LOG_FORMAT` default `json`,
   `KEEPPIX_ALLOWED_ORIGINS` default `[]`, `DATABASE_URL` obbligatoria
   (nessun default) — tutti confermati contro
   `crates/keeppix-server/src/config.rs` (`struct Defaults`). Unica
   discrepanza trovata: `RUST_LOG` (vedi correzione 2 sopra).
8. **Formato di `KEEPPIX_ALLOWED_ORIGINS` in `compose.yaml`** → `'[]'`
   (stringa YAML che diventa il valore letterale `[]`) è coerente con la
   sintassi mostrata in `.env.example`
   (`KEEPPIX_ALLOWED_ORIGINS=["https://foto.example.com"]`), che Figment
   interpreta come array. Nessuna correzione necessaria.
9. **`DATABASE_URL` nel compose combacia col servizio `db`** →
   `postgres://keeppix:${DB_PASSWORD:-changeme}@db/keeppix` usa utente
   `keeppix`, host `db` (nome del servizio Compose), database `keeppix`;
   il servizio `db` ha `POSTGRES_USER: keeppix`, `POSTGRES_DB: keeppix`,
   stessa password via `${DB_PASSWORD:-changeme}`. Coerente.
10. **Porte** → `EXPOSE 5673` nel Dockerfile, `ports: ["5673:5673"]` nel
    compose, `KEEPPIX_BIND=0.0.0.0:5673` di default: tutti allineati tra
    loro e con `config.rs` (`SocketAddr::from(([0, 0, 0, 0], 5673))`).
11. **`/health` e `/api/v1/setup/status`** → verificato in
    `crates/keeppix-api/src/lib.rs`: `all_routes()` monta `/health` alla
    radice e fa `.nest("/api/v1", api_routes())`, con `api_routes()` che
    contiene `/setup/status`. I due path citati nel brief (Step 6, non
    eseguibile qui) esistono davvero nel router. `routes/health.rs`
    risponde `{"status":"ok","version": env!("CARGO_PKG_VERSION")}`, e
    `Cargo.toml [workspace.package] version = "0.1.0"`, quindi la risposta
    attesa `{"status":"ok","version":"0.1.0"}` è corretta.
12. **`HEALTHCHECK` e `Config::load` (P3, primo punto)** → confermato in
    `crates/keeppix-server/src/main.rs`: il ramo `healthcheck()` chiama
    `Config::load(Some(config_path))?` prima di aprire la connessione TCP,
    quindi fallisce con l'errore "DATABASE_URL is required" (non con un
    errore di connessione) se la variabile manca. Aggiunta la nota in
    `docs/DEPLOY.md` (correzione 5 sopra) e nel `Dockerfile` stesso, sopra
    l'istruzione `HEALTHCHECK`.
13. **`read_only: true` e scritture su disco (P3, secondo punto)** →
    cercato `fs::write`, `File::create`, `tempfile`, `create_dir` in tutti
    i crate del workspace (`keeppix-server`, `keeppix-api`, `keeppix-db`,
    `keeppix-media`, `keeppix-domain`, `keeppix-jobs`, `keeppix-dav`):
    **nessun risultato**. Il logging passa da `tracing_subscriber::fmt()`
    su stdout (`telemetry.rs`), non su file. `Config::load` **legge**
    `/data/config.toml` se esiste ma non lo scrive mai. In Fase 0 il
    binario non scrive nulla sul filesystem oltre a quanto la libreria
    axum/tokio gestisce internamente (nessun temp file applicativo).
    `read_only: true` con `tmpfs: [/tmp]` su `/data` in sola scrittura via
    volume è quindi sicuro oggi. Nota per il futuro: quando Fase 1
    introdurrà scritture reali (derivati, sidecar), questo controllo andrà
    ripetuto.
14. **`.dockerignore` esclude `docs` e `*.md`, ma nulla li richiede** →
    verificato che nessun crate usa `include_str!`/`include_bytes!` su
    file Markdown o dentro `docs/`, e che nessun `Cargo.toml` del
    workspace ha un campo `readme` che punterebbe a un file escluso.
    Nessun risultato da `grep -rn "readme"` sui manifest. Coerente,
    nessuna correzione necessaria oltre a quanto già detto per
    `package-lock.json`.
15. **`depends_on: db: condition: service_healthy: required: false`,
    validità e comportamento con profilo `bundled` disattivato (P3, ultimo
    punto)** → verificato che il file YAML analizza correttamente
    (`python3 -c "import yaml; yaml.safe_load(...)"`, nessun errore, chiavi
    annidate come attese). La combinazione `required: false` +
    `condition: service_healthy` è un attributo della Compose
    Specification introdotto insieme al supporto ufficiale per
    "dipendenze opzionali controllate da profili": quando `db` appartiene
    solo al profilo `bundled` e quel profilo non è attivo, Compose non
    istanzia `db` e, con `required: false`, non tratta l'assenza come
    errore — esattamente il comportamento che `docs/DEPLOY.md` promette
    per "Avvio con un Postgres già esistente". Non ho potuto eseguire
    `docker compose config`/`up` per una conferma dinamica (Docker non
    disponibile): resta nell'elenco di cosa verificare su una macchina
    reale.
16. **Immagine runtime e librerie dinamiche** → controllato che
    `sqlx` sia configurato con `features = ["tls-rustls-ring", …]` (TLS
    puro Rust, non OpenSSL) in entrambi `keeppix-api/Cargo.toml` e
    `keeppix-db/Cargo.toml`; `ldd` su un binario locale (build debug, stesso
    linking del binario release) mostra solo `libgcc_s`, `libm`, `libc`:
    nessuna dipendenza da OpenSSL o altre librerie di sistema. La base
    `gcr.io/distroless/cc-debian12:nonroot` (glibc + libgcc, niente
    OpenSSL) è quindi sufficiente; non serve la base `distroless/base`
    (troppo nuda) né una base con OpenSSL. Nessuna correzione necessaria.
17. **Frontend: build reale** → eseguito `cd frontend && npm run build`:
    completa senza errori (`vue-tsc -b && vite build`), produce
    `dist/index.html`, `dist/assets/*.{js,css}`, `dist/favicon.svg` — la
    stessa struttura che il `Dockerfile` copia con
    `COPY --from=frontend /app/frontend/dist frontend/dist`.

## Comandi eseguiti e output

```bash
export KEEPPIX_TEST_DATABASE_URL="postgres://keeppix:keeppix@127.0.0.1:5432/postgres"
cargo build --workspace
# Finished `dev` profile [unoptimized + debuginfo] target(s) in 3.90s

cargo run -q -p keeppix-server -- --help
# Commands: serve, migrate, healthcheck, help — confermati

cargo test --workspace -- --test-threads=1
# tutti i "test result: ok", 0 failed, in ogni crate

cd frontend && npm run build
# vue-tsc -b && vite build — ✓ built in 735ms, dist/ generata
```

`df -h /` prima e dopo: 13-14 GB liberi, nessun accumulo sospetto di
database di test da fase precedenti; nessun fallimento riconducibile a
disco pieno.

## Backend defect trovato durante i controlli statici (non corretto, fuori
## dai confini di questo task)

- **`.env.example` disallineato col default reale di `RUST_LOG`.** Il file
  commenta `# RUST_LOG=info,sqlx=warn`, ma
  `crates/keeppix-server/src/telemetry.rs` usa come fallback
  `"info,sqlx=warn,tower_http=info"` quando la variabile non è impostata.
  Non è un difetto del Dockerfile/compose che ho scritto (l'ho verificato
  contro il codice, non contro `.env.example`), ma segnalo la
  disallineamento affinché entri nella coda dei finding: `.env.example`
  andrebbe aggiornato per riportare il default vero, oppure `telemetry.rs`
  andrebbe allineato al default documentato — decisione che spetta a chi
  possiede quel file, non a questo task.

## Cosa NON è stato verificato (richiede una macchina con Docker)

Nessuno dei seguenti comandi è stato eseguito. Vanno lanciati su una
macchina con accesso a Docker e a Internet prima di considerare la Fase 0
chiusa:

1. `docker build -t keeppix:dev .` — build effettiva dell'immagine
   multi-stage (frontend Node → backend Rust → runtime distroless). Non
   verificato: compila davvero, tempo di build, eventuali errori di
   sintassi Dockerfile non catturabili da un parser YAML/testuale.
2. `docker images keeppix:dev --format '{{.Size}}'` — dimensione reale
   dell'immagine, atteso sotto 100 MB in Fase 0 (nessun ffmpeg incluso).
3. `docker run --rm --entrypoint /bin/sh keeppix:dev -c 'echo ciao'` —
   conferma che l'immagine distroless non ha shell (comportamento atteso:
   errore `exec: "/bin/sh": stat /bin/sh: no such file or directory`).
4. `DB_PASSWORD=devpassword docker compose --profile bundled up -d --build`
   seguito da `curl http://127.0.0.1:5673/health` e
   `curl http://127.0.0.1:5673/api/v1/setup/status` — avvio reale dello
   stack, comunicazione tra i due container, risposta effettiva delle rotte
   (ho verificato solo che le rotte esistono nel codice sorgente, non la
   risposta a runtime in un container).
5. `docker compose ps --format '{{.Name}} {{.Status}}'` — che
   l'`HEALTHCHECK` riporti davvero `(healthy)` entro `start-period=20s`.
6. Flusso da browser: setup del primo admin, logout, login, riavvio dello
   stack (`docker compose down && … up -d`) con dati persistenti su
   `./pgdata` e `./data`.
7. `docker compose down -v` pulito, senza container o volumi residui.
8. Conferma dinamica che `depends_on: db: required: false` con profilo
   `bundled` disattivato faccia davvero avviare `keeppix` senza errori
   quando si passa un `DATABASE_URL` esterno (verificato solo staticamente
   al punto 15 sopra).
9. Che l'immagine base `gcr.io/distroless/cc-debian12:nonroot` sia
   raggiungibile e che il tag esista ancora con quel nome esatto (non
   raggiungibile da questo ambiente per verificarlo).

Il job `image` della CI (Task 15, `.github/workflows/ci.yml`) costruirà
questa immagine su un runner GitHub con Docker disponibile: sarà la prima
verifica reale di questo Dockerfile, come indicato dal preflight.

## Confini rispettati

Non ho toccato `crates/`, `frontend/`, `docs/api/`. Non ho toccato
`.env.example` (difetto segnalato sopra, non corretto). `git add` verrà
limitato ai quattro file creati da questo task.
