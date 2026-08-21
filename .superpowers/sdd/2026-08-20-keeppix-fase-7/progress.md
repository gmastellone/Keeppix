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

## Task 2bis — Verifica reale IT/EN prima di chiudere il modello

Ruling: **si tiene MobileCLIP2-S2.** — Sul banco di 20 coppie foto-didascalia
distinguibili (Wikimedia, non dog/cat), S2 fa EN recall@1=1.00 / MRR=1.000 e
IT recall@1=0.95 / MRR=0.967 (gap R@1=0.05, gap MRR=0.033). Il divario è
piccolo: non giustifica un checkpoint genuinamente multilingua. — Costo se
sbagliato: qualche tag con prompt italiano “difficile” potrebbe rankare peggio;
mitigazione futura = prompt bilingue o traduzione EN del solo testo del tag,
senza cambiare modello.

Ruling: **variante multilingua misurata solo come tetto, non scaricata.** —
`immich-app/nllb-clip-base-siglip__v1` ha `textual/model.onnx` ≈ **1.66 GB**
solo di pesi testo (oltre ~373 MB visual): a lotto minimo utile sforerebbe il
tetto duro Task 6 (< 1 GB RSS). Con gap IT/EN già stretto su S2, non si spende
il download. — Costo se sbagliato: si ripeserà solo se in produzione i prompt
IT falliscono in modo misurabile.

Ruling: **download script esteso a text+tokenizer; banco in testdata.** —
`scripts/download-mobileclip2-s2.sh` tira anche `text.onnx` (+`.data`),
`tokenizer.json`, config. Foto del banco via `scripts/download-ai-bench.sh`
in `models/bench-it-en/` (gitignore); caption IT/EN in
`crates/keeppix-media/testdata/ai-bench/captions.json`. — Costo se sbagliato:
CI senza script salta il bench (assert soft).

MEASUREMENT (questo host ≠ Pi, `keeppix-media` opt-level=2 in test, ort CPU):
- EN: recall@1=1.00, recall@5=1.00, MRR=1.000
- IT: recall@1=0.95, recall@5=1.00, MRR=0.967 (unica miss: id 18 rank 3)
- ms/foto vision ≈ 43–44; ms/testo ≈ 15–17
- RSS: before≈9 MB; after_load≈386–390 MB; peak_infer≈413–423 MB;
  after_drop≈388–392 MB (ORT non restituisce subito tutte le pagine al SO —
  Task 6 dovrà verificare lo scarico “utile”, non solo Drop)
- tetto 1 GiB: **rispettato** (peak ≈ 0.39–0.40 GiB)

Task 2bis: complete

## Task 3 — Immagine Postgres con pgvector

Ruling: **`Dockerfile.db` = `postgis/postgis:17-3.5` + `postgresql-17-pgvector`
via apt (PGDG).** — Perché: né l'immagine PostGIS né quella pgvector portano
entrambe le estensioni; il pacchetto Debian ufficiale per PG 17 è già nel
repo PGDG montato dall'immagine base. Verificato: `CREATE EXTENSION vector`
funziona sul build locale. — Costo se sbagliato: se un giorno PGDG ritarda
il pacchetto per una major, si ricompila da sorgente nel Dockerfile.

Ruling: **testcontainers restano su `postgis/postgis:17-3.5` (senza
pgvector) fino al Task 4.** — Perché: Task 3 non introduce ancora lo schema
vettoriale; l'immagine PostGIS-only è il percorso degradato reale di chi
usa un Postgres esterno senza l'estensione, e i test `pgvector.rs` lo
coprono. Passare all'immagine custom richiederebbe pubblicarla o un build
in CI per ogni suite. — Costo se sbagliato: Task 4 dovrà far girare i test
di schema su un Postgres con `vector` (immagine custom o `KEEPPIX_TEST_DATABASE_URL`).

Ruling: **stato in `system_settings.pgvector`, non dentro `capabilities.extra`.** —
`capabilities` è il risultato di `keeppix_media::probe()` (niente DB);
pgvector è una proprietà del Postgres collegato, scritta da
`persist_pgvector_status` in `keeppix-db` all'avvio. Messaggio inglese +
`CREATE EXTENSION IF NOT EXISTS vector;` quando `available == false`;
l'avvio non fallisce. — Costo se sbagliato: il pannello (Task 6) dovrà
leggere due chiavi invece di una; banale.

Task 3: complete

## Task 4 — Schema AI

Ruling: **migrazione `0043` condizionale su `pg_available_extensions`.** —
`CREATE EXTENSION IF NOT EXISTS vector` + tabelle solo se il pacchetto
pgvector è installato; altrimenti NOTICE e return. Così un Postgres esterno
senza il pacchetto applica la migrazione come no-op (galleria parte) e
`probe_pgvector` resta la fonte di verità per il degrade Task 3. — Costo se
sbagliato: chi installa pgvector *dopo* il primo migrate trova la 0043 già
«applied» senza tabelle; DEPLOY.md documenta di rieseguire il DDL a mano.
Alternativa (CREATE EXTENSION hard) avrebbe rotto l’avvio senza pacchetto.

