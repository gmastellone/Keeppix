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
