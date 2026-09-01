#!/usr/bin/env bash
# Single installer for Keeppix — Docker (bundled Postgres or your own),
# either for a production/quick-try run or for local development with hot
# reload. The mode and the database are both decided by this script, not
# by which file you happen to run.
#
# Writes real secrets to `.env`, actually verifies the database connection
# (the app's own startup — it applies its migrations and exits on a bad
# connection, so booting it for real is the test, not a separate check
# that could give a different answer), and only then confirms readiness.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_DIR"

# ── Look ─────────────────────────────────────────────────────────────────
if [ -t 1 ]; then
  BOLD=$'\033[1m'; DIM=$'\033[2m'
  BLUE=$'\033[38;5;39m'; GREEN=$'\033[38;5;42m'; YELLOW=$'\033[38;5;220m'
  RED=$'\033[38;5;203m'; CYAN=$'\033[38;5;51m'; NC=$'\033[0m'
else
  BOLD=""; DIM=""; BLUE=""; GREEN=""; YELLOW=""; RED=""; CYAN=""; NC=""
fi
INTERACTIVE=1
[ -t 0 ] || INTERACTIVE=0

# Braille spinner glyphs render as mangled bytes under a non-UTF-8 locale
# (e.g. LC_CTYPE=C, common on minimal/headless hosts — exactly the kind of
# machine this installer targets). Fall back to plain ASCII there instead.
SPIN_FRAMES='⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏'
case "$(locale charmap 2>/dev/null)" in
  UTF-8|utf8) ;;
  *) SPIN_FRAMES='|/-\' ;;
esac

ok()    { echo -e "  ${GREEN}✓${NC} $*"; }
info()  { echo -e "  ${DIM}${NC}$*"; }
warn()  { echo -e "  ${YELLOW}!${NC} $*"; }
fail()  { echo -e "  ${RED}✗${NC} $*" >&2; exit 1; }
title() { echo -e "\n${BOLD}${BLUE}$*${NC}"; }

# A `read` with a default, and — if not interactive (pipe/CI) — the
# default without blocking. Never leave the script hanging on a TTY that
# isn't there.
ask() {
  local prompt="$1" default="${2:-}" __var="$3" reply
  if [ "$INTERACTIVE" = 0 ]; then printf -v "$__var" '%s' "$default"; return; fi
  if [ -n "$default" ]; then
    read -r -p "$(echo -e "  ${CYAN}?${NC} ${prompt} ${DIM}[${default}]${NC} ")" reply
  else
    read -r -p "$(echo -e "  ${CYAN}?${NC} ${prompt} ")" reply
  fi
  printf -v "$__var" '%s' "${reply:-$default}"
}

# Numbered menu: `menu "Question" "1) first option" "2) second option" -- __var`
# prints each option on its own line, then asks for the number (default 1).
menu() {
  local question="$1"; shift
  local opts=() __var=""
  while [ "$1" != "--" ]; do opts+=("$1"); shift; done
  shift
  __var="$1"
  echo -e "  ${CYAN}?${NC} ${question}"
  local o
  for o in "${opts[@]}"; do echo -e "      ${DIM}${o}${NC}"; done
  ask "Choice" "1" "$__var"
}

# Runs "$@" in the background with a spinner in place of its output —
# `docker compose build` prints a line per layer, and none of that is
# useful while things are still working. All output stays saved in "$2":
# the caller decides whether to show it (on failure) or throw it away (on
# success).
run_quiet() {
  local label="$1" logfile="$2"; shift 2
  "$@" > "$logfile" 2>&1 &
  local pid=$! spin="$SPIN_FRAMES" i=0
  while kill -0 "$pid" 2>/dev/null; do
    printf "\r  %s %s  " "${spin:i%${#spin}:1}" "$label"
    i=$((i + 1))
    sleep 0.15
  done
  wait "$pid"
  local status=$?
  printf "\r\033[K"
  return $status
}

