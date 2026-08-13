# Task 14 — Immagine Docker e compose — Review

## Metodo

Review interamente statica (Docker non disponibile in questo ambiente, come da
P2 del preflight). Letto il diff review-17a1a28..19d9f22.diff per intero (4
file nuovi, nessun file esistente toccato), poi verificato ogni affermazione
del Dockerfile/compose.yaml/docs/DEPLOY.md contro la realtà del repository:
`crates/keeppix-server/src/{config.rs,main.rs,embed.rs,telemetry.rs}`,
`crates/keeppix-api/src/{lib.rs,routes/health.rs}`, `crates/keeppix-db/src/lib.rs`,
`crates/keeppix-db/Cargo.toml`, `crates/keeppix-api/Cargo.toml`, `Cargo.toml`,
`rust-toolchain.toml`, `.env.example`, `.gitignore`, e l'albero `crates/`
per assenza di `.sqlx/` e di scritture su filesystem. Validato `compose.yaml`
con `python3 -c "import yaml; yaml.safe_load(...)"` (parsing corretto).
Non ho eseguito `cargo build`/`cargo test` (vincolo esplicito), né alcun
comando Docker (bloccato dalla policy di rete, come per l'implementer).

## Spec Compliance

❌ **Issues found** — i quattro artefatti richiesti esistono e i due ruling
vincolanti del preflight (R2, R4) sono applicati correttamente, ma la
verifica di coerenza tra `docs/DEPLOY.md` e `compose.yaml` che il preflight
P4 rende vincolante ("Verifica che ogni comando che promette sia coerente con
i file che hai effettivamente scritto") non è stata completa: il comando
principale del percorso "Postgres già esistente" non funziona come descritto
(vedi Critical sotto). Questo non è un difetto introdotto dall'implementer —
il testo era già così nel brief originale — ma il report dichiara che questa
sezione specifica è stata "verificata" con l'unico controllo sull'host `db`
nello scenario *bundled* (punto 9 del report), senza però riverificare lo
scenario *esterno* che la stessa sezione di `docs/DEPLOY.md` documenta due
righe più sopra.

Tutto il resto è conforme:
- `.dockerignore`: identico al brief, verificato riga per riga che non
  esclude nulla richiesto dalla build (`frontend/package-lock.json` presente
  e non escluso).
- `Dockerfile`: R2 (`rust:1.88-bookworm`) e R4 (rimozione di
  `SQLX_OFFLINE`/`COPY .sqlx/`) applicati esattamente come richiesto dal
  preflight, con verifica indipendente che confermo: nessuna directory
  `.sqlx/` nel repository, nessun uso di `query!`/`query_as!`/`query_scalar!`
  in `crates/` (solo forme funzione `sqlx::query(...)`,
  `sqlx::query_scalar::<_, i32>(...)`).
- Tutti i percorsi `COPY` esistono: `Cargo.toml`, `Cargo.lock`,
  `rust-toolchain.toml`, `crates/` (incluso `crates/keeppix-db/migrations/`,
  necessario a `sqlx::migrate!("./migrations")` in
  `crates/keeppix-db/src/lib.rs:16`, che essendo una macro richiede i file
  `.sql` presenti **a tempo di compilazione**: confermato che
  `COPY crates/ crates/` li porta dentro e che nessuna riga di
  `.dockerignore` li esclude), `frontend/package.json`,
  `frontend/package-lock.json`, `frontend/`.
- `keeppix healthcheck` esiste davvero come sottocomando (`main.rs:20-27`,
  variante `Command::Healthcheck`) e si comporta come il Dockerfile assume:
  chiama `Config::load` (richiede `DATABASE_URL`) e poi un TCP connect su
  `127.0.0.1:<porta>` (`main.rs:96-101`).
- Nomi env/porte/default coincidono con `config.rs` (`struct Defaults`,
  righe 39-48) per tutte le variabili tranne `RUST_LOG`, dove
  `.env.example` è disallineato da `telemetry.rs:9` — la discrepanza è reale
  (confermata: `.env.example` dice `info,sqlx=warn`,
  `telemetry.rs:9` usa `"info,sqlx=warn,tower_http=info"`) e l'implementer
  l'ha corretta solo in `docs/DEPLOY.md:38`, segnalando correttamente che
  `.env.example` resta fuori dai confini del task.
- `read_only: true` è sicuro nella Fase 0 attuale: nessuna chiamata a
  `fs::write`/`File::create`/`OpenOptions`/`create_dir`/`tempfile` in
  nessun crate del workspace (confermato con grep su tutto `crates/`); il
  logging va su stdout (`telemetry.rs`), `Config::load` legge
  `/data/config.toml` ma non lo scrive mai.
- `depends_on: db: condition: service_healthy, required: false` è sintassi
  YAML valida e semantica coerente con la Compose Specification per
  dipendenze opzionali controllate da profili.

## Strengths

- I due ruling vincolanti (R2, R4) sono applicati esattamente come richiesto,
  con verifica indipendente nel report che ho confermato riproducendo gli
  stessi controlli (grep su `query!`, ricerca di `.sqlx/`, lettura di
  `rust-toolchain.toml`/`Cargo.toml`).
- Le 6 "correzioni di coerenza" dichiarate nel report sono reali, non
  cosmetiche: ho verificato singolarmente ognuna.
  - Il commento sul layer delle dipendenze era davvero falso (in un
    workspace Cargo con un solo `cargo build`, `COPY crates/ crates/`
    invalida lo stesso layer dei manifest) — riscritto per dire la verità
    sulla vera fonte di cache (`--mount=type=cache`).
  - Il default `RUST_LOG` in tabella era davvero sbagliato rispetto al
    codice — corretto con il valore vero da `telemetry.rs`.
  - La sezione "Aggiornamento" del brief prometteva `docker compose pull`
    per un'immagine che `compose.yaml` costruisce solo in locale
    (`build: .`, nessun registro) — riscritta per riflettere il flusso
    reale (`git pull` + `--build`).
  - La nota sull'healthcheck e `DATABASE_URL` mancava nel brief nonostante
    fosse richiesta esplicitamente dal preflight P3 — aggiunta sia nel
    Dockerfile (commento sopra `HEALTHCHECK`) sia in `docs/DEPLOY.md`.
  - Il tag `:1-debug` promesso dal brief non è pubblicato da nessuna
    pipeline del repository (verificato anche contro `task-15-brief.md`) —
    sostituito con una tecnica di debug che funziona davvero su
    un'immagine distroless (condivisione namespace PID/network con un
    container ausiliario).
- Verifica indipendente e utile della catena `rust-embed`: il percorso
  `#[folder = "$CARGO_MANIFEST_DIR/../../frontend/dist"]` in `embed.rs:13`
  risolve esattamente in `/app/frontend/dist` dato `WORKDIR /app` e la
  posizione del manifest di `keeppix-server`; confermato che il `COPY
  --from=frontend` scrive proprio lì, e **prima** del `RUN cargo build`
  (l'ordine è essenziale perché `rust-embed` legge la cartella a tempo di
  compilazione).
- Il report distingue con chiarezza cosa è stato verificato staticamente da
  cosa resta da fare su una macchina con Docker (sezione finale con 9 comandi
  puntuali), senza mai scrivere "immagine costruita" o "stack avviato" —
  rispetta la lettera di P2.

## Issues

### Critical (Must Fix)

#### 1. `docs/DEPLOY.md:24` promette un comando che non fa quello che dice — **avvio fallito**

La sezione "Avvio con un Postgres già esistente" istruisce:

```bash
DATABASE_URL=postgres://utente:password@mio-host:5432/keeppix docker compose up -d
```

Ma `compose.yaml:8-11` fissa `DATABASE_URL` a un valore **letterale**:

```yaml
    environment:
      # Con un Postgres esterno, sostituire questo valore e omettere
      # `--profile bundled`: il servizio `db` non verrà avviato.
      DATABASE_URL: postgres://keeppix:${DB_PASSWORD:-changeme}@db/keeppix
```

Docker Compose interpola solo i token `${VAR}` che compaiono letteralmente
nel file YAML, prendendone il valore dall'ambiente della shell (o da un file
`.env`) **al momento del parsing**. In questa riga l'unico token
interpolato è `${DB_PASSWORD}`; `DATABASE_URL` non compare mai come
`${DATABASE_URL}`. Di conseguenza, impostare `DATABASE_URL` nella shell prima
di lanciare `docker compose up` **non ha alcun effetto** sul valore che il
container riceve: il servizio `keeppix` proverà comunque a connettersi a
`postgres://keeppix:changeme@db/keeppix`.

