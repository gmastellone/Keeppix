#!/usr/bin/env bash
# Scarica MobileCLIP2-S2 (visual + text + tokenizer) da HuggingFace in models/.
# I pesi vengono cotti nell'immagine Docker (spec Fase 7 §6.3).
# Zero rete a runtime: questo script è solo per CI/dev/bake.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DEST="${KEEPPIX_MODELS_DIR:-$ROOT/models}/mobileclip2-s2"
REPO="https://huggingface.co/RuteNL/MobileCLIP2-S2-OpenCLIP-ONNX/resolve/main"
mkdir -p "$DEST"

fetch() {
  local name="$1"
  local out="$DEST/$name"
  if [[ -f "$out" && "${KEEPPIX_FORCE_DOWNLOAD:-}" != "1" ]]; then
    echo "· already have $name"
    return 0
  fi
  echo "→ downloading $name"
  curl -fL --retry 3 -o "$out" "$REPO/$name"
}

echo "→ MobileCLIP2-S2 into $DEST"
fetch visual.onnx
fetch visual.onnx.data
fetch text.onnx
fetch text.onnx.data
fetch tokenizer.json
fetch tokenizer_config.json
fetch special_tokens_map.json
fetch open_clip_config.json
fetch model_config.json

ls -lh \
  "$DEST/visual.onnx" "$DEST/visual.onnx.data" \
  "$DEST/text.onnx" "$DEST/text.onnx.data" \
  "$DEST/tokenizer.json"
echo "✓ MobileCLIP2-S2 visual+text ready (local files, no runtime download)"
