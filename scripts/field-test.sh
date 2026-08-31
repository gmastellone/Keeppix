#!/usr/bin/env bash
# Field test: indexes a real archive via HTTP and measures every phase.
#
# Originals are mounted READ-ONLY (`:ro` in the compose file). The script
# never writes to the source folder: if it did, that would be a defect.
#
#   PHOTOS_PATH="/path/to/archive" ./scripts/field-test.sh
#
# Setup, library creation, and scanning all go through the HTTP endpoints
# (no SQL INSERT, no container restart). Exits non-zero if the discovery
# budget is exceeded.
#
# Produces a table with: duration of each phase, throughput, RAW preview
# coverage, and derivative disk usage. The numbers also land in
# .superpowers/field-test-<date>.md
set -euo pipefail

ARCHIVE="${PHOTOS_PATH:?PHOTOS_PATH is required}"
# Must match whatever DB_PASSWORD the running bundled `db` service was
# started with — no hardcoded fallback, since a plausible-looking guess
# would silently work against a database seeded with the same weak default.
DB_PASSWORD="${DB_PASSWORD:?DB_PASSWORD is required — export the same value used to start the bundled db service}"
BASE="http://127.0.0.1:5673"
REPORT=".superpowers/field-test-$(date +%Y%m%d-%H%M).md"
COOKIE_JAR="$(mktemp)"
# Discovery budget: 1,000 files < 30s -> ~30ms/file linear ceiling.
# The observed discovery time on the real archive is compared against it.
BUDGET_DISCOVERY_MS_PER_FILE="${BUDGET_DISCOVERY_MS_PER_FILE:-30}"
export DB_PASSWORD

psql() { docker compose exec -T db psql -U keeppix -d keeppix -tAc "$1" 2>/dev/null | tr -d ' '; }
now()  { date +%s; }
hms()  { printf '%dm%02ds' $(($1/60)) $(($1%60)); }

say() { printf '%s\n' "$*" | tee -a "$REPORT"; }

api() {
    local method="$1" path="$2"
    shift 2
    curl -sf -X "$method" "$BASE$path" \
        -H 'content-type: application/json' \
        -H 'x-keeppix-client: field-test' \
        -b "$COOKIE_JAR" -c "$COOKIE_JAR" \
        "$@"
}

fingerprint() {
    # Linux: `%n %s %Y`. macOS would use `stat -f`; this environment is Linux.
    find "$1" -type f -exec stat -c '%n %s %Y' {} \; 2>/dev/null | sort | sha256sum | awk '{print $1}'
}

cleanup() {
    rm -f "$COOKIE_JAR"
}
trap cleanup EXIT

# ── 0. Starting state ──────────────────────────────────────────────────
mkdir -p .superpowers
: > "$REPORT"
say "# Field test — $(date '+%Y-%m-%d %H:%M')"
say
say "Archive: \`$ARCHIVE\`"
SRC_FILES=$(find "$ARCHIVE" -type f \( -iname '*.arw' -o -iname '*.nef' -o -iname '*.cr2' \
    -o -iname '*.cr3' -o -iname '*.dng' -o -iname '*.orf' -o -iname '*.raf' \
    -o -iname '*.jpg' -o -iname '*.jpeg' -o -iname '*.heic' -o -iname '*.png' \) | wc -l | tr -d ' ')
SRC_BYTES=$(du -sk "$ARCHIVE" | awk '{print $1*1024}')
say "Indexable files: **$SRC_FILES** · $(numfmt --to=iec "$SRC_BYTES" 2>/dev/null || echo "${SRC_BYTES}B")"
say
SRC_FINGERPRINT=$(fingerprint "$ARCHIVE")

# ── 1. Clean stack ───────────────────────────────────────────────────────
echo "→ rebuilding image and starting stack…"
docker compose --profile bundled down -v >/dev/null 2>&1 || true
rm -rf ./data ./pgdata
(cd frontend && npm ci --silent && npm run build >/dev/null) || exit 1
if ! docker compose --profile bundled build >/dev/null 2>&1; then
    echo "docker compose build failed (is the Docker daemon available?)"
    exit 1
fi
if ! docker compose --profile bundled up -d db >/dev/null 2>&1; then
    echo "docker compose up (db) failed"
    exit 1
fi

