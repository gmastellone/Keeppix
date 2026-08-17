#!/usr/bin/env bash
# Suite locale: un crate alla volta, container via tra un crate e l'altro,
# poi `cargo clean`. Argomenti extra vanno al test harness (filtro per nome).
# Compatibile con bash 3.2 (macOS /bin/bash) — niente `mapfile`.
set -euo pipefail
cd "$(dirname "$0")/.."

cleanup_containers() {
  if command -v docker >/dev/null 2>&1 && docker info >/dev/null 2>&1; then
    docker ps -aq --filter 'label=org.testcontainers.managed-by=testcontainers' \
      | while IFS= read -r id; do
          docker rm -f "$id" >/dev/null 2>&1 || true
        done
  fi
}

cleanup() {
  cleanup_containers
  cargo clean
}
trap cleanup EXIT

# Un PostGIS per processo di test: --jobs 1 non basta se Drop non rimuove il
# container alla fine del crate. Si gira crate per crate e si fa rm subito.
pkgs=()
while IFS= read -r pkg; do
  pkgs+=("$pkg")
done < <(
  cargo metadata --format-version=1 --no-deps \
    | python3 -c 'import json,sys; [print(p["name"]) for p in json.load(sys.stdin)["packages"]]'
)

for pkg in "${pkgs[@]}"; do
  cargo test -p "$pkg" --jobs 1 -- --test-threads=1 "$@"
  cleanup_containers
done