clear 2>/dev/null || true
echo -e "\n  ${BLUE}${BOLD}KEEPPIX${NC}"
echo -e "  ${DIM}Self-hosted photo gallery — open source${NC}\n"
echo -e "  This installer writes real secrets to ${BOLD}.env${NC}, actually verifies the"
echo -e "  database connection, and then starts everything."
echo -e "  ${DIM}Ctrl+C at any point leaves nothing touched until you confirm a step.${NC}"

# ── 1. Mode ──────────────────────────────────────────────────────────────
title "1/5 — Mode"

menu "How do you want to run Keeppix?" \
  "1) Docker — recommended, isolated, production or a quick try" \
  "2) Local development — hot reload on backend and frontend (needs Rust + Node.js)" \
  -- MODE_CHOICE
if [ "$MODE_CHOICE" = "2" ]; then MODE="dev"; else MODE="docker"; fi
ok "Mode: $([ "$MODE" = docker ] && echo 'Docker' || echo 'local development')"

# ── 2. Prerequisites ─────────────────────────────────────────────────────
title "2/5 — Prerequisites"

command -v docker >/dev/null 2>&1 || fail "docker not found — https://docs.docker.com/get-docker/"
ok "docker $(docker --version | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' | head -1)"

DOCKER_COMPOSE_CMD=""
if docker compose version >/dev/null 2>&1; then
  DOCKER_COMPOSE_CMD="docker compose"
elif command -v docker-compose >/dev/null 2>&1; then
  DOCKER_COMPOSE_CMD="docker-compose"
else
  fail "docker compose not found — update Docker Desktop or install the compose plugin."
fi
ok "$DOCKER_COMPOSE_CMD"

command -v openssl >/dev/null 2>&1 || fail "openssl not found — needed to generate the database password."
ok "openssl"

docker info >/dev/null 2>&1 || fail "Docker isn't responding — is the daemon running (Docker Desktop / dockerd)?"
ok "Docker daemon reachable"

if [ "$MODE" = "dev" ]; then
  command -v cargo >/dev/null 2>&1 || fail "cargo not found — only needed for local development (Docker alone doesn't need it). https://rustup.rs"
  ok "cargo $(cargo --version | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' | head -1)"
  command -v node >/dev/null 2>&1 || fail "node not found — only needed for local development."
  ok "node $(node --version)"
  command -v npm >/dev/null 2>&1 || fail "npm not found."
  ok "npm $(npm --version)"
fi

# ── 3. Existing .env? ────────────────────────────────────────────────────
title "3/5 — Configuration"

# An existing .env carries the bundled Postgres's password (needed to match
# an already-initialized Docker volume — see the reset flow further down).
# A reused .env keeps it; starting fresh means a new password, which only
# works against a fresh (or reset) database.
FRESH_ENV=1
if [ -f .env ]; then
  menu "An existing .env was found." \
    "1) Reuse it — keep the database password already set" \
    "2) Start fresh — back up the old one, ask everything again" \
    -- ENV_CHOICE
  if [ "$ENV_CHOICE" = "1" ]; then FRESH_ENV=0; fi
fi

is_weak() { [ -z "${1:-}" ] || [[ "$1" =~ ^change_?me$ ]]; }
env_get() { [ "$FRESH_ENV" = 1 ] && return; [ -f .env ] && grep -E "^${1}=" .env | tail -1 | cut -d= -f2- || true; }

DB_PASSWORD="$(env_get DB_PASSWORD)"
PHOTOS_PATH="$(env_get PHOTOS_PATH)"; PHOTOS_PATH="${PHOTOS_PATH:-./photos}"

# ── 4. Database ──────────────────────────────────────────────────────────
title "4/5 — Database"

menu "Database:" \
  "1) Bundled Postgres + PostGIS, managed by Docker — recommended" \
  "2) I already have a reachable Postgres 17 + PostGIS 3.5" \
  -- DB_CHOICE
