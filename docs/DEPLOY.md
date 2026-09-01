# Installation

## Requirements

- Docker 24+ with Compose v2.20+ (needed for `depends_on: … required: false`,
  used to make the `db` service optional)
- PostgreSQL 17 with PostGIS 3.5 (bundled, or external)
- 2 GB of free RAM, `amd64` or `arm64` architecture

The image includes `dcraw_emu` (Debian package `libraw-bin`) along with its
libraries — `libraw`, `liblcms2`, `libjpeg`, `libgomp` — in
`/usr/local/lib/keeppix`, reached via `LD_LIBRARY_PATH`. It's needed for RAW
demosaicing: without it, cameras that embed small previews wouldn't get
thumbnails, and full-resolution zoom in culling would respond with
`503 keeppix/full-unavailable`. It costs ~4 MB on the image.

Anyone who rebuilds the image themselves, or recomposes it on a base
different from the one in the `Dockerfile`, must bring that binary along: it
always runs in a separate process with `rlimit`, never inside the Keeppix
process.

## Starting with everything bundled

`docker compose` automatically reads a `.env` file in the same folder as
`compose.yaml`: use it so you don't have to reset the password every shell
session (an `export` only applies to the current terminal — if you close
it, a subsequent `docker compose up` would fall back to the default value
`changeme`, incompatible with the password already written inside
`./pgdata`).

```bash
echo "DB_PASSWORD=$(openssl rand -hex 24)" > .env
docker compose --profile bundled up -d
```

Open http://127.0.0.1:5673 and complete the administrator setup.

## Starting with an existing Postgres

The database must have the PostGIS extension available. Set
`DATABASE_URL` (in `.env` or in the shell) and omit the profile:

```bash
echo "DATABASE_URL=postgres://user:password@my-host:5432/keeppix" > .env
docker compose up -d
```

`compose.yaml` makes `DATABASE_URL`, when set, win over the value built for
the bundled `db` service; the `db` service still won't start, because it
belongs to the `bundled` profile, which isn't passed to `docker compose`
here.

### pgvector (semantic search and automatic tags)

The bundled Postgres (`Dockerfile.db`, `bundled` profile) already includes
**pgvector** alongside PostGIS. The AI schema migration enables
`CREATE EXTENSION vector` and creates the tables. With an external
Postgres, install your distribution's package for the major version in use
(e.g. `postgresql-17-pgvector` on Debian/Ubuntu) **before** starting
Keeppix with that migration; otherwise the AI schema won't be created (the
gallery still starts fine) and a warning appears at startup with:

```sql
CREATE EXTENSION IF NOT EXISTS vector;
```

If you install pgvector **after** the first startup (migration already
applied as a no-op), rerun the AI schema DDL from
`crates/keeppix-db/migrations/0043_ai_embeddings_tags.sql` by hand, or
restore from a backup taken with the extension already present.

If the extension is **not** installed, Keeppix **starts anyway**: gallery,
upload, and everything else work; AI features remain off. It's not a
blocking configuration error.

Watch out if a `.env` used for local development (copied from
`.env.example` for `cargo run`) already exists in the same folder: Compose
reads that same file, and if that `.env` contains a `DATABASE_URL` pointing
to `localhost`, this section would use that value for the bundled stack too
— inside the container, `localhost` is its own loopback, not the host, and
the connection would fail. In that case use a different `.env` file (for
example with `docker compose --env-file .env.docker …`) or export
`DATABASE_URL` only in the shell from which you launch `docker compose`.

### Face recognition model weights

pgvector fixes the *schema*; face recognition additionally needs the
YuNet+SFace ONNX weights actually present at image build time — without
them the "Face recognition" toggle in Settings turns on with no error, but
never detects a single face. Not committed to git (binary, ~9.5 MB): fetch
them once before building —

```bash
./scripts/download-yunet-sface.sh
docker compose --profile bundled up -d --build
```

The published `ghcr.io` images already include them (the release workflow
runs the same script before building). Semantic search / AI tag matching
needs a second, larger model (OpenCLIP XLM-R IT/EN) that has no equivalent
one-line download yet — see `models/README.md`.

