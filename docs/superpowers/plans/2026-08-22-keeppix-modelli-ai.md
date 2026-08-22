# Piano — Modelli IA: licenze, sostituzioni, ottimizzazione IT/EN

**Stato:** deciso, misurato, non ancora implementato.
**Origine:** verifica di licenza del 22 agosto 2026 sui testi reali (non su
riassunti) + doppio benchmark eseguito lo stesso giorno.
**Quando:** il Task A (volti) prima di chiudere la roadmap; il Task B (CLIP)
dopo la Fase 11, prima di qualsiasi offerta commerciale.

---

## Il problema: i pesi attuali sono research-only

Verificato leggendo i file di licenza originali:

- **MobileCLIP2** (Fase 7, già in `main`): i pesi sono sotto "Apple Machine
  Learning Research Model License" — *"'Research Purposes' does not include
  any commercial exploitation, product development or use in any commercial
  product or service."* Il codice è MIT; **i pesi no**.
- **InsightFace SCRFD/ArcFace** (previsti dalla Fase 8, mai scaricati —
  correttamente: nessuna fonte verificata trovata): stesso schema, pesi
  *"available for non-commercial research purposes only"*.

Keeppix è AGPL-3.0 con doppia licenza commerciale pianificata: questi pesi
non possono far parte di nessuna offerta commerciale. Da qui i due task.

---

## Task A — Volti: YuNet + SFace al posto di SCRFD + ArcFace

**Scelta dell'utente, su confronto verificato**: OpenCV Zoo, licenze
MIT (YuNet) e Apache 2.0 (SFace) — libere anche per uso commerciale, e più
leggere di qualunque variante InsightFace (~9,5 MB totali contro 16-264 MB).
SFace ≈ 99,3-99,4% sul benchmark standard di verifica; per raggruppare foto
di famiglia (non controllo accessi) il mezzo punto in meno rispetto ad
ArcFace-ResNet100 è irrilevante; 261 MB → 9,5 MB no.

Fonti verificate (repo `github.com/opencv/opencv_zoo`, file via Git LFS):

| File | sha256 | Byte |
|---|---|---|
| `models/face_detection_yunet/face_detection_yunet_2023mar_int8.onnx` | `321aa5a6afabf7ecc46a3d06bfab2b579dc96eb5c3be7edd365fa04502ad9294` | 100.416 |
| `models/face_recognition_sface/face_recognition_sface_2021dec_int8.onnx` | `2b0e941e6f16cc048c20aee0c8e31f569118f65d702914540f7bfdc14048d78a` | 9.896.933 |

Il lavoro:

1. `scripts/download-yunet-sface.sh` sul modello di
   `download-mobileclip2-s2.sh`, con verifica sha256 dei valori sopra.
2. Adattare `crates/keeppix-media/src/face.rs` e `align.rs`: YuNet decodifica
   diversamente da SCRFD (output diversi), SFace ha il suo allineamento.
   **Il codice SCRFD/ArcFace che non serve più si elimina, non si commenta.**
3. Far girare **per la prima volta** il test end-to-end
   `detects_and_groups_faces_when_weights_are_present` (mai eseguito, nemmeno
   in CI — aggiungere il download al workflow con cache, come per MobileCLIP).
4. Misurare e mettere a ledger: ms per rilevamento, ms per impronta, RSS.
   Le soglie `ASSIGN_SIMILARITY`/`PROPOSE_SIMILARITY` sono stime mai
   calibrate: verificarle sul bench reale e annotare il Ruling.
