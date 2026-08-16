#!/usr/bin/env bash
# Prova sul campo: indicizza un archivio reale via HTTP e misura ogni fase.
#
# Gli originali sono montati READ-ONLY (`:ro` nel compose). Lo script non
# scrive mai nella cartella sorgente: se lo facesse, sarebbe un difetto.
#
#   PHOTOS_PATH="/percorso/archivio" ./scripts/field-test.sh
#
# Dopo Fase 2R: setup, libreria e scansione passano dagli endpoint (nessun
# INSERT SQL, nessun restart del container). Esce ≠ 0 se i budget di
# discovery sono sforati (vedi Task 8).
#
# Produce una tabella con: durata di ogni fase, throughput, copertura delle
# preview RAW, spazio dei derivati. I numeri finiscono anche in
# .superpowers/field-test-<data>.md
set -euo pipefail

ARCHIVE="${PHOTOS_PATH:?serve PHOTOS_PATH}"
DB_PASSWORD="${DB_PASSWORD:-fieldtest}"
BASE="http://127.0.0.1:5673"
REPORT=".superpowers/field-test-$(date +%Y%m%d-%H%M).md"
COOKIE_JAR="$(mktemp)"
# Budget discovery (Task 8): 1.000 file < 30 s → ~30 ms/file di tetto
# lineare. Sull'archivio reale si confronta la discovery osservata.
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
    # Linux: `%n %s %Y`. macOS aveva `stat -f`; questo ambiente è Linux.
    find "$1" -type f -exec stat -c '%n %s %Y' {} \; 2>/dev/null | sort | sha256sum | awk '{print $1}'
}

cleanup() {
    rm -f "$COOKIE_JAR"
}
trap cleanup EXIT

# ── 0. Stato di partenza ──────────────────────────────────────────────────
mkdir -p .superpowers
: > "$REPORT"
say "# Prova sul campo — $(date '+%Y-%m-%d %H:%M')"
say
say "Archivio: \`$ARCHIVE\`"
SRC_FILES=$(find "$ARCHIVE" -type f \( -iname '*.arw' -o -iname '*.nef' -o -iname '*.cr2' \
    -o -iname '*.cr3' -o -iname '*.dng' -o -iname '*.orf' -o -iname '*.raf' \
    -o -iname '*.jpg' -o -iname '*.jpeg' -o -iname '*.heic' -o -iname '*.png' \) | wc -l | tr -d ' ')
SRC_BYTES=$(du -sk "$ARCHIVE" | awk '{print $1*1024}')
say "File indicizzabili: **$SRC_FILES** · $(numfmt --to=iec "$SRC_BYTES" 2>/dev/null || echo "${SRC_BYTES}B")"
say
SRC_FINGERPRINT=$(fingerprint "$ARCHIVE")

# ── 1. Stack pulito ───────────────────────────────────────────────────────
echo "→ ricostruzione immagine e avvio stack…"
docker compose --profile bundled down -v >/dev/null 2>&1 || true
rm -rf ./data ./pgdata
(cd frontend && npm ci --silent && npm run build >/dev/null) || exit 1
if ! docker compose --profile bundled build >/dev/null 2>&1; then
    echo "docker compose build fallito (daemon Docker disponibile?)"
    exit 1
fi
if ! PHOTOS_PATH="$ARCHIVE" docker compose --profile bundled up -d >/dev/null 2>&1; then
    echo "docker compose up fallito"
    exit 1
fi

until curl -sf "$BASE/health" >/dev/null 2>&1; do sleep 2; done
echo "→ stack pronto"

# Sicurezza: il mount degli originali deve essere read-only.
# L'immagine è distroless: niente shell. Si interroga Docker, che è la fonte
# di verità sul montaggio, invece di provare a scrivere da dentro.
RO=$(docker inspect "$(docker compose ps -q keeppix)" \
      --format '{{range .Mounts}}{{if eq .Destination "/photos"}}{{.RW}}{{end}}{{end}}')
if [[ "$RO" != "false" ]]; then
    say "> ⚠️ **ATTENZIONE: /photos non è read-only** (RW=$RO). Interrotto per non rischiare l'archivio."
    docker compose --profile bundled down -v >/dev/null 2>&1
    exit 1
fi
say "Mount degli originali verificato **read-only** (Docker riporta RW=false)."
say

# ── 2. Admin + libreria + scansione via HTTP ──────────────────────────────
api POST /api/v1/setup \
    -d '{"username":"tester","display_name":"Tester","password":"correct horse battery staple"}' \
    >/dev/null || { echo "setup fallito"; exit 1; }