# `rm -rf ./pgdata` above recreates the host directory right before Docker
# Desktop mounts over it: on this platform the bind mount is sometimes not
# ready at the exact moment postgres' entrypoint runs `initdb`, which then
# fails with "wrong ownership" on its first internal startup. The image
# still reports "healthy" (the socket accepts connections), but the init
# script skips creating the application database, believing the cluster
# was already initialized. Effect: `keeppix` starts, can't find the
# database, crash-loops, and the `/health` polling two lines below hangs
# forever — at a glance it looks like a stuck stack, not a missing
# database.
#
# Verified and reproduced three times in a row on this machine (amd64
# build emulated on Apple Silicon, so a wider race window than usual).
# Rather than hope it doesn't recur, the check heals itself: if the
# database is missing after `db` reports healthy, it creates it.
until docker compose exec -T db pg_isready -U keeppix >/dev/null 2>&1; do sleep 1; done
if ! docker compose exec -T db psql -U keeppix -d keeppix -c 'SELECT 1' >/dev/null 2>&1; then
    echo "→ application database missing after init (bind mount race): creating it"
    docker compose exec -T -u postgres db \
        psql -U keeppix -d template1 -c 'CREATE DATABASE keeppix OWNER keeppix' >/dev/null
fi

if ! PHOTOS_PATH="$ARCHIVE" docker compose --profile bundled up -d >/dev/null 2>&1; then
    echo "docker compose up failed"
    exit 1
fi

until curl -sf "$BASE/health" >/dev/null 2>&1; do sleep 2; done
echo "→ stack ready"

# Safety: the originals mount must be read-only.
# The image is distroless: no shell. We ask Docker, which is the source of
# truth for the mount, instead of trying to write from inside the container.
RO=$(docker inspect "$(docker compose ps -q keeppix)" \
      --format '{{range .Mounts}}{{if eq .Destination "/photos"}}{{.RW}}{{end}}{{end}}')
if [[ "$RO" != "false" ]]; then
    say "> ⚠️ **WARNING: /photos is not read-only** (RW=$RO). Aborted to avoid risking the archive."
    docker compose --profile bundled down -v >/dev/null 2>&1
    exit 1
fi
say "Originals mount verified **read-only** (Docker reports RW=false)."
say

# ── 2. Admin + library + scan via HTTP ──────────────────────────────
api POST /api/v1/setup \
    -d '{"username":"tester","display_name":"Tester","password":"correct horse battery staple"}' \
    >/dev/null || { echo "setup failed"; exit 1; }

LIB_JSON=$(api POST /api/v1/libraries \
    -d '{"name":"Campo","root_path":"/photos"}') \
    || { echo "POST /libraries failed (allowlist KEEPPIX_LIBRARY_ROOTS?)"; exit 1; }
