# Fase 7 — progress ledger

Branch: `fase-7` from `main` @ `1f1d3e9` (post Fase 10 merge + plan revisit).

## Startup

Read PROSEGUI.md (21 ago): Fase 10 chiusa; prossimo = 7; piani 7/8/9 ripassati.
Piano: `docs/superpowers/plans/2026-08-20-keeppix-fase-7.md` (13 task).
Spec: `docs/superpowers/specs/fase-7-ai-tag-scene.md`.


## Task 1 — Estendere il probe con l'inferenza AI

Ruling: **Task 1 misura i fatti host; i ms di inferenza restano
`pending_runtime` fino a Task 2.** — Perché: il piano ordina Task 1 prima di
Task 2 (tract/ort + modello), e senza runtime/pesi un'inferenza "vera" sul
MobileCLIP non esiste ancora. Inventare un tempo su un toy model mentirebbe
all'utente (i livelli Piena/Ridotta mostrano quel numero). Si scrive
`extra.ai` con `free_ram_bytes` (`MemAvailable`), `cpu_cores`, `has_neon`,
`inference_ms: null`, `inference_status: "pending_runtime"`; Task 2 riempie
i ms quando sceglie il runtime. — Costo se sbagliato: Task 1 non produce
ancora il numero che la UI promette; accettabile se Task 2 arriva subito.

Ruling: **`get_json` esce dai rinvii** — già chiamato da `transcode.rs`
(Fase 6); Task 1 aggiunge `load_ai_host_facts` (get_json → `extra.ai`)
chiamato all'avvio da `main` dopo `persist_capabilities`. — Costo se
sbagliato: nessuno; la guardia wired resta verde.

MEASUREMENT (questo host, Task 1): free_ram_bytes ~13.6 GiB, cpu_cores = 4,
has_neon = false, inference_status = pending_runtime.

Task 1: complete

## Task 2 — tract o ort, deciso per prova

Ruling: **runtime = `ort`.** — Tract è stato provato per primo sul
MobileCLIP2-S2 visual ONNX: con `batch_size` simbolico fallisce l'analisi
(`Impossible to unify Sym(batch_size) with Val(1)`); dopo rewrite bake-time
a batch=1 gira in isolamento (~371 ms/foto, load+optimize ~530 ms). Ort
carica l'export HF stock e inferisce ~42–67 ms/foto (load ~220 ms) su questo
host. Tract **non entra nel workspace**: `tract-data` pinna `libm = 0.2.11`
mentre `crypto-primes` (stack russh → rsa) vuole `libm ^0.2.13`. Integrare
tract rompe la risoluzione Cargo; ort (MIT/Apache-2.0, `download-binaries` +
`tls-rustls`) sì. — Costo se sbagliato: dipendenza C++/libstdc++ in Docker
distroless (già accettata per stile LibRaw); se un giorno tract risolve il
conflitto `libm`, si può ripesare.

Ruling: **pesi locali sotto `models/mobileclip2-s2/` (gitignored).** —
`./scripts/download-mobileclip2-s2.sh` scarica
`RuteNL/MobileCLIP2-S2-OpenCLIP-ONNX` (`visual.onnx` + `.data` ≈ 140 MB).
Override: `KEEPPIX_AI_VISUAL_ONNX` / `KEEPPIX_MODELS_DIR`. Zero rete a
runtime. Docker bake (Task successivi) cuoce gli stessi file. — Costo se
sbagliato: CI senza script → `inference_status=model_missing` (esplicito).

Ruling: **pin rsa/crypto-primes/crypto-bigint in `keeppix-jobs`.** — Serve
a far aggiornare il lockfile quando si aggiunge ort: rsa 0.10.0-rc.17
tirerebbe `crypto-bigint 0.7.5` contro il pin `=0.7.0-rc.28` di russh. —
Costo se sbagliato: pin da rivedere al prossimo bump russh.

MEASUREMENT (questo host, Task 2, release):
- tract (bs1 rewrite, isolamento): ~371 ms/inferenza, ~530 ms load+opt
- ort (stock ONNX, via keeppix-media): ~67 ms/inferenza (probe diretto ort
  ~42–45 ms); load ~220 ms
- chosen: ort → `extra.ai.inference_status=ok`, `inference_runtime=ort`,
  `inference_ms` misurato all'avvio

Task 2: complete
