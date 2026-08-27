#!/usr/bin/env bash
# Downloads YuNet (detection) + SFace (embedding) from opencv/opencv_zoo into
# models/yunet-sface/detect.onnx and embed.onnx.
#
# This script verifies sha256: the files are served via Git LFS, and the obvious
# "raw" URL (raw.githubusercontent.com) for an LFS-tracked path returns the
# ~130-byte text pointer, not the binary — a silent failure that would
# otherwise produce a 130-byte "model" accepted without complaint.
# media.githubusercontent.com/media/... resolves the actual LFS object
# (verified: exactly 9,896,933 bytes for SFace, 100,416 for YuNet). The
# sha256 below is the second line of defense, guarding not just against a
# wrong URL but against any byte altered along the way.
#
# Sources and hashes below were verified by downloading the actual files
# and recomputing sha256sum.
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
