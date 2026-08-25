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
import json, sys, time, urllib.error, urllib.parse, urllib.request, pathlib

dest = pathlib.Path(sys.argv[1])
captions = json.loads(pathlib.Path(sys.argv[2]).read_text())
ua = {"User-Agent": sys.argv[3]}

# Visto in CI (run 32870297867): 20 immagini = ~40 richieste ravvicinate
# (resolve + download per foto) bastano a far scattare il rate limit
# anonimo di Commons (HTTP 429, "reduce your request rate"). Mai capitato
# in locale — un dev lancia lo script una volta sola, non in loop veloce.
# Backoff su 429/5xx (rispettando Retry-After se presente) più una piccola
# pausa fra le foto, invece di ritentare l'intero script alla cieca.
def urlopen_retry(req, timeout, max_attempts=5):
    for attempt in range(1, max_attempts + 1):
        try:
            return urllib.request.urlopen(req, timeout=timeout)
        except urllib.error.HTTPError as e:
            if e.code not in (429, 500, 502, 503, 504) or attempt == max_attempts:
                raise
            wait = float(e.headers.get("Retry-After", 0)) or (2 ** attempt)
            print(f"  · HTTP {e.code}, retry {attempt}/{max_attempts} tra {wait:.0f}s")
            time.sleep(wait)
    raise AssertionError("unreachable")

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
    with urlopen_retry(req, timeout=60) as r:
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
    with urlopen_retry(req, timeout=120) as r, open(out, "wb") as f:
        f.write(r.read())
    time.sleep(0.5)

# Keep a working copy of captions next to images for ad-hoc runs.
(dest / "captions.json").write_text(json.dumps(captions, indent=2, ensure_ascii=False) + "\n")
print(f"✓ bench images in {dest}")
PY
