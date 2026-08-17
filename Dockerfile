# syntax=docker/dockerfile:1.9

# ── Frontend ──────────────────────────────────────────────────────────────
FROM node:24-bookworm-slim AS frontend
WORKDIR /app/frontend
COPY frontend/package.json frontend/package-lock.json ./
RUN npm ci
COPY frontend/ ./
RUN npm run build

# ── Backend ───────────────────────────────────────────────────────────────
FROM rust:1.88-bookworm AS backend
WORKDIR /app

# Le query sqlx sono verificate a runtime con le forme funzione
# (`sqlx::query(...)`), non con le macro `query!`: non esiste alcuna cache
# `.sqlx/` da copiare, e la build non ha bisogno di un database.
#
# I manifest sono copiati per primi, ma `cargo build --bin keeppix` compila
# comunque dipendenze e codice applicativo in un solo passaggio: modificare
# `crates/` invalida anche questo layer, quindi la separazione qui sotto non
# regala una cache "dipendenze vs sorgenti". Il vero acceleratore delle build
# ripetute sono le cache montate nel RUN successivo (`cargo/registry` e
# `target`), che BuildKit conserva tra una build e l'altra indipendentemente
# dai layer dell'immagine.
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates/ crates/
COPY --from=frontend /app/frontend/dist frontend/dist

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo build --release --bin keeppix && \
    cp target/release/keeppix /usr/local/bin/keeppix

# ── libraw ────────────────────────────────────────────────────────────────
# `dcraw_emu` serve al demosaic dei RAW: `keeppix-media` lo invoca come
# processo separato con `rlimit` (mai in-process, vedi AGENTS.md). Senza di
# esso `derive_raw` non può ripiegare sul demosaic quando l'anteprima
# incorporata è troppo piccola, e `GET /media/full` risponde 503
# `keeppix/full-unavailable`: lo zoom del culling sui RAW non funziona.
#
# distroless non ha package manager, quindi il binario e le sue librerie si
# raccolgono qui e si copiano nel runtime. Stessa base Debian 12 del runtime,
# così le versioni combaciano.
FROM debian:bookworm-slim AS libraw
RUN apt-get update \
 && apt-get install -y --no-install-recommends libraw-bin \
 && rm -rf /var/lib/apt/lists/*

# Si copiano solo le librerie che distroless NON fornisce già: il set glibc di
# base (libc, libm, libstdc++, libgcc_s, …) c'è, e sovrascriverlo via
# LD_LIBRARY_PATH rischierebbe di disallineare le librerie dal loader.
# Restano quelle specifiche di libraw — fra cui `libgomp`, che distroless non
# ha e senza la quale il binario non parte.
RUN set -eu; \
    mkdir -p /staging/bin /staging/lib; \
    cp /usr/bin/dcraw_emu /staging/bin/; \
    ldd /usr/bin/dcraw_emu \
      | awk '/=> \//{print $3}' \
      | grep -Ev '/(libc|libm|libdl|librt|libpthread|libstdc\+\+|libgcc_s)\.so' \
      | sort -u \
      | xargs -r -I{} cp -L {} /staging/lib/; \
    ls -1 /staging/lib

# ── Runtime ───────────────────────────────────────────────────────────────
# distroless: nessuna shell, nessun package manager, ~6 pacchetti da monitorare.
FROM gcr.io/distroless/cc-debian12:nonroot AS runtime

COPY --from=backend /usr/local/bin/keeppix /usr/local/bin/keeppix
COPY --from=libraw /staging/bin/dcraw_emu /usr/bin/dcraw_emu
COPY --from=libraw /staging/lib/ /usr/local/lib/keeppix/

USER nonroot:nonroot
WORKDIR /data
EXPOSE 5673

ENV KEEPPIX_BIND=0.0.0.0:5673 \
    KEEPPIX_DATA_DIR=/data \
    KEEPPIX_LOG_FORMAT=json \
    LD_LIBRARY_PATH=/usr/local/lib/keeppix

# Nessun curl disponibile: si usa il sottocomando del binario stesso. Come il
# resto del binario, `healthcheck` legge la configurazione con Config::load e
# quindi richiede `DATABASE_URL`: senza quella variabile il container risulta
# unhealthy per un errore di configurazione, non di rete (vedi docs/DEPLOY.md).
HEALTHCHECK --interval=30s --timeout=5s --start-period=20s --retries=3 \
    CMD ["/usr/local/bin/keeppix", "healthcheck"]

ENTRYPOINT ["/usr/local/bin/keeppix"]
CMD ["serve"]
