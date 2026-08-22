# Pesi ONNX locali (MobileCLIP2-S2, …). Non vanno in git: ~400 MB
# (visual+text) e vengono cotti nell'immagine Docker (spec Fase 7 §6.3).
# Scaricarli con:
#
#   ./scripts/download-mobileclip2-s2.sh
#   ./scripts/download-ai-bench.sh   # foto del banco IT/EN (Task 2bis)
#
# Runtime: `ort` (vedi ledger Fase 7 Task 2). Zero rete a runtime.
#
# ⚠️ LICENZA (verificato 22 agosto 2026): i pesi MobileCLIP2 sono sotto
# "Apple Machine Learning Research Model License" — SOLO ricerca non
# commerciale. Idem i pesi InsightFace (SCRFD/ArcFace, mai scaricati).
# Nessuno dei due può far parte di un'offerta commerciale di Keeppix.
# Sostituzioni decise e misurate — YuNet+SFace per i volti (MIT/Apache),
# OpenCLIP XLM-R int8 potato IT/EN per gli embedding (permissivo):
# piano completo in docs/superpowers/plans/2026-08-22-keeppix-modelli-ai.md