LIB_JSON=$(api POST /api/v1/libraries \
    -d '{"name":"Campo","root_path":"/photos"}') \
    || { echo "POST /libraries fallito (allowlist KEEPPIX_LIBRARY_ROOTS?)"; exit 1; }
LIB=$(printf '%s' "$LIB_JSON" | sed -n 's/.*"id"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -1)
if [[ -z "$LIB" ]]; then
    echo "impossibile leggere library id: $LIB_JSON"
    exit 1
fi
say "Libreria \`$LIB\` creata via **POST /api/v1/libraries** (nessun SQL)."
say

api POST "/api/v1/libraries/${LIB}/scan" -d '{}' >/dev/null \
    || { echo "POST /scan fallito"; exit 1; }
say "Scansione avviata via **POST /api/v1/libraries/${LIB}/scan** (nessun restart)."
say

# ── 3. Misura, fase per fase ──────────────────────────────────────────────
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

    LINE="  $(hms $EL)  trovati:$TOT  exif:$EXIF  hash:$HASH  indicizzati:$IDX  errori:$ERR  coda:$PEND"
    [[ "$LINE" != "$LAST_LINE" ]] && { printf '\r%-90s' "$LINE"; LAST_LINE="$LINE"; }

    [[ "$PEND" == "0" && "$TOT" -gt 0 ]] && { P_TOTAL=$EL; break; }
    [[ $EL -gt 7200 ]] && { echo; echo "timeout a 2h"; P_TOTAL=$EL; EXIT_CODE=2; break; }
    sleep 5
done
echo

# ── 4. Risultati ──────────────────────────────────────────────────────────
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

say "## Risultati"
say
say "| Fase | Durata | Throughput |"
say "|---|---|---|"
for pair in "discovery:$P_DISCOVERY" "exif:$P_EXIF" "hash:$P_HASH" "totale:$P_TOTAL"; do
    p="${pair%%:*}"; v="${pair#*:}"
    [[ -z "$v" ]] && continue
    if [[ "$v" -gt 0 ]]; then rate="$(( TOT / v )) file/s"; else rate="istantanea"; fi
    say "| $p | $(hms "$v") | $rate |"
done
say
say "| Metrica | Valore |"
say "|---|---|"
say "| File sorgente | $SRC_FILES |"
say "| Asset creati | $TOT |"
say "| Cartelle | $FOLDERS |"
say "| Indicizzati | $IDX |"
say "| Errori | $ERR |"
say "| Con thumbhash | $THUMBHASH |"
say "| RAW con preview | $RAW_PREVIEW / $RAW_TOT |"
say "| Derivati su disco | $(numfmt --to=iec "${DERIV_SIZE:-0}" 2>/dev/null || echo "${DERIV_SIZE:-0}B") |"
say "| Rapporto derivati/originali | $(awk -v d="${DERIV_SIZE:-0}" -v s="$SRC_BYTES" 'BEGIN{ if(s<=0){print "n/a"}else{printf "%.1f%%", d*100/s}}') |"
[[ -n "$P_HASH" && "$P_HASH" -gt 0 && "$MB" -gt 0 ]] && say "| Velocità di hash | $(( MB / P_HASH )) MB/s |"
say

# Budget discovery vs Task 8
if [[ -n "$P_DISCOVERY" && "$SRC_FILES" -gt 0 ]]; then
    BUDGET_S=$(( (SRC_FILES * BUDGET_DISCOVERY_MS_PER_FILE) / 1000 ))
    [[ "$BUDGET_S" -lt 30 ]] && BUDGET_S=30
    say "## Budget (Task 8)"
    say
    say "| Check | Osservato | Budget | Esito |"
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
    say "### File in errore"
    say '```'
    psql "SELECT filename||' — '||coalesce(error_detail,'?') FROM assets WHERE status='error' LIMIT 15" | tee -a "$REPORT"
    say '```'
fi

# ── 5. L'archivio è intatto? ──────────────────────────────────────────────
AFTER=$(fingerprint "$ARCHIVE")
if [[ "$SRC_FINGERPRINT" == "$AFTER" ]]; then
    say "✅ **Archivio intatto**: nessun file creato, modificato o rimosso."
else
    say "❌ **L'ARCHIVIO È CAMBIATO.** Difetto grave: gli originali devono essere immutabili."
    EXIT_CODE=4
fi
say
say "Stack ancora in esecuzione su $BASE — \`docker compose --profile bundled down -v\` per fermarlo."
echo "Report: $REPORT"
exit "$EXIT_CODE"