DB_MODE="bundled"
EXTERNAL_DATABASE_URL=""
if [ "$DB_CHOICE" = "2" ]; then
  DB_MODE="external"
  ask "DATABASE_URL (postgres://user:password@host:port/db)" "" EXTERNAL_DATABASE_URL
  [ -n "$EXTERNAL_DATABASE_URL" ] || fail "A DATABASE_URL is required to use an external Postgres."
fi
ok "Database: $([ "$DB_MODE" = bundled ] && echo 'bundled (Docker)' || echo 'external')"

if [ "$DB_MODE" = "bundled" ]; then
  if is_weak "$DB_PASSWORD"; then
    # hex, not base64: this password goes straight into a postgres:// URL
    # both here (dev mode) and in compose.yaml's DATABASE_URL interpolation
    # — base64's alphabet includes '/', which a URL parser would read as
    # a path separator and split the password on, and '+'/'=', which some
    # parsers treat specially too. Hex has none of that ambiguity.
    DB_PASSWORD="$(openssl rand -hex 24)"
    info "Generating a password for the bundled Postgres."
  else
    ok "Bundled Postgres password already in .env — kept."
  fi
  DATABASE_URL="postgres://keeppix:${DB_PASSWORD}@db/keeppix"
  # Local dev only: the backend runs on the host, not in a container on the
  # same Docker network — 'db' as a hostname wouldn't resolve. The bundled
  # container still publishes 5432 to the host (add it to compose.yaml's
  # `db.ports` if you don't see it — it isn't published by default since
  # Docker-mode Keeppix never needs it from the host).
  [ "$MODE" = "dev" ] && DATABASE_URL="postgres://keeppix:${DB_PASSWORD}@localhost:5432/keeppix"
else
  DATABASE_URL="$EXTERNAL_DATABASE_URL"
fi

if [ "$MODE" = "docker" ]; then
  ask "Path to your photo library on this machine (mounted read-only)" "$PHOTOS_PATH" PHOTOS_PATH
  mkdir -p "$PHOTOS_PATH"
  ok "Photos: $PHOTOS_PATH"

  # The app inside the container runs as its own fixed non-root user, not
  # your host user — a library folder that's only readable by its owner
  # (e.g. mode 700, the macOS default for anything outside your home dir's
  # usual spots) is invisible to it. The scan then just finds zero photos,
  # with nothing in the logs to explain why. `-perm -005` (other: r-x)
  # works identically on BSD find (macOS) and GNU find (Linux).
  readable() { find "$PHOTOS_PATH" -maxdepth 0 -perm -005 2>/dev/null | grep -q .; }
  RUN_AS_ROOT=0
  if ! readable; then
    warn "$PHOTOS_PATH isn't readable by other users on this machine."
    info "Keeppix's own user inside the container needs read+traverse access,"
    info "or your library will show up empty with no error anywhere."
    ask "Grant read-only access for other users now (chmod -R o+rX)? (Y/n)" "y" FIX_PERMS
    if [[ "$FIX_PERMS" =~ ^[yY] ]]; then
      chmod -R o+rX "$PHOTOS_PATH"
      ok "Permissions updated."
    else
      warn "Skipped."
    fi
  fi
  if ! readable; then
    # Some filesystems (exFAT, FAT32 — common on external/USB drives) have
    # no concept of Unix permissions at all: the chmod above just ran and
    # silently changed nothing. The only way in short of reformatting the
    # drive is to have the container read as root instead — the mount is
    # still read-only (compose.yaml), so this only affects who can read the
    # library, not what the app can do to it.
    warn "Still not readable — this is likely a filesystem (exFAT/FAT32) that"
    warn "doesn't support Unix permissions at all, so chmod can't fix it."
    ask "Start the Keeppix container as root instead, so it can read this library? (y/N)" "n" ROOT_ANSWER
    if [[ "$ROOT_ANSWER" =~ ^[yY] ]]; then
      RUN_AS_ROOT=1
      ok "Will start the container as root."
    else
      warn "Continuing as-is — the library will likely show up empty until this is resolved."
    fi
  fi
