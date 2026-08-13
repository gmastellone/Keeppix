# Installazione

## Requisiti

- Docker 24+ con Compose v2.20+ (serve per `depends_on: … required: false`,
  usato per rendere opzionale il servizio `db`)
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
| `RUST_LOG` | `info,sqlx=warn,tower_http=info` | Verbosità dei log |

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
git pull
docker compose --profile bundled up -d --build
```

Le migrazioni del database vengono applicate automaticamente all'avvio, in
transazione. `compose.yaml` costruisce l'immagine in locale (`keeppix:dev`) e
non punta a un registro: `docker compose pull` non recupera nulla di nuovo
finché il progetto non pubblicherà un'immagine remota su un registro.

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

Il container espone un `HEALTHCHECK` che esegue `keeppix healthcheck`. Questo
sottocomando carica la stessa configurazione del server e quindi richiede
anch'esso `DATABASE_URL`: se la variabile manca, il container risulta
`unhealthy` per un errore di configurazione, non di rete. Nello stack di
`compose.yaml` la variabile è sempre impostata dal servizio `keeppix`; se
esegui l'immagine "nuda" (senza compose), impostala tu.

L'immagine è distroless e **non contiene shell**: `docker exec ... sh` non
funziona, ed è voluto (nessuna pipeline di questo progetto pubblica al
momento una variante "debug" con shell). Per ispezionare un container in
esecuzione, condividi il suo namespace di processo da un'immagine con shell:

```bash
docker run --rm -it --pid container:keeppix-keeppix-1 --network container:keeppix-keeppix-1 busybox sh
```
