# Task 14 — Fix round 1 — Re-Review

## Metodo

Letto il diff `review-19d9f22..f6d1e34.diff` (2 file: `compose.yaml`,
`docs/DEPLOY.md`), il brief, la review che ha aperto i finding, e il report
del fix appeso in fondo a `task-14-report.md`. Verificato che HEAD
(`f1ef31f`) non diverge da `f6d1e34` su questi due file
(`git diff f6d1e34 HEAD -- compose.yaml docs/DEPLOY.md` vuoto), quindi ho
lavorato direttamente sul working tree.

Confermato il vincolo d'ambiente: `docker info` fallisce
(`failed to connect to the docker API at unix:///var/run/docker.sock …no
such file or directory`), ma `docker compose config` (Compose v5.1.1) è
client-side e gira senza daemon. L'ho usato per riprodurre empiricamente
ogni scenario, non solo a lettura, incluse le due varianti letterali dei
comandi copiati da `docs/DEPLOY.md`.

## Finding Verdicts

- **Critical — `DATABASE_URL` esterna promessa da `docs/DEPLOY.md` non aveva
  effetto su `compose.yaml`** — ADDRESSED, `compose.yaml:12`:
  `DATABASE_URL: ${DATABASE_URL:-postgres://keeppix:${DB_PASSWORD:-changeme}@db/keeppix}`.
  Verificato con `docker compose config` in tutti e 4 gli scenari richiesti:

  | Scenario | Comando eseguito | `DATABASE_URL` risolta |
  |---|---|---|
  | Bundled, nessuna var | `env -u DATABASE_URL -u DB_PASSWORD -u PHOTOS_PATH docker compose --profile bundled config` | `postgres://keeppix:changeme@db/keeppix`, `db` presente |
  | Bundled, `.env` con `DB_PASSWORD` | `echo "DB_PASSWORD=supersegreta" > .env; env -u DATABASE_URL -u DB_PASSWORD docker compose config` | `postgres://keeppix:supersegreta@db/keeppix` |
  | Esterno, `DATABASE_URL` impostata, no profilo | `env -u DB_PASSWORD DATABASE_URL="postgres://ext:extpass@myhost:5432/keeppix" docker compose config --services` → `keeppix` (solo) | `postgres://ext:extpass@myhost:5432/keeppix` |
  | Entrambe impostate | `env DB_PASSWORD=nonusata DATABASE_URL="postgres://ext:extpass@myhost:5432/keeppix" docker compose config` | `postgres://ext:extpass@myhost:5432/keeppix` (vince `DATABASE_URL`) |

  Ho anche eseguito letteralmente i due comandi copiati da
  `docs/DEPLOY.md:19-21` e `:31-33` (vedi sezione Comandi sotto): la
  password generata da `openssl rand -base64 24` compare identica in
  `POSTGRES_PASSWORD` e in `DATABASE_URL`; il comando "Postgres esterno"
  produce `keeppix` come unico servizio e `DATABASE_URL` esattamente col
  valore scritto in `.env`. Il difetto specifico (comando promesso senza
  effetto) non esiste più.

- **Important — `DB_PASSWORD` non persistita** — ADDRESSED,
  `docs/DEPLOY.md:12-22` sostituisce `export` con
  `echo "DB_PASSWORD=…" > .env`, con la spiegazione del rischio (mismatch
  post-`initdb`). Verificato che un `.env` scritto in una invocazione e
  riletto in un'altra (nuovo processo `env`, simulando una sessione di shell
  diversa) produce lo stesso risultato — persistenza reale, non solo
  documentata: `echo "DB_PASSWORD=dalfileenv" > .env` seguito da un
  `docker compose config` in un `env` pulito risolve
  `DATABASE_URL: postgres://keeppix:dalfileenv@db/keeppix`. La sezione
  "Aggiornamento" (`docs/DEPLOY.md:100-103`) ora dichiara esplicitamente che
  non serve reimpostare nulla, coerente col meccanismo verificato.

- **Minor — tabella variabili non chiariva cosa fosse sovrascrivibile** —
  ADDRESSED, `docs/DEPLOY.md:66-75` aggiunge il paragrafo che distingue le
  due sole variabili impostate esplicitamente (`DATABASE_URL`,
  `KEEPPIX_ALLOWED_ORIGINS`) dal resto, e spiega perché mettere le altre in
  `.env` non ha effetto. Verificato empiricamente: `echo
  "KEEPPIX_BIND=9.9.9.9:1234" > .env` seguito da `docker compose config` non
  fa comparire `KEEPPIX_BIND` da nessuna parte nell'`environment:` risolto
  del servizio `keeppix` — la distinzione documentata è reale, non pedante.

