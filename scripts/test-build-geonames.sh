#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

fixture="$tmp/fixture"
mkdir -p "$fixture" "$tmp/bin"

cat > "$tmp/bin/curl" <<'EOF'
#!/usr/bin/env bash
echo "network access is forbidden in the GeoNames normalizer test" >&2
exit 99
EOF
chmod +x "$tmp/bin/curl"

printf '%s\n' \
  $'IT.04\tEmilia-Romagna\tEmilia-Romagna\t3177401' \
  > "$fixture/admin1CodesASCII.txt"
: > "$fixture/admin2Codes.txt"
printf '%s\n' \
  $'IT\tITA\t380\tIT\tItaly\tRome\t301340\t62137802\tEU\t.it\tEUR\tEuro\t39\t#####\t^(\\d{5})$\tit-IT,de-IT,fr-IT\t3175395\tAT,CH,SM,SI,VA,FR' \
  > "$fixture/countryInfo.txt"
printf '%s\n' \
  $'1001\tModena\tModena\t\t44\t10\tP\tPPL\tIT\t\t04\t001\t\t\t1000\t\t100\tEurope/Rome\t2026-01-01' \
  $'1002\tRimini\tRimini\t\t46\t12\tP\tPPL\tIT\t\t04\t001\t\t\t2000\t\t100\tEurope/Rome\t2026-01-01' \
  > "$fixture/cities500.txt"

output="$tmp/places.csv"
PATH="$tmp/bin:$PATH" "$root/scripts/build-geonames.sh" "$output" "$fixture"

test "$(wc -l < "$output")" -eq 4
awk -F '\t' '
  $1 == 3177401 {
    found_region = $2 == "Emilia-Romagna" && $3 == "Emilia-Romagna" && $4 == "IT" && $5 == "Emilia-Romagna" && $6 == "" && $7 == 45 && $8 == 11 && $9 == 0
  }
  $1 == 3175395 {
    found_country = $2 == "Italy" && $3 == "Italy" && $4 == "IT" && $5 == "" && $6 == "" && $7 == 45 && $8 == 11 && $9 == 0
  }
  END {
    exit !(found_region && found_country)
  }
' "$output"

echo "GeoNames normalizer fixture test: ok (2 cities, 1 region, 1 country)"
