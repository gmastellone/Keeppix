# Keeppix

Galleria fotografica **self-hosted**: la famiglia rivede i ricordi senza
pensare a dove sono i file; il professionista tiene i RAW al sicuro, li
seleziona e li mostra ai clienti.

Un solo processo Rust con il frontend incorporato, più PostgreSQL 17 e
PostGIS. Gli originali restano sul disco, in sola lettura; Keeppix non li
riscrive. Hardware minimo dichiarato: Raspberry Pi 5 da 8 GB.

Stato attuale: **Fase 1 chiusa sul branch `fase-1`** (librerie, ingestione,
timeline). Non è ancora su `main`. La [PR #3](https://github.com/gmastellone/Keeppix/pull/3)
è una bozza: merge solo dopo la suite complessiva, e solo se chi mantiene
il progetto lo chiede.

## Avvio

Serve Docker 24+ con Compose v2.20+.

```bash
echo "DB_PASSWORD=$(openssl rand -base64 24)" > .env
docker compose --profile bundled up -d
```

Aprire http://127.0.0.1:5673 e creare l'amministratore.

Con un Postgres già esistente (PostGIS disponibile), imposta `DATABASE_URL`
e ometti il profilo `bundled`. Dettagli, volumi, reverse proxy e arresto:
[`docs/DEPLOY.md`](docs/DEPLOY.md).

## Sviluppo

```bash
cp .env.example .env          # DATABASE_URL verso Postgres 17 + PostGIS
cd frontend && npm ci && npm run build   # rust-embed incorpora dist/ a compile time
cd .. && cargo run -p keeppix-server
```

Frontend in dev (proxy verso `:5673`):

```bash
cd frontend && npm run dev
```

Prima di dichiarare un task chiuso:

```bash
cd frontend && npm ci && npm run build
cd .. && cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
./scripts/test.sh
```

`./scripts/test.sh` serializza i crate (`--jobs 1`), forza `--test-threads=1`,
poi elimina i container testcontainers e `target/`. Non lanciare
`cargo test --workspace` a mano: riempie RAM (un PostGIS per crate) e disco.

## Stack

| Pezzo | Scelta |
|---|---|
| Backend | Rust 1.88, Axum 0.8, sqlx 0.8 |
| Database | PostgreSQL 17 + PostGIS 3.5 |
| Frontend | Vue 3, TypeScript, Vite, Tailwind v4, Reka UI |
| Distribuzione | immagine distroless, non-root, senza shell |
| Licenza | AGPL-3.0-or-later |

Crate: `keeppix-domain`, `keeppix-db` (unico con SQL), `keeppix-media`
(nessun database), `keeppix-api`, `keeppix-jobs`, `keeppix-server`,
`keeppix-dav` (Fase 5), `keeppix-test-support`.

## Fasi

| Fase | Cosa produce | Stato |
|---|---|---|
| 0 | Auth, Docker, CI, frontend setup/login | su `main` |
| 1a–1c | Librerie, ingest, timeline, ricerca, WS | branch `fase-1`, PR #3 |
| 2 | RAW, sidecar XMP, culling | non iniziata |
| 3 | Multiutente, album, link pubblici | non iniziata |
| 4 | Mappe | non iniziata |
| 5 | WebDAV, upload tus | non iniziata |
| 6 | Consolidamento | non iniziata |

Roadmap e contratti congelati:
[`docs/superpowers/plans/2026-08-13-keeppix-roadmap.md`](docs/superpowers/plans/2026-08-13-keeppix-roadmap.md).

## Documentazione

| Documento | Per chi |
|---|---|
| [`AGENTS.md`](AGENTS.md) | Agenti: invarianti e metodo. Leggerlo **prima** di scrivere codice. |
| [`docs/CONTINUE.md`](docs/CONTINUE.md) | Prompt da incollare in una sessione nuova per riprendere il lavoro. |
| [`docs/superpowers/README.md`](docs/superpowers/README.md) | Indice spec, piani, STATO. |
| [`docs/DEPLOY.md`](docs/DEPLOY.md) | Installazione e esercizio. |
| [`docs/api/openapi.json`](docs/api/openapi.json) | Contratto HTTP `/api/v1` (solo aggiunte). |

## Backlog: rinvii e debiti

`scripts/check-wired.py` tiene in [`scripts/wired-exceptions.txt`](scripts/wired-exceptions.txt)
le funzioni e le rotte senza consumatore di produzione, in due sezioni
dichiarate:

- **Rinvii**: il consumatore è in una fase **non ancora eseguita**.
- **Debiti**: spediti in una fase **già chiusa** senza interfaccia. Il
  terzo campo è la fase che li salderà, non quella che li ha introdotti.

Esempi di debito: `/users*` (la 2R dichiarava la gestione utenti fatta,
il frontend ha zero consumatori), `/trash`, `/auth/refresh` (cookie
assoluto di 30 giorni, niente sliding: non si viene buttati fuori a
metà culling). Il probe hardware (`probe()` → `"unprobed"`) ha un
chiamante ma non misura nulla — debito della Fase 1b, saldo in Fase 6.
La guardia non lo prende: cerca chiamanti, non verifica che una funzione
faccia qualcosa.

## Lingue

Interfaccia in italiano e inglese, stesse chiavi. Nessuna stringa utente
hard-coded nei componenti.
