# Installazione

## Requisiti

- Docker 24+ con Compose v2.20+ (serve per `depends_on: … required: false`,
  usato per rendere opzionale il servizio `db`)
- PostgreSQL 17 con PostGIS 3.5 (incluso, oppure esterno)
- 2 GB di RAM liberi, architettura `amd64` o `arm64`

## Avvio con tutto incluso

`docker compose` legge automaticamente un file `.env` nella stessa cartella
di `compose.yaml`: usalo per non dover reimpostare la password a ogni
sessione di shell (un `export` vale solo per il terminale corrente — se lo
richiudi, un `docker compose up` successivo ricadrebbe sul valore
predefinito `changeme`, incompatibile con la password già scritta dentro
`./pgdata`).

```bash
echo "DB_PASSWORD=$(openssl rand -base64 24)" > .env
docker compose --profile bundled up -d
```

Aprire http://127.0.0.1:5673 e completare la creazione dell'amministratore.

## Avvio con un Postgres già esistente

Il database deve avere l'estensione PostGIS disponibile. Impostare
`DATABASE_URL` (in `.env` o nella shell) e omettere il profilo:

```bash
echo "DATABASE_URL=postgres://utente:password@mio-host:5432/keeppix" > .env
docker compose up -d
```

`compose.yaml` fa vincere `DATABASE_URL`, quando è impostata, sul valore
costruito per il servizio `db` bundled; il servizio `db` comunque non verrà
avviato, perché appartiene al profilo `bundled` che qui non è passato a
`docker compose`.

Attenzione se nella stessa cartella esiste già un `.env` usato per lo
sviluppo locale (copiato da `.env.example` per `cargo run`): Compose legge
lo stesso file, e se quel `.env` contiene un `DATABASE_URL` puntato a
`localhost` questa sezione userebbe quel valore anche per lo stack bundled
— dentro al container `localhost` è il suo stesso loopback, non l'host, e
la connessione fallirebbe. In quel caso usa un file `.env` diverso (per
esempio con `docker compose --env-file .env.docker …`) o esporta
`DATABASE_URL` solo nella shell da cui lanci `docker compose`.

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
| `KEEPPIX_WATCH_POLL_SECS` | `900` | Intervallo del watcher in modo polling (15 min) |
| `RUST_LOG` | `info,sqlx=warn,tower_http=info` | Verbosità dei log |

Le stesse chiavi sono impostabili in `/data/config.toml` in minuscolo e senza
prefisso. **L'ambiente vince sempre sul file.**

In questo `compose.yaml` solo `DATABASE_URL` e `KEEPPIX_ALLOWED_ORIGINS` sono
impostate esplicitamente per il servizio `keeppix`; le altre righe della
tabella prendono il valore predefinito già scritto nel `Dockerfile`
(`KEEPPIX_BIND`, `KEEPPIX_DATA_DIR`, `KEEPPIX_LOG_FORMAT`) o quello del
binario (`KEEPPIX_DB_MAX_CONNECTIONS`, `KEEPPIX_SESSION_TTL_SECS`,
`RUST_LOG`). Per cambiarne una, aggiungila sotto `environment:` del servizio
`keeppix` in `compose.yaml` (o in un file di override separato) — non basta
metterla in `.env`, perché `.env` alimenta solo l'interpolazione delle
variabili già referenziate nel file di compose (`DB_PASSWORD`, `DATABASE_URL`,
`PHOTOS_PATH`), non l'ambiente del processo `keeppix` dentro al container.

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

Se hai seguito il consiglio sopra e scritto `DB_PASSWORD` (o `DATABASE_URL`)
in `.env`, l'aggiornamento non richiede di reimpostare nulla: Compose lo
rilegge da solo a ogni `up`, anche da una sessione di shell diversa da quella
del primo avvio.

### Se l'avvio fallisce con un errore di checksum sulla migrazione

```
error: migration 1 was previously applied but has been modified
```

La migrazione `0001` è stata modificata durante lo sviluppo della Fase 0, prima
di qualsiasi rilascio, per abilitare l'estensione PostGIS che serve alle mappe.
sqlx confronta il checksum di ogni migrazione già applicata e rifiuta di
proseguire se non coincide — è la protezione che impedisce a uno schema di
divergere silenziosamente dal codice.

Se hai un database creato da un checkout precedente, non c'è nulla da salvare:
è un'installazione di sviluppo senza foto indicizzate. Ricrealo.

```bash
docker compose --profile bundled down -v
docker compose --profile bundled up -d
```

Questo non riguarderà mai un'installazione reale: dal primo rilascio in poi le
migrazioni già pubblicate non vengono più toccate, e i cambiamenti di schema
arrivano solo come nuovi file.

## Arresto

```bash
# Ferma tutto: applicazione e database bundled.
docker compose --profile bundled down

# Come sopra, cancellando anche i volumi anonimi (i dati in ./pgdata e ./data
# sono bind mount e restano su disco comunque).
docker compose --profile bundled down -v
```

**Il profilo va ripetuto anche per fermare, non solo per avviare.** Un
`docker compose down` senza `--profile bundled` rimuove il servizio `keeppix` e
**lascia il database in esecuzione** (verificato: `keeppix-db-1` resta `Up
(healthy)`, e la rete non viene rimossa perché ancora in uso da quel
container). È il comportamento normale di Compose — i servizi di un profilo non
attivo non vengono considerati — ma la conseguenza è che chi crede di aver
spento Keeppix si ritrova Postgres acceso sui propri dati.

Con un Postgres esterno il profilo non serve, né all'avvio né all'arresto:
`docker compose down` è sufficiente.

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

location /api/v1/ws {
    proxy_pass http://127.0.0.1:5673;
    proxy_http_version 1.1;
    proxy_set_header Upgrade $http_upgrade;
    proxy_set_header Connection "upgrade";
    proxy_set_header Host $host;
    proxy_set_header Origin $http_origin;
    proxy_read_timeout 3600s;
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
