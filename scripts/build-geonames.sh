#!/usr/bin/env bash
set -euo pipefail

output=${1:-/usr/share/keeppix/places.csv}
workdir=$(mktemp -d)
trap 'rm -rf "$workdir"' EXIT

base_url=https://download.geonames.org/export/dump

download() {
  curl --fail --location --retry 3 --proto '=https' --tlsv1.2 \
    "$base_url/$1" --output "$workdir/$1"
}

download cities500.zip
download admin1CodesASCII.txt
download admin2Codes.txt
download countryInfo.txt
unzip -q "$workdir/cities500.zip" -d "$workdir"

mkdir -p "$(dirname "$output")"
normalized="$workdir/places.csv"

LC_ALL=C.UTF-8 gawk -F '\t' -v OFS='\t' '
  ARGIND == 1 {
    sub(/\r$/, "")
    admin1[$1] = $2
    next
  }
  ARGIND == 2 {
    sub(/\r$/, "")
    admin2[$1] = $2
    next
  }
  ARGIND == 3 {
    sub(/\r$/, "")
    if ($0 !~ /^#/ && $1 != "") {
      countries[toupper($1)] = 1
    }
    next
  }
  ARGIND == 4 {
    sub(/\r$/, "")
    country = toupper($9)
    if (!(country in countries)) {
      next
    }
    population = ($15 == "" ? 0 : $15)
    print $1, $2, $3, country,
          admin1[country "." $11],
          admin2[country "." $11 "." $12],
          $5, $6, population
  }
' "$workdir/admin1CodesASCII.txt" \
  "$workdir/admin2Codes.txt" \
  "$workdir/countryInfo.txt" \
  "$workdir/cities500.txt" > "$normalized"

rows=$(wc -l < "$normalized")
if [ "$rows" -lt 100000 ]; then
  echo "GeoNames normalization produced only $rows rows" >&2
  exit 1
fi

mv "$normalized" "$output"