fi

BAK=""
if [ -f .env ]; then
  BAK=".env.bak.$(date +%s)"
  cp .env "$BAK"
  info "Previous .env backed up: $BAK"
fi

{
  echo "# Generated/updated by scripts/install.sh on $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "DATABASE_URL=${DATABASE_URL}"
  [ "$DB_MODE" = "bundled" ] && echo "DB_PASSWORD=${DB_PASSWORD}"
  [ "$MODE" = "docker" ] && echo "PHOTOS_PATH=${PHOTOS_PATH}"
  # Pre-existing lines not handled above (KEEPPIX_* tuning, Postgres
  # profile overrides, etc.) survive — but only when FRESH_ENV=0 (explicit
  # reuse): "start fresh" has to actually start fresh. DATABASE_URL/
  # DB_PASSWORD/PHOTOS_PATH are always excluded from the carried-over set,
  # in both cases, since this script is the one source of truth for them.
  if [ "$FRESH_ENV" = 0 ] && [ -n "$BAK" ]; then
    grep -vE '^(DATABASE_URL|DB_PASSWORD|PHOTOS_PATH)=' "$BAK" || true
  fi
} > .env.new
mv .env.new .env
chmod 600 .env
ok ".env written (permissions 600)."

# ── 5. Database: start + REAL verification ──────────────────────────────
title "5/5 — Start"

PG_ATTEMPTS=30
if [ "$DB_MODE" = "bundled" ]; then
  DB_UP_LOG="$(mktemp)"
  if ! run_quiet "Starting Postgres..." "$DB_UP_LOG" $DOCKER_COMPOSE_CMD --profile bundled up -d db; then
    echo -e "\n$(tail -40 "$DB_UP_LOG" | sed 's/^/  /')\n"
    rm -f "$DB_UP_LOG"
    fail "Couldn't start the bundled Postgres (log above)."
  fi
  rm -f "$DB_UP_LOG"
  ok "Postgres container started."

  DB_CONTAINER="$($DOCKER_COMPOSE_CMD --profile bundled ps -q db)"
  info "Waiting for Postgres to accept connections..."
  j=0
  until docker exec "$DB_CONTAINER" pg_isready -U keeppix -d keeppix >/dev/null 2>&1; do
    j=$((j + 1))
    [ "$j" -ge "$PG_ATTEMPTS" ] && fail "Postgres didn't respond in time — check '$DOCKER_COMPOSE_CMD --profile bundled logs db'."
    sleep 1
  done
  ok "Postgres accepts connections."
  # No separate authentication pre-check here: Keeppix applies its own
  # migrations at startup (db.migrate() in main.rs) rather than through a
  # dedicated migrate container — the app's own boot IS the real
  # connection/auth test, further down. Simulating it here (docker exec
  # ... psql) would just be a second, less trustworthy path to the same
  # answer.
elif command -v psql >/dev/null 2>&1; then
  info "Verifying the connection to the external Postgres..."
  if psql "$EXTERNAL_DATABASE_URL" -c 'SELECT 1' >/dev/null 2>&1; then
    ok "Authenticated successfully."
  else
    fail "Couldn't connect with the given DATABASE_URL — check it and re-run the script."
  fi
else
  warn "psql isn't available on this machine: can't verify the external Postgres ahead of time."
  warn "Continuing — if the connection is wrong you'll see it fail further down, with the database's own error."
fi

