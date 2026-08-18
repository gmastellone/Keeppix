#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

fixture="$tmp/fixture"
mkdir -p "$fixture" "$tmp/bin"

cat > "$tmp/bin/curl" <<'EOF'
#!/usr/bin/env bash
echo "network access is forbidden in the timezone normalizer test" >&2
exit 99
EOF
chmod +x "$tmp/bin/curl"

cat > "$fixture/combined.json" <<'EOF'
{
  "type": "FeatureCollection",
  "features": [
    {
      "type": "Feature",
      "properties": {"tzid": "Asia/Tokyo"},
      "geometry": {
        "type": "Polygon",
        "coordinates": [[[138,34],[139,34],[140,34],[141,34],[141,37],[138,37],[138,34]]]
      }
    },
    {
      "type": "Feature",
      "properties": {"tzid": "Europe/Rome"},
      "geometry": {
        "type": "MultiPolygon",
        "coordinates": [[[[6,35],[19,35],[19,48],[6,48],[6,35]]]]
      }
    }
  ]
}
EOF

output="$tmp/tz_boundaries.csv"
PATH="$tmp/bin:$PATH" "$root/scripts/build-tz-boundaries.sh" "$output" "$fixture"

python3 - "$output" <<'PY'
import json
import pathlib
import sys

rows = {}
for line in pathlib.Path(sys.argv[1]).read_text(encoding="utf-8").splitlines():
    name, geometry = line.split("\t", 1)
    rows[name] = json.loads(geometry)

assert set(rows) == {"Asia/Tokyo", "Europe/Rome"}
assert all(geometry["type"] == "MultiPolygon" for geometry in rows.values())
assert rows["Asia/Tokyo"]["coordinates"][0][0][0] == [138.0, 34.0]
PY

echo "timezone boundary normalizer fixture test: ok (2 zones, no network)"