## Environment variables

| Variable | Default | Description |
|---|---|---|
| `DATABASE_URL` | — | **Required.** Postgres connection string |
| `KEEPPIX_BIND` | `0.0.0.0:5673` | Listen address and port |
| `KEEPPIX_DATA_DIR` | `/data` | Derivatives, maps, backups, `config.toml` |
| `KEEPPIX_DB_MAX_CONNECTIONS` | `10` | Pool size |
| `KEEPPIX_SESSION_TTL_SECS` | `2592000` | Session duration (30 days) |
| `KEEPPIX_LOG_FORMAT` | `json` | `json` or `pretty` |
| `KEEPPIX_ALLOWED_ORIGINS` | `[]` | Allowed origins, e.g. `["https://photos.example.com"]` |
| `KEEPPIX_WATCH_POLL_SECS` | `900` | Watcher interval in polling mode (15 min) |
| `KEEPPIX_WEBP_QUALITY` | `82` | WebP quality of lossy derivatives (1–100). Below 75 it starts to show; above 88 you pay for an invisible difference. Thumbnail and preview use the same value. |
| `KEEPPIX_WEBP_METHOD` | `2` | libwebp encode effort (0–6). 0 is fast and larger; 4 is the simple API's default (~2x slower). 2 keeps the derivatives ratio under 1%. |
| `KEEPPIX_FULL_CACHE_BYTES` | `536870912` | Cap on the `full` derivatives cache (full resolution, generated on the first zoom). Default 512 MiB. Without a cap the cache would grow like a trash can that never empties. |
| `KEEPPIX_TRASH_RETENTION_DAYS` | `30` | Days before the trash empties itself. Low priority, once a day. |
| `RUST_LOG` | `info,sqlx=warn,tower_http=info` | Log verbosity |

The same keys can be set in `/data/config.toml`, in lowercase and without
the prefix. **The environment always wins over the file.**

In this `compose.yaml` only `DATABASE_URL` and `KEEPPIX_ALLOWED_ORIGINS` are
set explicitly for the `keeppix` service; the other rows in the table take
the default value already written in the `Dockerfile`
(`KEEPPIX_BIND`, `KEEPPIX_DATA_DIR`, `KEEPPIX_LOG_FORMAT`) or the binary's
default (`KEEPPIX_DB_MAX_CONNECTIONS`, `KEEPPIX_SESSION_TTL_SECS`,
`RUST_LOG`). To change one, add it under `environment:` for the `keeppix`
service in `compose.yaml` (or in a separate override file) — putting it in
`.env` isn't enough, because `.env` only feeds the interpolation of
variables already referenced in the compose file (`DB_PASSWORD`,
`DATABASE_URL`, `PHOTOS_PATH`), not the `keeppix` process's environment
inside the container.

## Postgres tuning (bundled)

The bundled `db` service passes GUC parameters via `command:` —
configurable with environment variables interpolated by Compose (in `.env`
or in the shell). The values **are not universal**: measure them at
install time (Phase 7's hardware probe is the natural place for this).
Here it's enough to document two typical profiles and make everything
overridable.

| Parameter | Factory default | SSD/NVMe (compose default) | microSD |
|---|---|---|---|
| `random_page_cost` | 4.0 | **1.1** | 4.0 |
| `shared_buffers` | 128 MB | ~2 GB | 128 MB |
| `effective_cache_size` | 4 GB | ~6 GB | 4 GB |
| `work_mem` | 4 MB | 32–64 MB | 4 MB |
| `max_connections` | 100 | 20 | 100 |

Compose variables (default values = SSD profile in `compose.yaml`):

| Variable | Compose default | Description |
|---|---|---|
| `POSTGRES_RANDOM_PAGE_COST` | `1.1` | Random page cost for the planner |
| `POSTGRES_SHARED_BUFFERS` | `2GB` | Postgres shared buffer |
| `POSTGRES_EFFECTIVE_CACHE_SIZE` | `6GB` | OS cache estimate for the planner |
| `POSTGRES_WORK_MEM` | `64MB` | Memory per sort/hash operation |
| `POSTGRES_MAX_CONNECTIONS` | `20` | Maximum Postgres connections |

For microSD, set the microSD column's values in `.env` (e.g.
`POSTGRES_RANDOM_PAGE_COST=4.0`). With an external Postgres, apply the same
parameters in the instance's configuration.

