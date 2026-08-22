#!/usr/bin/env bash
# Scarica le ~20 foto del banco IT/EN (Fase 7 Task 2bis) da Wikimedia Commons.
# I file finiscono in models/bench-it-en/ (gitignored). I caption sono in
# crates/keeppix-media/testdata/ai-bench/captions.json (committati).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DEST="${KEEPPIX_MODELS_DIR:-$ROOT/models}/bench-it-en"
CAPTIONS="$ROOT/crates/keeppix-media/testdata/ai-bench/captions.json"
UA="KeeppixBench/1.0 (https://keeppix.dev; Fase7-Task2bis)"
mkdir -p "$DEST"

if [[ ! -f "$CAPTIONS" ]]; then
  echo "missing captions: $CAPTIONS" >&2
  exit 1
fi

python3 - "$DEST" "$CAPTIONS" "$UA" <<'PY'
import json, sys, urllib.parse, urllib.request, pathlib

dest = pathlib.Path(sys.argv[1])
captions = json.loads(pathlib.Path(sys.argv[2]).read_text())
ua = {"User-Agent": sys.argv[3]}

def resolve(filename: str) -> tuple[str, str]:
    url = "https://commons.wikimedia.org/w/api.php?" + urllib.parse.urlencode({
        "action": "query",
        "format": "json",
        "titles": "File:" + filename,
        "prop": "imageinfo",
        "iiprop": "url|mime",
        "iiurlwidth": 640,
    })
    req = urllib.request.Request(url, headers=ua)
    with urllib.request.urlopen(req, timeout=60) as r:
        data = json.load(r)
    for page in data["query"]["pages"].values():
        info = (page.get("imageinfo") or [None])[0]
        if not info:
            raise SystemExit(f"not found on Commons: {filename}")
        return info.get("thumburl") or info["url"], info["mime"]
    raise SystemExit(f"empty response for {filename}")

for pair in captions["pairs"]:
    out = dest / pair["image"]
    if out.exists() and out.stat().st_size > 0:
        print(f"· already have {out.name}")
        continue
    url, mime = resolve(pair["source_file"])
    print(f"→ {out.name} ({mime})")
    req = urllib.request.Request(url, headers=ua)
    with urllib.request.urlopen(req, timeout=120) as r, open(out, "wb") as f:
        f.write(r.read())

# Keep a working copy of captions next to images for ad-hoc runs.
(dest / "captions.json").write_text(json.dumps(captions, indent=2, ensure_ascii=False) + "\n")
print(f"✓ bench images in {dest}")
PY
