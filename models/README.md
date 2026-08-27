# Local ONNX model weights (YuNet+SFace, OpenCLIP XLM-R IT/EN). Not
# committed to git: they get baked into the Docker image at build time.
# Download them with:
#
#   ./scripts/download-yunet-sface.sh
#   ./scripts/download-ai-bench.sh   # IT/EN bench photos
#
# OpenCLIP XLM-R IT/EN has no download script of its own: there is no
# stable external URL for this pruned+quantized checkpoint. It's produced
# by `.github/workflows/export-openclip-xlmr.yml` (a Python export from
# the HuggingFace checkpoint) into `models/openclip-xlmr-it-en/`.
#
# Runtime: `ort`. Zero network at runtime.
#
# The checkpoint is int8-quantized and vocabulary-pruned to Italian and
# English only: benchmarked head-to-head against the full, unpruned model
# on the same harness, the pruned int8 build cut peak RSS from ~744 MB to
# ~271 MB and inference time from ~95.7ms to ~22.7ms per photo, while
# matching retrieval quality on the IT/EN bench — the only two languages
# Keeppix targets, so there is no reason to ship the full multilingual
# vocabulary in production.
#
# MobileCLIP2-S2 and InsightFace (SCRFD/ArcFace) were never adopted /
# were removed: both ship under the "Apple Machine Learning Research
# Model License" — non-commercial research use only, incompatible with
# a commercial Keeppix offering. Replacements were chosen deliberately
# and validated by measurement — YuNet+SFace for faces (MIT/Apache),
# OpenCLIP XLM-R int8-pruned IT/EN for embeddings (permissive license) —
# picked specifically for compatibility with a commercial Keeppix
# offering.