The `assets` table has `autovacuum_vacuum_scale_factor = 0.05` (migration
`0033`): visibility maps stay fresh for index-only scans. After a massive
import, the scheduler also queues an immediate `VACUUM ANALYZE` (in
addition to the nightly run).

## Volumes

| Path | Mode | Content |
|---|---|---|
| `./data` → `/data` | rw | derivatives, maps, backups, configuration |
| `$PHOTOS_PATH` → `/photos` | **ro** | your originals |

In Phase 0 indexing doesn't exist yet: `/photos` is mounted read-only and
nothing touches it. It will become `rw` in Phase 1, only for libraries on
which you enable upload or sidecar writes.

## Updating

```bash
git pull
docker compose --profile bundled up -d --build
```

Database migrations are applied automatically at startup, in a
transaction. `compose.yaml` builds the image locally (`keeppix:dev`) and
doesn't point to a registry: `docker compose pull` won't fetch anything new
until the project publishes a remote image to a registry.

If you followed the advice above and wrote `DB_PASSWORD` (or
`DATABASE_URL`) in `.env`, updating doesn't require resetting anything:
Compose reads it again automatically on every `up`, even from a different
shell session than the first startup.

### If startup fails with a migration checksum error

```
error: migration 1 was previously applied but has been modified
```

Migration `0001` was modified during Phase 0 development, before any
release, to enable the PostGIS extension needed for maps. sqlx compares
the checksum of every already-applied migration and refuses to continue if
it doesn't match — it's the safeguard that prevents a schema from silently
diverging from the code.

If you have a database created from an earlier checkout, there's nothing
to save: it's a development install with no indexed photos. Recreate it.

```bash
docker compose --profile bundled down -v
docker compose --profile bundled up -d
```

This will never affect a real installation: from the first release
onward, already-published migrations are never touched again, and schema
changes only arrive as new files.

## Stopping

```bash
# Stops everything: the application and the bundled database.
docker compose --profile bundled down

# Same as above, also removing anonymous volumes (the data in ./pgdata and
# ./data are bind mounts and stay on disk regardless).
docker compose --profile bundled down -v
```

**The profile must be repeated to stop too, not only to start.** A
`docker compose down` without `--profile bundled` removes the `keeppix`
service and **leaves the database running** (verified: `keeppix-db-1`
stays `Up (healthy)`, and the network isn't removed because it's still in
use by that container). This is normal Compose behavior — services in an
inactive profile aren't considered — but the consequence is that anyone
who thinks they've shut down Keeppix ends up with Postgres still running
on their data.

With an external Postgres the profile isn't needed, either to start or to
stop: `docker compose down` is enough.

## Behind a reverse proxy

Keeppix speaks plain HTTP and expects TLS termination to happen upstream.
The session cookie uses the `__Host-` prefix, which **requires HTTPS**:
without TLS, access only works from `localhost`.

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

## Diagnostics

```bash
docker compose logs -f keeppix
curl -s http://127.0.0.1:5673/health
```

The container exposes a `HEALTHCHECK` that runs `keeppix healthcheck`.
This subcommand loads the same configuration as the server and therefore
also requires `DATABASE_URL`: if the variable is missing, the container
shows as `unhealthy` due to a configuration error, not a network one. In
the `compose.yaml` stack the variable is always set by the `keeppix`
service; if you run the "bare" image (without compose), set it yourself.

The image is distroless and **contains no shell**: `docker exec ... sh`
doesn't work, and that's intentional (no pipeline of this project
currently publishes a "debug" variant with a shell). To inspect a running
container, share its process namespace from an image that has a shell:

```bash
docker run --rm -it --pid container:keeppix-keeppix-1 --network container:keeppix-keeppix-1 busybox sh
```
