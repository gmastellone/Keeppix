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

# Nessun curl disponibile: si usa il sottocomando del binario stesso. Come il
# resto del binario, `healthcheck` legge la configurazione con Config::load e
# quindi richiede `DATABASE_URL`: senza quella variabile il container risulta
# unhealthy per un errore di configurazione, non di rete (vedi docs/DEPLOY.md).
HEALTHCHECK --interval=30s --timeout=5s --start-period=20s --retries=3 \
    CMD ["/usr/local/bin/keeppix", "healthcheck"]

ENTRYPOINT ["/usr/local/bin/keeppix"]
CMD ["serve"]
