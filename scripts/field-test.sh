#!/usr/bin/env bash
# Prova sul campo: indicizza un archivio reale e misura ogni fase.
#
# Gli originali sono montati READ-ONLY (`:ro` nel compose). Lo script non
# scrive mai nella cartella sorgente: se lo facesse, sarebbe un difetto.
#
#   PHOTOS_PATH="/percorso/archivio" ./scripts/field-test.sh
#
# Produce una tabella con: durata di ogni fase, throughput, copertura delle
# preview RAW, spazio dei derivati. I numeri finiscono anche in
# .superpowers/field-test-<data>.md
set -uo pipefail

ARCHIVE="${PHOTOS_PATH:?serve PHOTOS_PATH}"
DB_PASSWORD="${DB_PASSWORD:-fieldtest}"
BASE="http://127.0.0.1:5673"
REPORT=".superpowers/field-test-$(date +%Y%m%d-%H%M).md"
export DB_PASSWORD

psql() { docker compose exec -T db psql -U keeppix -d keeppix -tAc "$1" 2>/dev/null | tr -d ' '; }
now()  { date +%s; }
hms()  { printf '%dm%02ds' $(($1/60)) $(($1%60)); }

say() { printf '%s\n' "$*" | tee -a "$REPORT"; }

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
# Impronta per verificare a fine corsa che nulla sia stato modificato.
SRC_FINGERPRINT=$(find "$ARCHIVE" -type f -exec stat -f '%N %z %m' {} \; 2>/dev/null | sort | shasum | cut -d' ' -f1)

# ── 1. Stack pulito ───────────────────────────────────────────────────────
echo "→ ricostruzione immagine e avvio stack…"
docker compose --profile bundled down -v >/dev/null 2>&1
rm -rf ./data ./pgdata
(cd frontend && npm ci --silent && npm run build >/dev/null) || exit 1
docker compose --profile bundled build >/dev/null 2>&1 || exit 1
PHOTOS_PATH="$ARCHIVE" docker compose --profile bundled up -d >/dev/null 2>&1 || exit 1

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

# ── 2. Admin + libreria ───────────────────────────────────────────────────
curl -sf -X POST "$BASE/api/v1/setup" \
    -H 'content-type: application/json' -H 'x-keeppix-client: field-test' \
    -d '{"username":"tester","display_name":"Tester","password":"correct horse battery staple"}' \
    >/dev/null || { echo "setup fallito"; exit 1; }

OWNER=$(psql "SELECT id FROM users LIMIT 1")
LIB=$(psql "INSERT INTO libraries (id, name, owner_id, root_path)
            VALUES (gen_random_uuid(), 'Campo', '$OWNER', '/photos') RETURNING id" | head -1)
say "Libreria \`$LIB\` creata via SQL — **non esiste un endpoint per crearla**."
say

# Il watcher legge le librerie al boot: riavvio perché la veda.
docker compose --profile bundled restart keeppix >/dev/null 2>&1
until curl -sf "$BASE/health" >/dev/null 2>&1; do sleep 2; done

psql "INSERT INTO jobs (kind, payload, priority, dedup_key)
      VALUES ('discover_library', json_build_object('library_id','$LIB')::jsonb, 3, 'discover:$LIB')
      ON CONFLICT DO NOTHING" >/dev/null

# ── 3. Misura, fase per fase ──────────────────────────────────────────────
START=$(now)
P_DISCOVERY=""; P_EXIF=""; P_HASH=""; P_TOTAL=""
LAST_LINE=""

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
    [[ $EL -gt 7200 ]] && { echo; echo "timeout a 2h"; P_TOTAL=$EL; break; }
    sleep 5
done
echo

# ── 4. Risultati ──────────────────────────────────────────────────────────
TOT=$(psql "SELECT count(*) FROM assets")
IDX=$(psql "SELECT count(*) FROM assets WHERE status='indexed'")
ERR=$(psql "SELECT count(*) FROM assets WHERE status='error'")
FOLDERS=$(psql "SELECT count(*) FROM folders")
THUMBHASH=$(psql "SELECT count(*) FROM assets WHERE thumbhash IS NOT NULL")
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
say "| Derivati su disco | $(numfmt --to=iec "${DERIV_SIZE:-0}" 2>/dev/null || echo "${DERIV_SIZE:-0}B") |"
say "| Rapporto derivati/originali | $(awk -v d="${DERIV_SIZE:-0}" -v s="$SRC_BYTES" 'BEGIN{printf "%.1f%%", d*100/s}') |"
[[ -n "$P_HASH" && "$P_HASH" -gt 0 ]] && say "| Velocità di hash | $(( MB / P_HASH )) MB/s |"
say

if [[ "$ERR" != "0" ]]; then
    say "### File in errore"
    say '```'
    psql "SELECT filename||' — '||coalesce(error_detail,'?') FROM assets WHERE status='error' LIMIT 15" | tee -a "$REPORT"
    say '```'
fi

# ── 5. L'archivio è intatto? ──────────────────────────────────────────────
AFTER=$(find "$ARCHIVE" -type f -exec stat -f '%N %z %m' {} \; 2>/dev/null | sort | shasum | cut -d' ' -f1)
if [[ "$SRC_FINGERPRINT" == "$AFTER" ]]; then
    say "✅ **Archivio intatto**: nessun file creato, modificato o rimosso."
else
    say "❌ **L'ARCHIVIO È CAMBIATO.** Difetto grave: gli originali devono essere immutabili."
fi
say
say "Stack ancora in esecuzione su $BASE — \`docker compose --profile bundled down -v\` per fermarlo."
echo "Report: $REPORT"
