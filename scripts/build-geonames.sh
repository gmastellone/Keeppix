#!/usr/bin/env bash
set -euo pipefail

output=${1:-/usr/share/keeppix/places.csv}
source_dir=${2:-}
workdir=$(mktemp -d)
trap 'rm -rf "$workdir"' EXIT

base_url=https://download.geonames.org/export/dump

download() {
  curl --fail --location --retry 3 --proto '=https' --tlsv1.2 \
    "$base_url/$1" --output "$workdir/$1"
}

if [[ -n "$source_dir" ]]; then
  input_dir=$source_dir
else
  download cities500.zip
  download admin1CodesASCII.txt
  download admin2Codes.txt
  download countryInfo.txt
  unzip -q "$workdir/cities500.zip" -d "$workdir"
  input_dir=$workdir
fi

mkdir -p "$(dirname "$output")"
normalized="$workdir/places.csv"

admin1_file="$input_dir/admin1CodesASCII.txt"
admin2_file="$input_dir/admin2Codes.txt"
country_file="$input_dir/countryInfo.txt"
cities_file="$input_dir/cities500.txt"

LC_ALL=C.UTF-8 awk -F '\t' -v OFS='\t' \
  -v admin1_file="$admin1_file" \
  -v admin2_file="$admin2_file" \
  -v country_file="$country_file" \
  -v cities_file="$cities_file" '
  FILENAME == admin1_file {
    sub(/\r$/, "")
    admin1_name[$1] = $2
    admin1_ascii[$1] = $3
    admin1_id[$1] = $4
    next
  }
  FILENAME == admin2_file {
    sub(/\r$/, "")
    admin2[$1] = $2
    next
  }
  FILENAME == country_file {
    sub(/\r$/, "")
    if ($0 !~ /^#/ && $1 != "") {
      country = toupper($1)
      countries[country] = 1
      country_name[country] = $5
      country_id[country] = $17
    }
    next
  }
  FILENAME == cities_file {
    sub(/\r$/, "")
    country = toupper($9)
    if (!(country in countries)) {
      next
    }
    admin1_key = country "." $11
    population = ($15 == "" ? 0 : $15)
    print $1, $2, $3, country,
          admin1_name[admin1_key],
          admin2[country "." $11 "." $12],
          $5, $6, population
    if ($5 != "" && $6 != "") {
      country_lat[country] += $5
      country_lon[country] += $6
      country_count[country]++
      if (admin1_key in admin1_id) {
        admin1_lat[admin1_key] += $5
        admin1_lon[admin1_key] += $6
        admin1_count[admin1_key]++
      }
    }
  }
  END {
    for (admin1_key in admin1_id) {
      if (admin1_count[admin1_key] == 0) {
        continue
      }
      split(admin1_key, parts, ".")
      print admin1_id[admin1_key],
            admin1_name[admin1_key],
            admin1_ascii[admin1_key],
            parts[1],
            admin1_name[admin1_key],
            "",
            sprintf("%.8f", admin1_lat[admin1_key] / admin1_count[admin1_key]),
            sprintf("%.8f", admin1_lon[admin1_key] / admin1_count[admin1_key]),
            0
    }
    for (country in country_id) {
      if (country_count[country] == 0) {
        continue
      }
      # countryInfo has no separate ASCII-name field; its display name is the
      # normalized fallback used for both columns.
      print country_id[country],
            country_name[country],
            country_name[country],
            country,
            "",
            "",
            sprintf("%.8f", country_lat[country] / country_count[country]),
            sprintf("%.8f", country_lon[country] / country_count[country]),
            0
    }
  }
' "$admin1_file" "$admin2_file" "$country_file" "$cities_file" > "$normalized"

rows=$(wc -l < "$normalized")
minimum_rows=1
if [[ -z "$source_dir" ]]; then
  minimum_rows=100000
fi
if [ "$rows" -lt "$minimum_rows" ]; then
  echo "GeoNames normalization produced only $rows rows" >&2
  exit 1
fi

mv "$normalized" "$output"
