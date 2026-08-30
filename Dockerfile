# syntax=docker/dockerfile:1.9

# ── Offline geographic datasets ──────────────────────────────────────────
# The distroless runtime has no shell and no HTTP client. The dataset is
# downloaded and normalized here once; only the TSV ready for import into
# Postgres makes it into the final image.
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
# trixie (glibc ≥ 2.38, libstdc++ GCC 14): `ort`'s prebuilt binaries
# (`download-binaries`) are built on Ubuntu 24.04 and reference
# `__isoc23_strtol` / `_M_replace_cold`. On bookworm (glibc 2.36) the link
# fails. Runtime below is aligned to distroless/cc-debian13.
FROM rust:1.88-trixie AS backend
WORKDIR /app

# sqlx queries are verified at runtime using the function forms
# (`sqlx::query(...)`), not the `query!` macros: there's no `.sqlx/` cache
# to copy, and the build doesn't need a database.
#
# The manifests are copied first, but `cargo build --bin keeppix` still
# compiles dependencies and application code in a single pass: modifying
# `crates/` invalidates this layer too, so the separation below doesn't
# buy a "dependencies vs. sources" cache. What actually speeds up repeated
# builds are the caches mounted in the next RUN (`cargo/registry` and
# `target`), which BuildKit keeps across builds independently of the image
# layers.
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
# distroless has no package manager, so the binary and its libraries are
# gathered here and copied into the runtime. Same Debian 13 base as the
# runtime (trixie), so the versions line up with distroless/cc-debian13.
FROM debian:trixie-slim AS libraw
RUN apt-get update \
 && apt-get install -y --no-install-recommends libraw-bin \
 && rm -rf /var/lib/apt/lists/*

# Only the libraries distroless does NOT already provide are copied: the
# base glibc set (libc, libm, libstdc++, libgcc_s, …) is already there, and
# overriding it via LD_LIBRARY_PATH would risk desyncing the libraries from
# the loader. What's left is libraw-specific — including `libgomp`, which
# distroless lacks and without which the binary won't start.
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
# `heif-convert` decodes HEIF/HEIC (8- and 10-bit) in a sandbox, never
# libheif in-process — same pattern as `dcraw_emu` above: `keeppix-media`
# invokes it via `sandbox::run` with `rlimit`, never as an in-process
# binding. Without it, `derive_from_bytes`/`ensure_full_from_bytes` respond
# with `DeriveError::Decode` for every HEIC uploaded to the library (iPhone
# and many recent cameras).
#
# On trixie the HEIF codecs remain reachable via `ldd` on the
# `heif-convert` binary (same collection approach as `dcraw_emu`); if they
# ever move to `dlopen` plugins, the plugin directory would need to be
# added to the staging step.
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
# distroless: no shell, no package manager, ~6 packages to keep an eye on.
# debian13 aligned with the trixie builder (ort prebuilt needs glibc ≥ 2.38).
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

# No curl available: the binary's own subcommand is used instead. Like the
# rest of the binary, `healthcheck` loads configuration via Config::load and
# so requires `DATABASE_URL`: without that variable the container reports
# unhealthy due to a configuration error, not a network one (see
# docs/DEPLOY.md).
HEALTHCHECK --interval=30s --timeout=5s --start-period=20s --retries=3 \
    CMD ["/usr/local/bin/keeppix", "healthcheck"]

ENTRYPOINT ["/usr/local/bin/keeppix"]
CMD ["serve"]
