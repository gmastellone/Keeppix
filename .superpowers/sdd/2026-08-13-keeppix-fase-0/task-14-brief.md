## Task 14: Immagine Docker e compose

**Files:**
- Create: `Dockerfile`, `.dockerignore`, `compose.yaml`
- Create: `docs/DEPLOY.md`

**Interfaces:**
- Consumes: il binario `keeppix` e `frontend/dist`.
- Produces: immagine `keeppix:dev` avviabile con `docker compose --profile bundled up`.

- [ ] **Step 1: Scrivere `.dockerignore`**

```
target
frontend/node_modules
frontend/dist
data
pgdata
.git
docs
*.md
```

- [ ] **Step 2: Scrivere il `Dockerfile`**

```dockerfile
# syntax=docker/dockerfile:1.9

# ── Frontend ──────────────────────────────────────────────────────────────
FROM node:24-bookworm-slim AS frontend
WORKDIR /app/frontend
COPY frontend/package.json frontend/package-lock.json ./
RUN npm ci
COPY frontend/ ./
RUN npm run build

# ── Backend ───────────────────────────────────────────────────────────────
FROM rust:1.85-bookworm AS backend
WORKDIR /app

# Le query sqlx sono verificate contro la cache committata: nessun database
# è necessario in fase di build.
ENV SQLX_OFFLINE=true

# Strato di dipendenze, invalidato solo dai manifest.
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates/ crates/
COPY .sqlx/ .sqlx/
COPY --from=frontend /app/frontend/dist frontend/dist

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo build --release --bin keeppix && \
    cp target/release/keeppix /usr/local/bin/keeppix

# ── Runtime ───────────────────────────────────────────────────────────────
# distroless: nessuna shell, nessun package manager, ~6 pacchetti da monitorare.
FROM gcr.io/distroless/cc-debian12:nonroot AS runtime

COPY --from=backend /usr/local/bin/keeppix /usr/local/bin/keeppix

USER nonroot:nonroot
WORKDIR /data
EXPOSE 5673

ENV KEEPPIX_BIND=0.0.0.0:5673 \
    KEEPPIX_DATA_DIR=/data \
    KEEPPIX_LOG_FORMAT=json

# Nessun curl disponibile: si usa il sottocomando del binario stesso.
HEALTHCHECK --interval=30s --timeout=5s --start-period=20s --retries=3 \
    CMD ["/usr/local/bin/keeppix", "healthcheck"]

ENTRYPOINT ["/usr/local/bin/keeppix"]
CMD ["serve"]
```

- [ ] **Step 3: Scrivere `compose.yaml`**

```yaml
name: keeppix

services:
  keeppix:
    build: .
    image: keeppix:dev
    restart: unless-stopped
    environment:
      # Con un Postgres esterno, sostituire questo valore e omettere
      # `--profile bundled`: il servizio `db` non verrà avviato.
      DATABASE_URL: postgres://keeppix:${DB_PASSWORD:-changeme}@db/keeppix
      KEEPPIX_ALLOWED_ORIGINS: '[]'
    ports:
      - "5673:5673"
    volumes:
      - ./data:/data
      # Originali in sola lettura: nessun bug può cancellarli.
      # Passare a `rw` solo quando servirà l'upload (Fase 1).
      - ${PHOTOS_PATH:-./photos}:/photos:ro
    read_only: true
    tmpfs:
      - /tmp
    security_opt:
      - no-new-privileges:true
    cap_drop:
      - ALL
    depends_on:
      db:
        condition: service_healthy
        required: false

  db:
    profiles: ["bundled"]
    image: postgis/postgis:17-3.5
    restart: unless-stopped
    environment:
      POSTGRES_USER: keeppix
      POSTGRES_PASSWORD: ${DB_PASSWORD:-changeme}
      POSTGRES_DB: keeppix
    volumes:
      - ./pgdata:/var/lib/postgresql/data
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U keeppix -d keeppix"]
      interval: 5s
      timeout: 3s
      retries: 10
```

- [ ] **Step 4: Costruire l'immagine**

Run: `docker build -t keeppix:dev .`
Expected: build completata. Verificare la dimensione:

```bash
docker images keeppix:dev --format '{{.Size}}'
```