if [ "$MODE" = "dev" ]; then
  if [ ! -d frontend/dist ]; then
    info "frontend/dist doesn't exist yet — the backend won't compile without it (rust-embed bakes it in at build time)."
    info "npm ci (frontend)..."
    npm --prefix frontend ci
    info "npm run build (frontend)..."
    npm --prefix frontend run build
  else
    ok "frontend/dist already present."
  fi

  info "Checking the backend actually starts and migrates (Ctrl+C once you see \"keeppix listening\")..."
  echo

  # cargo run doesn't read .env itself (only docker compose does) — export
  # what we just wrote into this shell before handing off to the two dev
  # processes below.
  set -a
  # shellcheck disable=SC1091
  source .env
  set +a

  cleanup() { jobs -p | xargs -r kill 2>/dev/null || true; }
  trap cleanup EXIT INT TERM

  cargo run -p keeppix-server &
  BACKEND_PID=$!

  info "Waiting for the backend on :5673..."
  j=0
  until curl -fsS http://127.0.0.1:5673/health >/dev/null 2>&1; do
    j=$((j + 1))
    if ! kill -0 "$BACKEND_PID" 2>/dev/null; then
      fail "The backend exited before becoming ready — see the output above (a database auth error is the most likely cause with a bundled Postgres from a previous run; see the Docker-mode reset flow in this script for the equivalent fix, or just delete ./pgdata if you're running the bundled Postgres from a previous .env)."
    fi
    [ "$j" -ge 60 ] && fail "Backend didn't respond on :5673 in time."
    sleep 1
  done
  ok "Backend up on :5673 (migrations applied)."

  echo -e "\n  ${GREEN}${BOLD}✓ Development environment ready${NC}\n"
  echo -e "  Starting the frontend dev server — ${BOLD}Ctrl+C stops both.${NC}"
  echo -e "  ${DIM}Frontend: http://localhost:5173  (proxies /api, /media, /health to :5673)${NC}\n"

  npm --prefix frontend run dev
  exit 0
fi

# Docker mode from here on.
COMPOSE_PROFILE_FLAG=""
[ "$DB_MODE" = "bundled" ] && COMPOSE_PROFILE_FLAG="--profile bundled "
# Merges in compose.root.yaml only when the permission check above couldn't
# make the library readable any other way (see the comment on that file).
COMPOSE_FILE_FLAG=""
[ "${RUN_AS_ROOT:-0}" = 1 ] && COMPOSE_FILE_FLAG="-f compose.yaml -f compose.root.yaml "
BUILD_LOG="$(mktemp)"
if ! run_quiet "Building the image..." "$BUILD_LOG" $DOCKER_COMPOSE_CMD $COMPOSE_FILE_FLAG$COMPOSE_PROFILE_FLAG up -d --build; then
  echo -e "\n$(tail -40 "$BUILD_LOG" | sed 's/^/  /')\n"
  rm -f "$BUILD_LOG"
  fail "The build failed (log above)."
fi
rm -f "$BUILD_LOG"

SPIN="$SPIN_FRAMES"
ATTEMPTS=60
i=0
READY=0
KEEPPIX_CONTAINER="$($DOCKER_COMPOSE_CMD $COMPOSE_FILE_FLAG ps -q keeppix)"
while [ $i -lt $ATTEMPTS ]; do
  if curl -fsS "http://localhost:5673/health" >/dev/null 2>&1; then
    READY=1
    break
  fi
  STATE="$(docker inspect -f '{{.State.Status}}' "$KEEPPIX_CONTAINER" 2>/dev/null || echo unknown)"
  [ "$STATE" = "exited" ] && break
  printf "\r  %s Starting...  " "${SPIN:i%${#SPIN}:1}"
  i=$((i + 1))
  sleep 1.5
done
printf "\r"

if [ "$READY" = 1 ]; then
  echo -e "\n  ${GREEN}${BOLD}✓ Keeppix is ready${NC}\n"
  echo -e "  ${BOLD}→ http://localhost:5673${NC}\n"
  echo -e "  ${DIM}First run: create the admin account from the page that opens.${NC}"
  echo -e "  ${DIM}Logs:  ${DOCKER_COMPOSE_CMD} ${COMPOSE_FILE_FLAG}${COMPOSE_PROFILE_FLAG}logs -f keeppix${NC}"
  echo -e "  ${DIM}Stop:  ${DOCKER_COMPOSE_CMD} ${COMPOSE_FILE_FLAG}${COMPOSE_PROFILE_FLAG}down${NC}\n"
  exit 0