LIB=$(printf '%s' "$LIB_JSON" | sed -n 's/.*"id"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -1)
if [[ -z "$LIB" ]]; then
    echo "unable to read library id: $LIB_JSON"
    exit 1
fi
say "Library \`$LIB\` created via **POST /api/v1/libraries** (no SQL)."
say

api POST "/api/v1/libraries/${LIB}/scan" -d '{}' >/dev/null \
    || { echo "POST /scan failed"; exit 1; }
say "Scan started via **POST /api/v1/libraries/${LIB}/scan** (no restart)."
say

# ── 3. Measure, phase by phase ──────────────────────────────────────────────
START=$(now)
P_DISCOVERY=""; P_EXIF=""; P_HASH=""; P_TOTAL=""
LAST_LINE=""
EXIT_CODE=0

while :; do
    DISC=$(psql "SELECT count(*) FROM assets WHERE status='discovered'")
    IDX=$(psql  "SELECT count(*) FROM assets WHERE status='indexed'")
    ERR=$(psql  "SELECT count(*) FROM assets WHERE status='error'")
    EXIF=$(psql "SELECT count(*) FROM asset_exif")
    HASH=$(psql "SELECT count(*) FROM assets WHERE content_hash IS NOT NULL")
    TOT=$(psql  "SELECT count(*) FROM assets")
    PEND=$(psql "SELECT count(*) FROM jobs WHERE status IN ('pending','running')")
    EL=$(( $(now) - START ))

    [[ -z "$P_DISCOVERY" && "$TOT" -ge "$SRC_FILES" ]] && P_DISCOVERY=$EL
    [[ -z "$P_EXIF" && "$EXIF" -ge "$TOT" && "$TOT" -gt 0 ]] && P_EXIF=$EL
    [[ -z "$P_HASH" && "$HASH" -ge "$TOT" && "$TOT" -gt 0 ]] && P_HASH=$EL

    LINE="  $(hms $EL)  found:$TOT  exif:$EXIF  hash:$HASH  indexed:$IDX  errors:$ERR  queue:$PEND"
    [[ "$LINE" != "$LAST_LINE" ]] && { printf '\r%-90s' "$LINE"; LAST_LINE="$LINE"; }

    [[ "$PEND" == "0" && "$TOT" -gt 0 ]] && { P_TOTAL=$EL; break; }
    [[ $EL -gt 7200 ]] && { echo; echo "timeout at 2h"; P_TOTAL=$EL; EXIT_CODE=2; break; }
    sleep 5
done
echo

# ── 4. Results ──────────────────────────────────────────────────────────
TOT=$(psql "SELECT count(*) FROM assets")
IDX=$(psql "SELECT count(*) FROM assets WHERE status='indexed'")
ERR=$(psql  "SELECT count(*) FROM assets WHERE status='error'")
FOLDERS=$(psql "SELECT count(*) FROM folders")
THUMBHASH=$(psql "SELECT count(*) FROM assets WHERE thumbhash IS NOT NULL")
RAW_PREVIEW=$(psql "SELECT count(*) FROM assets a JOIN asset_exif e ON e.asset_id = a.id
    WHERE a.kind = 'raw_image' AND a.thumbhash IS NOT NULL" 2>/dev/null || echo 0)
RAW_TOT=$(psql "SELECT count(*) FROM assets WHERE kind = 'raw_image'")
DERIV_SIZE=$(du -sk ./data/derivatives 2>/dev/null | awk '{print $1*1024}')
MB=$(( SRC_BYTES / 1048576 ))

say "## Results"
say
say "| Phase | Duration | Throughput |"
say "|---|---|---|"
for pair in "discovery:$P_DISCOVERY" "exif:$P_EXIF" "hash:$P_HASH" "total:$P_TOTAL"; do
    p="${pair%%:*}"; v="${pair#*:}"
    [[ -z "$v" ]] && continue
    if [[ "$v" -gt 0 ]]; then rate="$(( TOT / v )) file/s"; else rate="instant"; fi
    say "| $p | $(hms "$v") | $rate |"
done
say
say "| Metric | Value |"
say "|---|---|"
say "| Source files | $SRC_FILES |"
say "| Assets created | $TOT |"
say "| Folders | $FOLDERS |"
say "| Indexed | $IDX |"
say "| Errors | $ERR |"
say "| With thumbhash | $THUMBHASH |"
say "| RAW with preview | $RAW_PREVIEW / $RAW_TOT |"
say "| Derivatives on disk | $(numfmt --to=iec "${DERIV_SIZE:-0}" 2>/dev/null || echo "${DERIV_SIZE:-0}B") |"
say "| Derivatives/originals ratio | $(awk -v d="${DERIV_SIZE:-0}" -v s="$SRC_BYTES" 'BEGIN{ if(s<=0){print "n/a"}else{printf "%.1f%%", d*100/s}}') |"
[[ -n "$P_HASH" && "$P_HASH" -gt 0 && "$MB" -gt 0 ]] && say "| Hash speed | $(( MB / P_HASH )) MB/s |"
say

# Discovery budget check
if [[ -n "$P_DISCOVERY" && "$SRC_FILES" -gt 0 ]]; then
    BUDGET_S=$(( (SRC_FILES * BUDGET_DISCOVERY_MS_PER_FILE) / 1000 ))
    [[ "$BUDGET_S" -lt 30 ]] && BUDGET_S=30
    say "## Budget"
    say
    say "| Check | Observed | Budget | Result |"
    say "|---|---|---|---|"
    if [[ "$P_DISCOVERY" -le "$BUDGET_S" ]]; then
        say "| discovery | $(hms "$P_DISCOVERY") | ≤ $(hms "$BUDGET_S") | ✅ |"
    else
        say "| discovery | $(hms "$P_DISCOVERY") | ≤ $(hms "$BUDGET_S") | ❌ |"
        EXIT_CODE=3
    fi
    say
fi

if [[ "$ERR" != "0" ]]; then
    say "### Files with errors"
    say '```'
    psql "SELECT filename||' — '||coalesce(error_detail,'?') FROM assets WHERE status='error' LIMIT 15" | tee -a "$REPORT"
    say '```'
fi

# ── 5. Is the archive intact? ──────────────────────────────────────────────
AFTER=$(fingerprint "$ARCHIVE")
if [[ "$SRC_FINGERPRINT" == "$AFTER" ]]; then
    say "✅ **Archive intact**: no files created, modified, or removed."
else
    say "❌ **THE ARCHIVE HAS CHANGED.** Serious defect: originals must be immutable."
    EXIT_CODE=4
fi
say
say "Stack still running at $BASE — \`docker compose --profile bundled down -v\` to stop it."
echo "Report: $REPORT"
exit "$EXIT_CODE"
