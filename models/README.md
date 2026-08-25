# Pesi ONNX locali (YuNet+SFace, OpenCLIP XLM-R IT/EN). Non vanno in git:
# vengono cotti nell'immagine Docker (spec Fase 7 §6.3). Scaricarli con:
#
#   ./scripts/download-yunet-sface.sh
#   ./scripts/download-ai-bench.sh   # foto del banco IT/EN (Task 2bis)
#
# OpenCLIP XLM-R IT/EN non ha uno script di download: niente URL esterno
# stabile per questo checkpoint potato+quantizzato. Lo produce
# `.github/workflows/export-openclip-xlmr.yml` (export Python dal checkpoint
# HuggingFace) in `models/openclip-xlmr-it-en/`.
#
# Runtime: `ort` (vedi ledger Fase 7 Task 2). Zero rete a runtime.
#
# Modelli MobileCLIP2-S2 e InsightFace (SCRFD/ArcFace) mai adottati /
# rimossi: "Apple Machine Learning Research Model License" — SOLO ricerca
# non commerciale, incompatibile con un'offerta commerciale di Keeppix.
# Sostituzioni decise e misurate — YuNet+SFace per i volti (MIT/Apache),
# OpenCLIP XLM-R int8 potato IT/EN per gli embedding (permissivo):
# piano completo in docs/superpowers/plans/2026-08-22-keeppix-modelli-ai.md