fi

echo -e "\n  ${RED}${BOLD}✗ Keeppix isn't responding yet on :5673${NC}\n"
echo -e "  Last lines of the container log:\n"
$DOCKER_COMPOSE_CMD $COMPOSE_FILE_FLAG$COMPOSE_PROFILE_FLAG logs --tail=40 keeppix 2>&1 | sed 's/^/  /'
echo

if [ "$DB_MODE" = "bundled" ] && $DOCKER_COMPOSE_CMD $COMPOSE_FILE_FLAG$COMPOSE_PROFILE_FLAG logs keeppix 2>&1 | grep -qi "password authentication failed"; then
  # Keeppix applies its own migrations at startup — a failed connection
  # crashes the container immediately, so this log line is the whole
  # authentication test, not a guess. Postgres never re-applies
  # credentials to an already-initialized data directory: the bundled
  # database is a bind mount (./pgdata), not a named Docker volume, so
  # "reset" here means clearing that directory on the host, not
  # `docker volume rm`.
  warn "Most likely cause: ./pgdata already exists from a previous run, initialized with a different password."
  ask "Reset the bundled Postgres and recreate it with the current credentials? This DELETES any data already in ./pgdata. (y/N)" "n" RESET_DB
  if [[ "$RESET_DB" =~ ^[yY] ]]; then
    $DOCKER_COMPOSE_CMD $COMPOSE_FILE_FLAG$COMPOSE_PROFILE_FLAG down >/dev/null 2>&1
    rm -rf ./pgdata
    RESET_LOG="$(mktemp)"
    if ! run_quiet "Recreating Postgres and starting Keeppix..." "$RESET_LOG" $DOCKER_COMPOSE_CMD $COMPOSE_FILE_FLAG$COMPOSE_PROFILE_FLAG up -d; then
      echo -e "\n$(tail -40 "$RESET_LOG" | sed 's/^/  /')\n"
      rm -f "$RESET_LOG"
      fail "Couldn't recreate the bundled Postgres (log above)."
    fi
    rm -f "$RESET_LOG"
    i=0; READY=0
    KEEPPIX_CONTAINER="$($DOCKER_COMPOSE_CMD $COMPOSE_FILE_FLAG ps -q keeppix)"
    while [ $i -lt $ATTEMPTS ]; do
      curl -fsS "http://localhost:5673/health" >/dev/null 2>&1 && { READY=1; break; }
      [ "$(docker inspect -f '{{.State.Status}}' "$KEEPPIX_CONTAINER" 2>/dev/null || echo unknown)" = "exited" ] && break
      printf "\r  %s Starting...  " "${SPIN:i%${#SPIN}:1}"
      i=$((i + 1))
      sleep 1.5
    done
    printf "\r"
    if [ "$READY" = 1 ]; then
      echo -e "\n  ${GREEN}${BOLD}✓ Postgres recreated, Keeppix is ready${NC}\n"
      echo -e "  ${BOLD}→ http://localhost:5673${NC}\n"
      exit 0
    fi
    echo -e "\n  ${RED}${BOLD}✗ Still not responding after the reset${NC}\n"
    $DOCKER_COMPOSE_CMD $COMPOSE_FILE_FLAG$COMPOSE_PROFILE_FLAG logs --tail=40 keeppix 2>&1 | sed 's/^/  /'
    fail "Something doesn't add up — please open an issue with the full output above."
  else
    fail "Update DB_PASSWORD in .env to match ./pgdata's original password, then re-run this script."
  fi
fi

fail "Keeppix didn't become ready (log above)."