5. Aggiornare spec/piano di Fase 8 dove nominano SCRFD/ArcFace (fatto per i
   punti principali il 22 agosto; ripassare al momento dell'implementazione).

---

## Task B — CLIP: OpenCLIP XLM-R ViT-B-32 int8, ottimizzato SOLO per IT/EN

**Decisione presa su benchmark doppio** (22 agosto): prima qualità sulle
stesse 20 coppie IT/EN del Task 2bis, poi confronto RSS **same-harness** in
onnxruntime fra l'attuale e il proposto — perché i numeri di macchine/runtime
diversi non si confrontano (il ledger dava MobileCLIP2 a 413-423 MB di picco;
nello stesso harness del confronto ne ha fatti 744).

### I numeri (banco 20 coppie IT/EN; RSS misurato in onnxruntime, CPU 4 thread)

| Candidato | IT r@1 | EN r@1 | Note |
|---|---|---|---|
| **OpenCLIP XLM-R ViT-B-32 int8** | **0.95** (MRR 0.975) | **1.00** | **scelto** — 512-d, licenza permissiva |
| MobileCLIP2-S2 (attuale, stesso harness) | 0.95 (MRR 0.967) | 1.00 | research-only |
| SigLIP2-base-p32-256 | 0.95 | 1.00 | riserva: Apache ma 768-d (migrazione `vector(512)`) e 58 ms/foto |
| TinyCLIP 40M / 8M | 0.65 / 0.35 | 1.00 / 0.95 | **squalificati**: inglese-only confermato |

| Stesso harness, lotto da 16 foto | MobileCLIP2 fp32 (attuale) | XLM int8 (proposto) |
|---|---|---|
| Visual: pesi su disco | 144 MB | **89 MB** |
| Visual: RSS picco | 744 MB | **271 MB** |
| Visual: ms/foto | 95,7 | **22,7** |
| Text: pesi / RSS picco / ms | 248 MB / 452 MB / 26,9 | 279 MB / 754 MB / 16,6 |

Il visual — l'unico che gira sulle 200.000 foto nelle finestre notturne — è
**~2,7× più leggero e ~4× più veloce**. Il text tower gira solo per creazione
tag e ricerca, mai concorrente coi lotti (una ricerca è attività utente → la
finestra si pausa e scarica il visual, per il design già esistente).

### Vincoli espliciti dell'utente

**A. Il modello spedito è solo IT/EN, non multilingua.** La matrice di
vocabolario XLM-R (250k token per 100+ lingue) è la maggior parte dei 279 MB
del text tower: potarla ai token effettivamente prodotti dal tokenizer su
corpus IT+EN, riscrivere tokenizer e matrice di embedding di conseguenza, e
misurare prima/dopo su peso **e** recall (il bench IT/EN non deve calare).
Le altre 107 lingue non sono un requisito: se la potatura le rompe, va bene.

**B. Ottimizzazione e misura in Rust, non Python.** Python è ammesso **solo**
nello script di export offline (`scripts/`, come i download): torch → ONNX
(serve `torch.backends.mha.set_fastpath_enabled(False)` con l'exporter
legacy — già verificato, senza fallisce su
`aten::_native_multi_head_attention`) + quantizzazione int8 + potatura
vocabolario. Tutto il resto — caricamento, inferenza, bench di regressione
IT/EN, misure RSS/ms — vive nel crate `ort` e nell'harness Rust esistenti
(`ai_retrieval_bench`, la strumentazione `rss_*` di `embed.rs`). I numeri che
chiudono il task sono quelli Rust sul target, non quelli Python del benchmark
di selezione.

### Il resto del lavoro

- Embed dim resta **512**: nessuna migrazione di schema; `model_version`
  gestisce il re-embed della libreria. Ricalibrare `TAG_MATCH_BAND` sui nuovi
  score, Ruling a ledger.
- L'int8 dinamico costa un colpo IT sul visual (fp32 era 1.00): provare QDQ
  statica o fp16 e scegliere col numero.
- Aggiornare download script (con sha256 dei file finali), cache CI, bench
  Rust come regressione. **Il codice e i riferimenti MobileCLIP2 che non
  servono più si eliminano** (script di download, costanti, doc) — niente
  codice morto dietro.
- Tetto duro invariato: **sotto 1 GB di RSS reale durante l'analisi**,
  misurato, non stimato.
