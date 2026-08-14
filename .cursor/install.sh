#!/usr/bin/env bash
# Idempotent setup for the Keeppix Cloud Agent dev environment.
#
# What it prepares:
#   - PostgreSQL 17 + PostGIS 3 (the integration suite needs a real PostGIS
#     server; Docker/testcontainers isn't available in the VM, so we use the
#     repo's KEEPPIX_TEST_DATABASE_URL escape hatch — see
#     crates/keeppix-db/tests/harness/mod.rs — against a local server).
#   - the `keeppix` role + `keeppix` database.
#   - Node 24 (matches .github/workflows/ci.yml) and the built frontend, which
#     keeppix-server embeds via rust-embed and therefore needs at compile time.
#   - the `keeppix` backend binary.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

PG_VERSION=17
DB_URL="postgres://keeppix:changeme@localhost:5432/keeppix"

# --- System packages: PostgreSQL 17 + PostGIS + build deps ------------------
if ! command -v pg_ctlcluster >/dev/null 2>&1; then
  sudo install -d /usr/share/postgresql-common/pgdg
  sudo curl -fsSL https://www.postgresql.org/media/keys/ACCC4CF8.asc \
    -o /usr/share/postgresql-common/pgdg/apt.postgresql.org.asc
  # shellcheck disable=SC1091
  . /etc/os-release
  echo "deb [signed-by=/usr/share/postgresql-common/pgdg/apt.postgresql.org.asc] https://apt.postgresql.org/pub/repos/apt ${VERSION_CODENAME}-pgdg main" \
    | sudo tee /etc/apt/sources.list.d/pgdg.list >/dev/null
  sudo apt-get update -qq
  sudo apt-get install -y -qq \
    "postgresql-${PG_VERSION}" "postgresql-${PG_VERSION}-postgis-3" \
    "postgresql-client-${PG_VERSION}" pkg-config libssl-dev build-essential
fi

# --- Postgres cluster + app role/database -----------------------------------
sudo pg_ctlcluster "${PG_VERSION}" main start 2>/dev/null || true
for _ in $(seq 1 30); do sudo -u postgres pg_isready -q && break; sleep 1; done

# Superuser so the test harness can CREATE DATABASE and CREATE EXTENSION
# postgis in each throwaway database. ponytail: dev-only superuser, never a
# production posture.
sudo -u postgres psql -v ON_ERROR_STOP=1 <<'SQL'
DO $$ BEGIN
  IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'keeppix') THEN
    CREATE ROLE keeppix LOGIN SUPERUSER CREATEDB PASSWORD 'changeme';
  END IF;
END $$;
SQL
sudo -u postgres psql -tqc "SELECT 1 FROM pg_database WHERE datname = 'keeppix'" \
  | grep -q 1 || sudo -u postgres createdb -O keeppix keeppix

# --- Node 24 (matches CI) ---------------------------------------------------
export NVM_DIR="$HOME/.nvm"
# shellcheck disable=SC1091
. "$NVM_DIR/nvm.sh"
nvm install 24 >/dev/null
nvm alias default 24 >/dev/null
nvm use 24 >/dev/null
export PATH="$(dirname "$(nvm which 24)"):$PATH"

# --- Frontend (embedded into the backend via rust-embed) --------------------
( cd "$REPO_ROOT/frontend" && npm ci && npm run build )

# --- Backend ----------------------------------------------------------------
cargo build --bin keeppix

# --- Expose env vars to interactive agent shells ----------------------------
# environment.json has no env field, and the app/tests read real env vars, so
# persist them here. Idempotent via a marked block.
BASHRC="$HOME/.bashrc"
MARKER="# >>> keeppix env >>>"
if ! grep -qF "$MARKER" "$BASHRC"; then
  cat >> "$BASHRC" <<EOF

${MARKER}
export DATABASE_URL="${DB_URL}"
export KEEPPIX_TEST_DATABASE_URL="${DB_URL}"
# <<< keeppix env <<<
EOF
fi

echo "Keeppix install complete."
