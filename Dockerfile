# syntax=docker/dockerfile:1.9

# ── Offline geographic datasets ──────────────────────────────────────────
# Il runtime distroless non ha shell né client HTTP. Il dataset viene
# scaricato e normalizzato qui una volta sola; nell'immagine finale entra
# soltanto il TSV pronto per l'import in Postgres.
FROM debian:bookworm-slim AS geonames
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl gawk python3 unzip \
 && rm -rf /var/lib/apt/lists/*
COPY scripts/build-geonames.sh /usr/local/bin/build-geonames
COPY scripts/build-tz-boundaries.sh /usr/local/bin/build-tz-boundaries
COPY scripts/build-tz-boundaries.py /usr/local/bin/build-tz-boundaries.py
RUN /usr/local/bin/build-geonames /usr/share/keeppix/places.csv \
 && /usr/local/bin/build-tz-boundaries /usr/share/keeppix/tz_boundaries.csv

# ── Frontend ──────────────────────────────────────────────────────────────
FROM node:24-bookworm-slim AS frontend
WORKDIR /app/frontend
COPY frontend/package.json frontend/package-lock.json ./
RUN npm ci
COPY frontend/ ./
RUN npm run build

# ── Backend ───────────────────────────────────────────────────────────────
# trixie (glibc ≥ 2.38, libstdc++ GCC 14): i binari prebuilt di `ort`
# (`download-binaries`) sono compilati su Ubuntu 24.04 e referenziano
# `__isoc23_strtol` / `_M_replace_cold`. Su bookworm (glibc 2.36) il link
# fallisce. Runtime sotto allineato a distroless/cc-debian13.
FROM rust:1.88-trixie AS backend
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
# `dcraw_emu` handles RAW demosaicing: `keeppix-media` invokes it as a
# separate process with `rlimit` (never in-process — it's C code decoding
# untrusted files). Without it, `derive_raw` can't fall back to demosaicing
# when the embedded preview is too small, and `GET /media/full` responds 503
# `keeppix/full-unavailable`: RAW zoom in culling stops working.
#
# distroless non ha package manager, quindi il binario e le sue librerie si
# raccolgono qui e si copiano nel runtime. Stessa base Debian 13 del runtime
# (trixie), così le versioni combaciano con distroless/cc-debian13.
FROM debian:trixie-slim AS libraw
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

# ── libheif ───────────────────────────────────────────────────────────────
# `heif-convert` decodifica HEIF/HEIC (8 e 10 bit, Task 22) in sandbox, mai
# libheif in processo — stesso schema di `dcraw_emu` sopra: `keeppix-media`
# lo invoca via `sandbox::run` con `rlimit`, mai come binding in-process.
# Senza di esso `derive_from_bytes`/`ensure_full_from_bytes` rispondono
# `DeriveError::Decode` per ogni HEIC caricato in libreria (iPhone e molte
# fotocamere recenti).
#
# Su trixie i codec HEIF restano raggiungibili via `ldd` sul binario
# `heif-convert` (stessa raccolta di `dcraw_emu`); se un giorno passassero
# a plugin `dlopen`, andrebbe aggiunta la directory dei plugin allo staging.
FROM debian:trixie-slim AS heif
RUN apt-get update \
 && apt-get install -y --no-install-recommends libheif-examples \
 && rm -rf /var/lib/apt/lists/*

RUN set -eu; \
    mkdir -p /staging/bin /staging/lib; \
    cp /usr/bin/heif-convert /staging/bin/; \
    ldd /usr/bin/heif-convert \
      | awk '/=> \//{print $3}' \
      | grep -Ev '/(libc|libm|libdl|librt|libpthread|libstdc\+\+|libgcc_s)\.so' \
      | sort -u \
      | xargs -r -I{} cp -L {} /staging/lib/; \
    ls -1 /staging/lib

# ── Runtime ───────────────────────────────────────────────────────────────
# distroless: nessuna shell, nessun package manager, ~6 pacchetti da monitorare.
# debian13 allineato al builder trixie (ort prebuilt richiede glibc ≥ 2.38).
FROM gcr.io/distroless/cc-debian13:nonroot AS runtime

COPY --from=backend /usr/local/bin/keeppix /usr/local/bin/keeppix
COPY --from=libraw /staging/bin/dcraw_emu /usr/bin/dcraw_emu
COPY --from=libraw /staging/lib/ /usr/local/lib/keeppix/
COPY --from=heif /staging/bin/heif-convert /usr/bin/heif-convert
COPY --from=heif /staging/lib/ /usr/local/lib/keeppix/
COPY --from=geonames --chown=nonroot:nonroot /usr/share/keeppix/places.csv /usr/share/keeppix/places.csv
COPY --from=geonames --chown=nonroot:nonroot /usr/share/keeppix/tz_boundaries.csv /usr/share/keeppix/tz_boundaries.csv

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
