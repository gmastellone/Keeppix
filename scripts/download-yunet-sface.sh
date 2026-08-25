#!/usr/bin/env bash
# Scarica YuNet (rilevamento) + SFace (impronta) da opencv/opencv_zoo in
# models/yunet-sface/detect.onnx e embed.onnx.
#
# Questo script verifica lo sha256: i file arrivano via Git LFS, e l'URL "raw" ovvio
# (raw.githubusercontent.com) per un percorso tracciato LFS torna il
# pointer testuale di ~130 byte, non il binario — un errore silenzioso che
# altrimenti produrrebbe un "modello" da 130 byte accettato senza fiatare.
# media.githubusercontent.com/media/... risolve l'oggetto LFS reale
# (verificato: 9.896.933 byte per SFace, 100.416 per YuNet, esatti). Lo
# sha256 sotto è la seconda rete di sicurezza, non solo contro un URL
# sbagliato ma contro qualunque byte alterato lungo la strada.
#
# Fonti e hash: docs/superpowers/plans/2026-08-22-keeppix-modelli-ai.md
# (verificati scaricando i file reali e ricalcolando sha256sum).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DEST="${KEEPPIX_MODELS_DIR:-$ROOT/models}/yunet-sface"
REPO="https://media.githubusercontent.com/media/opencv/opencv_zoo/main"
mkdir -p "$DEST"

fetch_verified() {
  local url="$1"
  local out="$2"
  local sha="$3"
  if [[ -f "$out" ]]; then
    if echo "$sha  $out" | sha256sum -c - >/dev/null 2>&1; then
      echo "· already have $(basename "$out") (sha256 verified)"
      return 0
    fi
    echo "· $(basename "$out") present but sha256 mismatch, re-downloading"
  fi
  echo "→ downloading $(basename "$out")"
  curl -fL --retry 3 -o "$out" "$url"
  if ! echo "$sha  $out" | sha256sum -c - >/dev/null 2>&1; then
    echo "✗ sha256 mismatch for $(basename "$out") — got $(sha256sum "$out" | cut -d' ' -f1), expected $sha" >&2
    rm -f "$out"
    exit 1
  fi
}

echo "→ YuNet + SFace into $DEST"
fetch_verified \
  "$REPO/models/face_detection_yunet/face_detection_yunet_2023mar_int8.onnx" \
  "$DEST/detect.onnx" \
  "321aa5a6afabf7ecc46a3d06bfab2b579dc96eb5c3be7edd365fa04502ad9294"
fetch_verified \
  "$REPO/models/face_recognition_sface/face_recognition_sface_2021dec_int8.onnx" \
  "$DEST/embed.onnx" \
  "2b0e941e6f16cc048c20aee0c8e31f569118f65d702914540f7bfdc14048d78a"

ls -lh "$DEST/detect.onnx" "$DEST/embed.onnx"
echo "✓ YuNet+SFace ready (local files, no runtime download, sha256 verified)"
