#!/usr/bin/env bash
set -euo pipefail

output=${1:-/usr/share/keeppix/tz_boundaries.csv}
source_dir=${2:-}
workdir=$(mktemp -d)
trap 'rm -rf "$workdir"' EXIT

release=2026c
archive=timezones.geojson.zip
release_url="https://github.com/evansiroky/timezone-boundary-builder/releases/download/${release}/${archive}"

if [[ -n "$source_dir" ]]; then
  input="$source_dir/combined.json"
else
  curl --fail --location --retry 3 --proto '=https' --tlsv1.2 \
    "$release_url" --output "$workdir/$archive"
  unzip -q "$workdir/$archive" -d "$workdir/source"
  input="$workdir/source/combined.json"
fi

mkdir -p "$(dirname "$output")"
normalized="$workdir/tz_boundaries.csv"
"$(dirname "${BASH_SOURCE[0]}")/build-tz-boundaries.py" \
  "$input" "$normalized" 0.01

rows=$(wc -l < "$normalized")
minimum_rows=1
if [[ -z "$source_dir" ]]; then
  minimum_rows=300
fi
if [ "$rows" -lt "$minimum_rows" ]; then
  echo "timezone boundary normalization produced only $rows rows" >&2
  exit 1
fi

mv "$normalized" "$output"
