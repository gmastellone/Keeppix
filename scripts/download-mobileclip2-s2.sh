#!/usr/bin/env bash
# Scarica MobileCLIP2-S2 (visual) da HuggingFace in models/ (gitignored).
# I pesi vengono cotti nell'immagine Docker (spec Fase 7 §6.3).
# Zero rete a runtime: questo script è solo per CI/dev/bake.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DEST="${KEEPPIX_MODELS_DIR:-$ROOT/models}/mobileclip2-s2"
REPO="https://huggingface.co/RuteNL/MobileCLIP2-S2-OpenCLIP-ONNX/resolve/main"
mkdir -p "$DEST"

echo "→ downloading visual.onnx (+ external data) into $DEST"
curl -fL --retry 3 -o "$DEST/visual.onnx" "$REPO/visual.onnx"
curl -fL --retry 3 -o "$DEST/visual.onnx.data" "$REPO/visual.onnx.data"
curl -fL --retry 3 -o "$DEST/open_clip_config.json" "$REPO/open_clip_config.json"
ls -lh "$DEST/visual.onnx" "$DEST/visual.onnx.data"
echo "✓ MobileCLIP2-S2 visual ready (local files, no runtime download)"