Expected: sotto 100 MB (in Fase 0 non c'è ancora ffmpeg).

- [ ] **Step 5: Verificare che l'immagine non abbia shell**

Run: `docker run --rm --entrypoint /bin/sh keeppix:dev -c 'echo ciao'`
Expected: errore `exec: "/bin/sh": stat /bin/sh: no such file or directory`. È il comportamento voluto.

- [ ] **Step 6: Avviare lo stack completo**

```bash
DB_PASSWORD=devpassword docker compose --profile bundled up -d --build
sleep 15
curl -s http://127.0.0.1:5673/health
curl -s http://127.0.0.1:5673/api/v1/setup/status
```

Expected: `{"status":"ok","version":"0.1.0"}` e `{"initialised":false}`.

- [ ] **Step 7: Verificare l'healthcheck del container**

Run: `docker compose ps --format '{{.Name}} {{.Status}}'`
Expected: il servizio `keeppix` riporta `(healthy)`.

- [ ] **Step 8: Verificare il flusso completo nel browser**

Aprire `http://127.0.0.1:5673`, completare il setup, uscire, rientrare. Poi:

```bash
docker compose down && DB_PASSWORD=devpassword docker compose --profile bundled up -d
```

Expected: l'istanza risulta già configurata (`initialised: true`), i dati sono sopravvissuti al riavvio.

- [ ] **Step 9: Scrivere `docs/DEPLOY.md`**

````markdown
# Installazione

## Requisiti

- Docker 24+ con Compose v2
- PostgreSQL 17 con PostGIS 3.5 (incluso, oppure esterno)
- 2 GB di RAM liberi, architettura `amd64` o `arm64`

## Avvio con tutto incluso

```bash
export DB_PASSWORD=$(openssl rand -base64 24)
docker compose --profile bundled up -d
```

Aprire http://127.0.0.1:5673 e completare la creazione dell'amministratore.

## Avvio con un Postgres già esistente

Il database deve avere l'estensione PostGIS disponibile. Omettere il profilo:

```bash
DATABASE_URL=postgres://utente:password@mio-host:5432/keeppix docker compose up -d
```

Il servizio `db` non verrà avviato.

## Variabili d'ambiente

| Variabile | Predefinito | Descrizione |
|---|---|---|
| `DATABASE_URL` | — | **Obbligatoria.** Stringa di connessione a Postgres |
| `KEEPPIX_BIND` | `0.0.0.0:5673` | Indirizzo e porta di ascolto |
| `KEEPPIX_DATA_DIR` | `/data` | Derivati, mappe, backup, `config.toml` |
| `KEEPPIX_DB_MAX_CONNECTIONS` | `10` | Dimensione del pool |
| `KEEPPIX_SESSION_TTL_SECS` | `2592000` | Durata della sessione (30 giorni) |
| `KEEPPIX_LOG_FORMAT` | `json` | `json` o `pretty` |
| `KEEPPIX_ALLOWED_ORIGINS` | `[]` | Origini ammesse, es. `["https://foto.example.com"]` |
| `RUST_LOG` | `info,sqlx=warn` | Verbosità dei log |

Le stesse chiavi sono impostabili in `/data/config.toml` in minuscolo e senza
prefisso. **L'ambiente vince sempre sul file.**

## Volumi

| Percorso | Modo | Contenuto |
|---|---|---|
| `./data` → `/data` | rw | derivati, mappe, backup, configurazione |
| `$PHOTOS_PATH` → `/photos` | **ro** | i tuoi originali |

In Fase 0 non esiste ancora l'indicizzazione: `/photos` è montato in sola
lettura e nulla lo tocca. Passerà a `rw` in Fase 1, solo per le librerie su cui
abiliterai upload o scrittura dei sidecar.

## Aggiornamento

```bash
docker compose pull && docker compose up -d
```

Le migrazioni del database vengono applicate automaticamente all'avvio, in
transazione. Il tag `:1` segue la versione major: gli aggiornamenti al suo
interno non richiedono interventi manuali.

## Dietro un reverse proxy

Keeppix parla HTTP in chiaro e si aspetta che la terminazione TLS avvenga a
monte. Il cookie di sessione usa il prefisso `__Host-`, che **richiede HTTPS**:
senza TLS l'accesso funziona solo da `localhost`.

```nginx
location / {
    proxy_pass http://127.0.0.1:5673;
    proxy_set_header Host $host;
    proxy_set_header X-Forwarded-Proto $scheme;
    proxy_http_version 1.1;
}
```

## Diagnosi

```bash
docker compose logs -f keeppix
curl -s http://127.0.0.1:5673/health
```

L'immagine è distroless e **non contiene shell**: `docker exec ... sh` non
funziona, ed è voluto. Per ispezionarla, usare il tag `:1-debug`.
````

- [ ] **Step 10: Pulire e committare**

```bash
docker compose down -v
git add Dockerfile .dockerignore compose.yaml docs/DEPLOY.md
git commit -m "feat: add distroless docker image and compose stack"
```

---