## Verifica addizionale richiesta dal controller

**Coerenza guida/compose su tutti gli scenari promessi:**
- Bundled: verificato sopra (letterale e con variazioni).
- Postgres esterno: verificato sopra (letterale e con variazioni).
- Aggiornamento (`git pull` + `docker compose --profile bundled up -d
  --build`): la persistenza di `.env` fra sessioni è verificata (vedi
  Important sopra); non verificabile il `--build` reale (richiede daemon),
  ma questo non è cambiato dal fix e non era in discussione.
- Dietro reverse proxy: il fix non tocca questa sezione (confermato dal
  diff, nessun hunk su quelle righe); non referenzia variabili Compose,
  quindi non era a rischio di questo genere di incoerenza.

**Effetto collaterale dichiarato dall'implementer** (`.env` di sviluppo
locale con `DATABASE_URL=…localhost…` che romperebbe l'avvio bundled se
letto anche da `docker compose`): riprodotto,

```bash
echo "DATABASE_URL=postgres://keeppix:changeme@localhost:5432/keeppix" > .env
env -u DB_PASSWORD docker compose --profile bundled config | grep DATABASE_URL
# → DATABASE_URL: postgres://keeppix:changeme@localhost:5432/keeppix
```

confermato vero: un `.env` di sviluppo copiato da `.env.example` per
`cargo run` verrebbe effettivamente letto da `docker compose` nella stessa
cartella e romperebbe il bundled (host `localhost` risolve al loopback del
container, non a quello dell'host). L'avvertenza aggiunta in
`docs/DEPLOY.md:41-48` descrive esattamente questo meccanismo e offre due
mitigazioni concrete (`--env-file` diverso, o `DATABASE_URL` solo in shell).
Non elimina il rischio (nessun fix di prodotto lo farebbe senza validazione
applicativa), ma è un avvertimento accurato e collocato nel punto giusto
della guida — sufficiente come mitigazione documentale.

**Rotture nuove — il profilo `bundled` deve continuare a governare `db`:**
confermato invariato,

```bash
env -u DATABASE_URL -u DB_PASSWORD docker compose config --services
# → keeppix
env -u DATABASE_URL -u DB_PASSWORD docker compose --profile bundled config --services
# → db, keeppix
```

Confrontato l'intero `docker compose --profile bundled config` risolto
contro la struttura attesa: `read_only: true`, `tmpfs: [/tmp]`,
`security_opt: [no-new-privileges:true]`, `cap_drop: [ALL]`,
`depends_on.db.condition: service_healthy` + `required: false`, entrambi i
volumi bind (`./data:/data` rw, `./photos:/photos:ro`), porte
`5673:5673`, `healthcheck` del servizio `db` — tutti presenti e identici a
prima del fix. Nessuna riga toccata dal fix ha effetti collaterali sul
resto del file.

## New Breakage in the Fix Diff

Nessuna. Il file `compose.yaml` risultante parsea correttamente
(`docker compose config` valida anche la struttura, non solo lo YAML
grezzo), l'interpolazione annidata `${DATABASE_URL:-...${DB_PASSWORD:-...}}`
è sintassi Compose Specification standard e si comporta come atteso in
tutti gli scenari testati. Nessuna regressione sulle proprietà di sicurezza
(`read_only`, `cap_drop`, `security_opt`) o sui volumi/porte/healthcheck,
tutti confermati identici. `docs/DEPLOY.md` non contiene affermazioni non
verificate: ogni comando bash citato nelle sezioni toccate dal fix è stato
eseguito letteralmente o in una variazione equivalente.

## Out-of-Scope Observations

- `git status --short` sul repository mostra `.github/` e `deny.toml` come
  untracked — non introdotti da questo diff (altri agenti al lavoro, come
  indicato dal controller); non ho toccato né valutato questi file.
- Il nome container assunto in `docs/DEPLOY.md:140`
  (`keeppix-keeppix-1`, usato nell'esempio `docker run --pid
  container:keeppix-keeppix-1 …`) segue la convenzione standard di naming
  di Compose (`<project>-<service>-<index>`, con `name: keeppix` e servizio
  `keeppix`) ma non è stato toccato da questo fix round e non rientra nei
  tre finding: lo segnalo solo per completezza, non è un blocco.

## Verdict

**Fix round:** Tutti i finding indirizzati, nessuna nuova rottura
Critical/Important nel diff del fix. Il Critical è verificato empiricamente
in tutti e 4 gli scenari richiesti (bundled senza var, bundled con
`DB_PASSWORD`, esterno con `DATABASE_URL`, entrambe impostate), incluse le
esecuzioni letterali dei comandi copiati da `docs/DEPLOY.md`. L'Important è
verificato con persistenza reale tramite `.env` fra invocazioni separate.
Il Minor è verificato empiricamente mostrando che le variabili non
referenziate nel compose non filtrano nell'ambiente del container.
L'effetto collaterale dichiarato dall'implementer (`.env` di sviluppo
locale che rompe il bundled) è confermato reale e adeguatamente
documentato. Il profilo `bundled` continua a governare correttamente la
presenza del servizio `db`. Nessun finding resta aperto.

## Comandi eseguiti (evidenza)

```bash
# Ambiente
docker info 2>&1 | tail -3
# → Server: failed to connect to the docker API at unix:///var/run/docker.sock …

docker compose version
# → Docker Compose version v5.1.1

# Scenario 1: bundled, nessuna variabile
env -u DATABASE_URL -u DB_PASSWORD -u PHOTOS_PATH docker compose --profile bundled config
# → DATABASE_URL: postgres://keeppix:changeme@db/keeppix
#   db: presente, POSTGRES_PASSWORD: changeme

env -u DATABASE_URL -u DB_PASSWORD docker compose config --services
# → keeppix
env -u DATABASE_URL -u DB_PASSWORD docker compose --profile bundled config --services
# → db
#   keeppix

# Scenario 2: bundled, .env con DB_PASSWORD
echo "DB_PASSWORD=supersegreta" > .env
env -u DATABASE_URL -u DB_PASSWORD docker compose config | grep DATABASE_URL
# → DATABASE_URL: postgres://keeppix:supersegreta@db/keeppix
rm .env

# Scenario 3: esterno, DATABASE_URL impostata, no profilo
env -u DB_PASSWORD DATABASE_URL="postgres://ext:extpass@myhost:5432/keeppix" docker compose config --services
# → keeppix
env -u DB_PASSWORD DATABASE_URL="postgres://ext:extpass@myhost:5432/keeppix" docker compose config | grep DATABASE_URL
# → DATABASE_URL: postgres://ext:extpass@myhost:5432/keeppix

# Scenario 4: entrambe impostate
env DB_PASSWORD=nonusata DATABASE_URL="postgres://ext:extpass@myhost:5432/keeppix" docker compose config | grep DATABASE_URL
# → DATABASE_URL: postgres://ext:extpass@myhost:5432/keeppix

# Comandi letterali da docs/DEPLOY.md — "Avvio con tutto incluso"
echo "DB_PASSWORD=$(openssl rand -base64 24)" > .env
docker compose --profile bundled config | grep -E "DATABASE_URL|POSTGRES_PASSWORD|POSTGRES_USER|POSTGRES_DB"
# → POSTGRES_DB: keeppix
#   POSTGRES_PASSWORD: 9aDy1rE+HBOvwkXn7j3pmHjQo+xCRcva
#   POSTGRES_USER: keeppix
#   DATABASE_URL: postgres://keeppix:9aDy1rE+HBOvwkXn7j3pmHjQo+xCRcva@db/keeppix
docker compose --profile bundled config --services
# → db
#   keeppix
rm .env

# Comandi letterali da docs/DEPLOY.md — "Avvio con un Postgres già esistente"
echo "DATABASE_URL=postgres://utente:password@mio-host:5432/keeppix" > .env
docker compose config --services
# → keeppix
docker compose config | grep DATABASE_URL
# → DATABASE_URL: postgres://utente:password@mio-host:5432/keeppix
rm .env

# Effetto collaterale: .env di sviluppo locale rompe il bundled (confermato)
echo "DATABASE_URL=postgres://keeppix:changeme@localhost:5432/keeppix" > .env
env -u DB_PASSWORD docker compose --profile bundled config | grep DATABASE_URL
# → DATABASE_URL: postgres://keeppix:changeme@localhost:5432/keeppix
rm .env

# Minor: variabile non referenziata non filtra nel container
echo "KEEPPIX_BIND=9.9.9.9:1234" > .env
docker compose config | grep -A5 "environment:"
# → DATABASE_URL / KEEPPIX_ALLOWED_ORIGINS soltanto, nessuna KEEPPIX_BIND
rm .env

# Nessuna rottura: struttura completa invariata
env -u DATABASE_URL -u DB_PASSWORD -u PHOTOS_PATH docker compose --profile bundled config
# → read_only, tmpfs, security_opt, cap_drop, depends_on, volumi, porte,
#   healthcheck db: tutti presenti e identici alla versione pre-fix

git status --short
# → (pulito lato compose.yaml/docs/DEPLOY.md; nessun residuo .env di test)
```