Poiché la stessa sezione dice di *omettere* `--profile bundled`, il servizio
`db` non viene nemmeno istanziato: l'hostname `db` non esiste sulla rete
Compose, la connessione fallisce, e con `restart: unless-stopped`
(`compose.yaml:7`) il container entra in un ciclo di riavvio continuo —
esattamente il caso che un installatore che segue questa guida alla lettera
incontrerebbe.

Il commento stesso in `compose.yaml:9-10` ("sostituire questo valore e
omettere `--profile bundled`") indica il flusso corretto: **modificare
`compose.yaml`**, non impostare una variabile d'ambiente. `docs/DEPLOY.md`
contraddice il file che dovrebbe documentare.

Da correggere in `docs/DEPLOY.md`: riscrivere la sezione per dire di editare
direttamente la riga `DATABASE_URL` in `compose.yaml` (coerente col commento
già presente lì), invece di suggerire un override via variabile d'ambiente
che questo file di compose non supporta.

Questo era già presente nel testo letterale del brief (non introdotto
dall'implementer), ma il preflight P4 rendeva vincolante la verifica di
coerenza tra `docs/DEPLOY.md` e i file scritti; il report elenca un controllo
positivo solo per lo scenario *bundled* (punto 9: "`DATABASE_URL` nel
compose combacia col servizio `db`"), non per questo scenario *esterno* che
la stessa guida promette due righe sopra.

### Important (Should Fix)

#### 2. `docs/DEPLOY.md:13` / sezione "Aggiornamento" (righe 56-61) — `DB_PASSWORD` non persistito, rischio di **avvio fallito** dopo un aggiornamento

`export DB_PASSWORD=$(openssl rand -base64 24)` (riga 13) vale solo per la
sessione di shell corrente. La sezione "Aggiornamento" (`git pull` +
`docker compose --profile bundled up -d --build`) non ripete l'export né
menziona un file `.env`. Se un utente aggiorna da un'altra sessione (o dopo
aver chiuso il terminale) senza riesportare `DB_PASSWORD`, Compose usa il
default `changeme` per **entrambi** i servizi che referenziano
`${DB_PASSWORD:-changeme}` (`compose.yaml:11` e `compose.yaml:118`). Il
volume `./pgdata` contiene già un cluster Postgres inizializzato con la
password reale: Postgres **ignora** `POSTGRES_PASSWORD` sui riavvii
successivi al primo `initdb`, quindi il container `db` continua a usare la
password vera, mentre il container `keeppix` viene ricreato con
`DATABASE_URL` che contiene `changeme` — mismatch di autenticazione,
container `keeppix` in crash loop. Non è corruzione dei dati, ma è un guasto
di avvio che sembra un bug della guida.

Suggerimento: raccomandare di scrivere `DB_PASSWORD` in un file `.env` nella
root del progetto (che Compose carica automaticamente per l'interpolazione)
invece di limitarsi a `export` in shell.

### Minor (Nice to Have)

#### 3. `docs/DEPLOY.md` — tabella "Variabili d'ambiente" lascia intendere che tutte siano impostabili a piacere

Solo `DATABASE_URL` e `KEEPPIX_ALLOWED_ORIGINS` sono fissate esplicitamente
in `compose.yaml:9-12`; le altre righe della tabella (`KEEPPIX_BIND`,
`KEEPPIX_DATA_DIR`, `KEEPPIX_DB_MAX_CONNECTIONS`,
`KEEPPIX_SESSION_TTL_SECS`, `KEEPPIX_LOG_FORMAT`, `RUST_LOG`) non hanno un
meccanismo di override documentato nello stack Compose (andrebbero aggiunte
a `environment:` in `compose.yaml`, con un override file, o passate a
`docker run` se non si usa Compose). La tabella da sola non chiarisce come
cambiarle in pratica in questo stack; non è un bug ma una lacuna di
completezza, in parte la stessa causa radice del Critical #1.

## Assessment

**Task quality:** Needs fixes

**Reasoning:** L'implementazione tecnica (Dockerfile, `.dockerignore`,
struttura del compose) è solida e le correzioni rivendicate dall'implementer
sono reali, verificate una per una contro il codice sorgente — non
riscritture cosmetiche. Il difetto trovato è però esattamente nel documento
che il preflight (P4) e le istruzioni del compito indicano come "la porta
d'ingresso per chi installa Keeppix": `docs/DEPLOY.md` promette un comando
per il percorso "Postgres esterno" che non funziona con il `compose.yaml`
scritto in questo stesso commit, e porterebbe un utente reale a un container
in crash loop. Va corretto prima di considerare il task chiuso.
