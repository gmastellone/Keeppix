#!/usr/bin/env bash
# Suite locale: un crate alla volta, poi via container e `target/`.
# Argomenti extra vanno al test harness (filtro per nome).
set -euo pipefail
cd "$(dirname "$0")/.."

cleanup() {
  if command -v docker >/dev/null 2>&1; then
    docker ps -aq --filter 'label=org.testcontainers.managed-by=testcontainers' \
      | while IFS= read -r id; do
          docker rm -f "$id" >/dev/null 2>&1 || true
        done
  fi
  cargo clean
}
trap cleanup EXIT

# --jobs 1: cargo test --workspace altrimenti lancia più crate in parallelo,
# ognuno con un PostGIS (~1 GB RAM). --test-threads=1: config.rs tocca l'env.
cargo test --workspace --jobs 1 -- --test-threads=1 "$@"