Ruling: **niente HNSW/IVFFlat in 0043** — Task 11. Indici creati:
`asset_embeddings_model_idx`, `tags_parent_idx`, `asset_tags_tag_idx`,
`asset_tags_proposed_idx`.

Ruling: **un solo livello di nesting `parent_id`** — vincolo applicativo,
non CHECK: PostgreSQL non ammette subquery nei CHECK. — Costo se sbagliato:
un bug di API potrebbe creare nipoti finché il domain non valida.

Ruling: **harness `keeppix-db` → `keeppix-db:dev`, con fallback.** —
`KEEPPIX_TEST_DATABASE_URL` si usa solo se quel server offre `vector`;
altrimenti testcontainers su `keeppix-db:dev`. Il degrade resta in
`TestDb::start_postgis_only` (`postgis/postgis:17-3.5`). — Costo se
sbagliato: CI senza Docker image locale fallisce il boot del container
(serve `docker build -f Dockerfile.db -t keeppix-db:dev .`).

Task 4: complete (commits 4df8093, 0a3332c, 00313db; test verdi su
`keeppix-db` migrations + pgvector)

## Task 5 — Calcolare le impronte

Ruling: **`libraries.culling_root_folder_id` arriva in migrazione `0044`
(nullable), non si aspetta Fase 9.** — Perché: il predicato
`NOT (f.path <@ cull.path)` deve compilare già ora; con NULL la LEFT JOIN
rende il filtro inerte. Fase 9 imposterà il valore, non ricreerà la colonna.
— Costo se sbagliato: migrazione Fase 9 dovrà usare `ADD COLUMN IF NOT EXISTS`
(o diventare no-op).

Ruling: **`model_version` stabile = `mobileclip2-s2` (`keeppix_media::MODEL_VERSION`).**
— Allineato a `extra.ai.model_version` del probe quando `inference_status=ok`.
— Costo se sbagliato: cambio nome = ricalcolo completo di tutti gli embedding.

Ruling: **`JobKind::EmbedAssets` + `EmbeddingRepo::{list_pending,upsert,get}`.**
— Media fa solo inferenza a lotto (`embed_images_nchw_batch`); jobs orchestra
thumb→NCHW→batch→DB. Ingresso = `*-thumb.webp` 240px; originali non letti
(test: originali corrotti dopo derive). Esclusi: culling subtree, già
embedded stessa versione, kind ≠ `image`, hash assente / thumb mancante.
— Costo se sbagliato: video restano fuori finché non si decide un percorso
dedicato; accettabile per questa fase.

Task 5: complete (commits 5650784, dfe8f94, 2be46f1, 6f1655b, 9507f79;
test verdi: keeppix-db embeddings/migrations, keeppix-jobs embed,
keeppix-media clip+probe)

## CI unblock after Task 5 (pre-Task 6)

Ruling: **`measure_rss_peak_during` in wired-exceptions (fase-7).** — Helper
pubblico per il tetto RSS; Task 6 lo collega allo scheduler. Fino ad allora
solo i test. — Costo se sbagliato: debito wired se Task 6 dimentica di
togliarlo dalla lista quando lo chiama da produzione.

Ruling: **rimossi i pin `rsa`/`crypto-primes`/`crypto-bigint` da
`keeppix-jobs`.** — Servivano solo a sbloccare il lockfile quando ort entrò
(Task 2); senza pin la risoluzione Cargo resta verde e `cargo deny` non
segnala più RUSTSEC-2023-0071 sul pin diretto (rsa resta transitivo via
russh, come prima di Fase 7). — Costo se sbagliato: un bump futuro di russh
potrebbe richiedere di nuovo un pin esplicito.

Ruling: **ignore `RUSTSEC-2024-0436` (`paste`).** — Proc-macro di build via
`tokenizers` (CLIP BPE); upstream ≤0.23 dipende ancora da `paste`, non da
`pastey`. Non finisce nel binario. — Costo se sbagliato: advisory resta
aperta finché tokenizers non migra.

Ruling: **Docker builder `rust:1.88-trixie` + runtime
`distroless/cc-debian13` + staging libraw/heif su `debian:trixie-slim`.** —
I prebuilt ort (`download-binaries`) referenziano glibc ≥2.38 /
`_M_replace_cold`; bookworm/debian12 falliscono al link. — Costo se
sbagliato: host di deploy devono avere un runtime Debian 13-compatible
(già vero per un'immagine distroless fresca); Pi OS bookworm-host che
corre l'immagine containerizzata non è affetto (glibc è dentro
l'immagine).


Ruling: **CI costruisce `keeppix-db:dev` prima dei test.** — Il service
`postgis/postgis:17-3.5` non ha pgvector; lo harness `keeppix-db` ricade su
testcontainers `keeppix-db:dev`, che non è su Docker Hub. Senza lo step di
build, albums/embeddings abortiscono con pull 404. — Costo se sbagliato:
~1–2 min in più per job backend; accettabile.


Ruling: **CI scarica MobileCLIP2-S2 (cache Actions); i test embed
saltano se i pesi mancano.** — Stesso contratto di probe/bench: zero rete
a runtime, pesi via script. Skip evita panic locali senza models/; il
download in CI fa girare i test per davvero. — Costo se sbagliato: ~150–
300 MB su cache Actions + un fetch HF al miss.

