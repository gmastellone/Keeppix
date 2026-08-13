# Task 14 — note di pre-volo del controller

Da leggere insieme a `task-14-brief.md`. **Dove queste note contraddicono il
brief, vincono le note.**

## P1 — Il Dockerfile del brief non compila: due ruling già presi lo correggono

**Ruling R2 (toolchain).** Il brief dice `FROM rust:1.85-bookworm`. Il codice
del workspace usa i let-chain (`if let … && …`), stabili solo da **Rust 1.88**:
con 1.85 la build fallisce. `rust-toolchain.toml` e `rust-version` sono già a
1.88.0. Usa **`rust:1.88-bookworm`**.

**Ruling R4 (sqlx).** Il brief contiene:

```dockerfile
ENV SQLX_OFFLINE=true
COPY .sqlx/ .sqlx/
```

Nessuno dei due va tenuto. Il codice usa le forme *funzione* di sqlx
(`sqlx::query(...)`), verificate a runtime, non le macro `query!`: non esiste
alcuna cache `.sqlx/` e non è mai stata generata. `COPY .sqlx/ .sqlx/` su una
directory inesistente **fa fallire la build immediatamente**. Rimuovi entrambe
le righe e il commento che le spiega, e sostituiscilo con una riga onesta: la
build non ha bisogno di un database perché le query non sono verificate a
compile-time.

## P2 — Non puoi verificare l'immagine in questo ambiente, e non devi fingere di averlo fatto

Il pull di immagini Docker è **bloccato dalla policy di egress** (403 al CONNECT
verso il CDN dei blob). Non è transitorio, non riprovarci, non cercare
mirror alternativi. Di conseguenza **tutti** gli step di verifica del brief che
richiedono `docker build`, `docker compose up`, `docker run` o l'healthcheck
sono ineseguibili qui.

Cosa devi fare:

1. **Scrivi comunque tutti gli artefatti** — `Dockerfile`, `.dockerignore`,
   `compose.yaml`, `docs/DEPLOY.md` — completi e corretti.
2. **Verifica staticamente tutto ciò che è verificabile senza Docker**: che ogni
   percorso citato nel `COPY` esista davvero nel repository; che i nomi dei
   servizi, delle variabili d'ambiente e delle porte combacino con
   `crates/keeppix-server/src/config.rs` e con `.env.example`; che il
   sottocomando dell'healthcheck esista davvero (`cargo run -p keeppix-server --
   --help`); che il profilo `bundled` faccia ciò che `docs/DEPLOY.md` promette.
   Elenca nel report ogni controllo statico che hai fatto e il suo esito.
3. **Nel report, dichiara esplicitamente cosa NON hai potuto verificare**, con
   l'elenco puntuale dei comandi che restano da eseguire su una macchina con
   Docker. Non scrivere «immagine costruita» o «stack avviato» per nessuna
   ragione: sarebbe un'affermazione falsa in un documento che qualcuno userà per
   decidere se la fase è chiusa.

Il job `image` della CI (Task 15) costruirà l'immagine su GitHub, dove Docker
c'è: è lì che questo Dockerfile riceverà la sua prima verifica reale.

## P3 — Controlli di coerenza da fare per davvero

Alcuni li ho già notati leggendo il brief; verificali e correggi ciò che non
torna, motivando nel report:

- **`HEALTHCHECK` e `Config::load`.** Il sottocomando `healthcheck` passa da
  `Config::load`, che **richiede `DATABASE_URL`**. Nel container la variabile è
  impostata dal compose, quindi funziona; ma se qualcuno avvia l'immagine senza
  `DATABASE_URL` l'healthcheck fallisce con un errore di configurazione invece
  che di rete. Verifica che `docs/DEPLOY.md` lo dica.
- **`read_only: true` e `KEEPPIX_DATA_DIR=/data`.** Il filesystem del container
  è in sola lettura con `tmpfs` su `/tmp` e un volume su `/data`. Controlla che
  il binario non scriva altrove (log su stdout, nessun file temporaneo fuori da
  `/tmp`).
- **`.dockerignore` esclude `docs` e `*.md`**, ma il Dockerfile non li copia:
  coerente. Verifica però che non escluda nulla che serva alla build — in
  particolare `frontend/package-lock.json`, che il Task 12 ha committato e che
  `npm ci` richiede.
- **Il commento «strato di dipendenze, invalidato solo dai manifest»** è falso
  così com'è scritto: `COPY crates/ crates/` avviene nello stesso strato, quindi
  qualunque modifica al codice invalida la cache delle dipendenze. O separi
  davvero gli strati, o correggi il commento. Non lasciare un commento che
  descrive un'ottimizzazione che il codice non fa.
- **`depends_on: db: required: false`** con `condition: service_healthy`:
  verifica che sia una combinazione valida nella versione di compose che
  documenti, e che il caso «Postgres esterno, profilo non attivo» funzioni
  davvero come descritto.

## P4 — `docs/DEPLOY.md`

Il brief ne contiene il testo. Verifica che ogni comando che promette sia
coerente con i file che hai effettivamente scritto (nomi di servizio, profili,
variabili, percorsi dei volumi) e che i valori predefiniti citati coincidano con
`.env.example` e con `config.rs`. Una guida di installazione che diverge dal
compose è peggio di nessuna guida.

## P5 — Confini

Non toccare `crates/`, `frontend/`, `docs/api/`. Se durante i controlli statici
trovi un difetto nel codice del backend, **segnalalo nel report** invece di
correggerlo: entra nella coda dei finding, non in questo commit.
