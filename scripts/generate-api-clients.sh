#!/usr/bin/env bash
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
SPEC="$ROOT/docs/api/openapi.json"
OUT_BASE="$ROOT/docs/api/clients"
GEN_IMAGE="${OPENAPI_GENERATOR_IMAGE:-openapitools/openapi-generator-cli:v7.15.0}"

mkdir -p "$OUT_BASE"

if [[ "${UPDATE_OPENAPI:-0}" == "1" ]]; then
  (
    cd "$ROOT"
    UPDATE_OPENAPI=1 cargo test -p keeppix-api --test openapi openapi_snapshot_matches_the_committed_file -- --exact --test-threads=1
  )
fi

if [[ ! -f "$SPEC" ]]; then
  echo "missing spec at $SPEC" >&2
  exit 1
fi

docker run --rm \
  -u "$(id -u):$(id -g)" \
  -v "$ROOT:/local" \
  "$GEN_IMAGE" generate \
  -i /local/docs/api/openapi.json \
  -g typescript-fetch \
  -o /local/docs/api/clients/typescript-fetch \
  --additional-properties=typescriptThreePlus=true,npmName=keeppix-api-client

docker run --rm \
  -u "$(id -u):$(id -g)" \
  -v "$ROOT:/local" \
  "$GEN_IMAGE" generate \
  -i /local/docs/api/openapi.json \
  -g swift5 \
  -o /local/docs/api/clients/swift5 \
  --additional-properties=projectName=KeeppixApiClient,responseAs=AsyncAwait

echo "generated clients under $OUT_BASE"
