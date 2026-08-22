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


## Task 6 — Scheduler dell'analisi

Ruling: **`AnalysisLevel::{Full,Reduced,Off}` con ms misurati (45 / 270 /
None).** — Task 2bis: vision ≈ 43–44 ms → Full=45; Reduced=6× (doc UI).
`Off` spegne l'analisi (pgvector assente / scelta operatore). — Costo se
sbagliato: stime ETA leggermente off su Pi; si ricalibrano dal probe.

Ruling: **`max_claimable_priority` + `analysis_should_run` nel WorkerPool.**
— Viewport fresco (≤4000 ms) cap a `Visible`: i job `Background`
(`EmbedAssets` backfill) non partono; Visible/High sì. — Costo se sbagliato:
qualche job Background non-AI resta in pausa con l'analisi (accettabile:
sono già Background).

Ruling: **ingest → `enqueue_after_ingest` (High, dedup); boot →
`schedule_backfill` (Background); re-queue a fine lotto.** — Foto nuove
non aspettano la notte; il modello resta caricato solo per il lotto
(`measure_rss_peak_during` su load + infer). — Costo se sbagliato: coda
High troppo aggressiva sotto carico WebDAV; mitigazione = dedup + batch.

MEASUREMENT (Task 2bis, riusata): peak RSS infer ≈ 413–423 MB ≪ 1 GiB
ceiling. Il job logga `rss_after_load_bytes` / `rss_peak_infer_bytes`.

Task 6: complete


Ruling: **harness worker esce quando non ci sono job *reclamabili*.** —
`pending` con `run_after` nel futuro (retry backoff) non tiene vivo il
JoinHandle: altrimenti cancel-scan scade a 20s. Derive accoda embed solo
se i pesi CLIP ci sono. — Costo se sbagliato: un test che aspetta i retry
dovrebbe fare sleep esplicito; oggi nessuno lo fa.


## Task 7 — Tag e categorie CRUD

Ruling: **vocabolario condiviso, qualsiasi utente autenticato.** — Spec:
i tag non sono per-utente; `created_by` è audit. Admin non richiesto.
— Costo se sbagliato: un utente può rinominare/cancellare tag creati da
altri; accettabile per un archivio di famiglia, rivedibile con ACL in
fase successiva.

Ruling: **id sconosciuto → Forbidden (mai NotFound).** — Invariante
AGENTS anche sul vocabolario condiviso: niente oracolo di esistenza.
— Costo se sbagliato: client che aspettano 404 vanno aggiornati.

Ruling: **solo `kind=tag` ha embedding testuale; le categorie no.** —
Labbinamento (Task 8) è per tag; le categorie sono contenitori. Prompt
assente → si embedda il `name`. Pesi assenti → create ok con
`embedding=NULL`; patch del testo azzera un vettore stantio.
— Costo se sbagliato: matching ritardato finché non ci sono i pesi.

Ruling: **`assignment_count` su GET/list per il dialog di delete.** —
Il cascade è già FK; la UI legge il conteggio prima di confermare.
— Costo se sbagliato: conteggio include rejected (tutte le righe
`asset_tags`); onesto per «foto coinvolte» dalla cancellazione.

Ruling: **harness API → `keeppix-db:dev` (pgvector).** — Allineato a
`keeppix-db` tests: senza `vector` lo schema AI è no-op e i test tags
mentirebbero (503). `provision_dedicated` resta postgis-only (test 503).
— Costo se sbagliato: CI deve già buildare `Dockerfile.db` (già vero).

Task 7: complete

Ruling: **/tags e /tags/{id} in wired-exceptions → fase-11.** — Task 7 è solo API; la pagina Tag/categorie è UI della 11 (come bootstrap/timeline). — Costo se sbagliato: eccezione da togliere quando arrivano i componenti Vue.

Ruling: **API harness: postgis di default; `start_with_vector` solo per tags.** — Forzare keeppix-db:dev su tutti i test API in CI (URL senza vector) ha riacceso il flake bootstrap (`individual=0`). Stesso schema di keeppix-jobs. — Costo se sbagliato: dimenticare start_with_vector su un nuovo test AI → 503.

Ruling: **budget bootstrap → capture sqlx global + lock.** — `set_default` TLS perde eventi quando sqlx logga da un altro worker sotto `--test-threads>1`. `set_global_default` una volta + `BUDGET_LOCK` tra i due test. — Costo se sbagliato: un altro test del binario che chiama set_global_default per primo spegne il capture (assert individual>0 fallisce esplicito).

## Task 8 — Abbinamento tag↔foto

Ruling: **`TAG_MATCH_BAND = 0.01f` (un punto percentuale).** — Spec: la
banda sotto soglia è una costante di sistema «un punto percentuale sotto»,
non esposta in API. Unico hint numerico nella spec → `0.01`. Score ≥
`threshold − BAND` → `state='proposed'`, `source='ai'`; sotto → niente.
Anche sopra soglia resta `proposed` (Task 9 conferma). — Costo se
sbagliato: banda troppo stretta perde proposte deboli; troppo larga
riempie la coda. Regolabile solo con un deploy.

Ruling: **ON CONFLICT aggiorna solo `state='proposed'`.** — Spec: decisioni
umane (`confirmed`/`rejected`) permanenti. Rematch aggiorna lo `score` delle
proposte esistenti; non tocca le altre. — Costo se sbagliato: un bug nella
clausola WHERE del DO UPDATE riporterebbe rifiuti in coda.

Ruling: **filtro `asset_embeddings.model_version = tags.model_version`.** —
Vettori di modelli diversi non sono confrontabili; mismatch → skip.
— Costo se sbagliato: score spazzatura tra spazi latenti.

Ruling: **API rematch solo su create con embedding o patch che riscrive
l'embedding (name/prompt).** — Threshold/color/parent da soli non chiamano
`propose_for_tag`: la soglia governa le analisi *future*. Job embed chiama
`propose_for_assets` sul lotto appena upsertato. — Costo se sbagliato:
cambiare soglia non ripulisce proposte già sotto soglia (accettato dalla
spec: «Cambiare la soglia non rivaluta nulla di già deciso»).

Task 8: complete (commits 5051e9e, bba350a, 217fa2e, fc5a524; test verdi
`keeppix-db` asset_tags, `keeppix-api` tags; clippy -D warnings)

## Task 6 follow-up — RSS dopo Drop (debito Task 2bis)

Ruling: **`embed.rs` logga `rss_after_drop_bytes` (con before/load/peak).** —
Drop della sessione ONNX a fine lotto; VmRSS campionato subito dopo.
MEASUREMENT (questo host, test `dropping_the_session_…`): before≈8.7 MB,
after_load≈369 MB, peak_infer≈370 MB, after_drop≈369 MB. Scende **dal
picco verso la base di sessione (after_load)**, non fino alla base di
processo: l’allocatore ORT trattiene pagine finché vive il processo — lo
scarico “utile” è liberare la sessione tra lotti (non tenere 370 MB × N
lotti sovrapposti). — Costo se sbagliato: su Pi le cifre cambiano; il
log resta la prova operativa.


## Embed session keepalive (finestra, non lotto)

Ruling: **la sessione ONNX resta viva per tutta la finestra di analisi**,
non si ricarica ogni `DEFAULT_BATCH_SIZE` (16). Si processa a lotti di 16
controllando `ActivityTracker::analysis_should_run` fra un lotto e l'altro;
Drop della sessione alla pausa o a coda vuota. `DEFAULT_BATCH_SIZE` resta
16 apposta (reattività): alzarlo terrebbe la CPU satura dopo che l'utente
riprende a navigare. `maybe_requeue_backfill` solo se la pausa lascia
pending. La Ruling del piano («per finestra o lotto») lo consentiva già;
l'implementazione precedente sceglieva il lotto e pagava ~7 load/100 foto
senza beneficio di RAM.

MEASUREMENT (questo host, release/`opt-level=2`, test
`session_keepalive_beats_reload_every_batch_for_100_photos`, NCHW sintetico,
7 lotti × 16): **OLD** total≈6068 ms (load≈2166, infer≈3712) — **NEW**
total≈3863 ms (load≈306, infer≈3514). Risparmio ≈2.2 s / 100 foto (~36%
del wall clock vecchio; load 7× → 1×). Su 200k foto a ~45 ms/foto di sola
inferenza, i ~220 ms/load × ceil(N/16) erano ~31% di overhead.

Ruling: **dopo il primo lotto ~369–404 MB di `VmRSS` restano residenti nel
processo per tutta la sua vita** (misura after_drop ≈ after_load; non cresce
fra i lotti; sotto il tetto 1 GiB). Compromesso accettato — docs.rs/ort e
onnxruntime#11627: l'allocatore ORT non restituisce le pagine all'OS al
Drop della `Session`. Scaricare/ricaricare ogni 16 non liberava RAM utile;
serviva solo a ripagare il parsing del grafo. — Costo se sbagliato: su Pi
con poca headroom sotto 1 GiB il resident fisso resta visibile in `ps`;
non c'è un percorso «scarica davvero» senza uscire dal processo.

Embed keepalive: complete (test verdi jobs embed window/pause + media
keepalive 100-photo; clippy -D warnings ok)

## Task 9 — Coda di revisione + bootstrap.revision

Ruling: **`badges.revision` = count_proposed_visible** (metà tag; Fase 8
aggiungerà i volti sullo stesso campo). `count_proposed_visible` non
propaga assenza pgvector (0) — bootstrap non deve fallire per IA
opzionale. Bulk confirm/reject per tag usano `BulkOutcome::from_partition`
senza fallimenti (solo asset visibili). Rotte HTTP in wired-exceptions
fase-11 (UI). OpenAPI 144→149.

Task 9: complete (db 971ae1d + api HTTP/bootstrap/openapi; test verdi
tags review_queue, bootstrap budget, openapi)

Nota: `cargo clippy --workspace --all-targets -- -D warnings` sul commit
`eee260f` (embed keepalive) falliva — `peek`/`peak` in `embed::run`
urtavano `clippy::similar_names`, e bloccava anche `keeppix-api` (che
dipende da `keeppix-jobs`). Fix in e1e595f: la probe one-shot dei
pending non ha più un binding nominato. Nessun cambio di comportamento.
Verificato dopo il fix: `cargo fmt --check` e
`cargo clippy --workspace --all-targets -- -D warnings` puliti;
`keeppix-db` asset_tags (21), `keeppix-api` tags/bootstrap/openapi/scan
(26), `keeppix-jobs` embed/ingest_fixture/production_config (23) verdi.

## Task 12 — OperationKind::AiAnalysis

Ruling: **una `Operation` AiAnalysis per finestra `embed::run`**, owner =
`UserRepo::first_admin_id` via `OperationsRepo::create_for_owner` (eccezione
senza AuthContext, motivata: job background senza utente HTTP; `list_running`
WS filtra per owner → l'admin vede il progresso). `set_total` =
`count_pending` a inizio finestra; `record_success_many` per lotto;
`finish_done` a drenaggio/pausa, `finish_cancelled` se `cancel_requested`.
Niente evento WS nuovo — `drain_operations` basta. `suggestions.changed`
non implementato (non in scope).

Task 12: complete (test `embed_window_opens_and_finishes_an_ai_analysis_operation`)

## Task 10 — SearchNode Tag / Category / Semantic

Ruling: **Semantic ORDER BY = (a)** — membership nei K più simili sotto
`VisibilityScope` (subquery riusa `$1,$2,$3`), risultati ancora
`taken_at_utc DESC, id DESC`. Non similarity-ordered.

Ruling: **Tag / Category filtrano solo `state='confirmed'`.** — Proposed
rimane coda di revisione.

Ruling: **Category** (nel piano, non in §4.1) = EXISTS su tag figli con
`parent_id` = categoria. — Costo se sbagliato: serve un secondo asse.

Ruling: **embedding testuale in API** (`prepare_semantic_embeddings`); db
riceve `Semantic.embedding` già pieno (`#[serde(skip)]`). `MODEL_VERSION`
duplicato in keeppix-db (no dipendenza media).

Task 10: complete (test tag/category/semantic search verdi)

Ruling: **`AlbumRepo::refresh` e `GeoRepo::clusters` passavano `None` come
`semantic_vis`** — un `SearchNode::Semantic` dentro una regola d'album
dinamico o un filtro mappa avrebbe scelto i K vicini su *tutti* gli
embedding del DB (subquery `TRUE`), non solo su quelli visibili al
chiamante — esattamente il bug che il commento in `SearchRepo::run` avvisa
di evitare («K fra i visibili, non K globali poi filtrati»). Fix: entrambi
ora costruiscono un secondo `VisibilityScope::filter` con alias `vf`/`va`
sugli stessi param ($1..$3 in `albums.rs`, $8..$10 in `geo.rs`, già
riservati al filtro esterno) e lo passano come `semantic_vis`. — Costo se
non fosse stato corretto: un album dinamico con regola `Semantic` avrebbe
potuto restare vuoto (o mostrare solo asset dell'owner) quando i K vicini
globali erano tutti fuori dal suo perimetro di visibilità.

Ruling: **aggiunto test end-to-end HTTP per `Semantic`**
(`keeppix-api/tests/search.rs::semantic_search_finds_the_asset_embedded_with_the_same_text`,
skip se il modello non è scaricato) e un test di coerenza
(`keeppix-api/tests/model_version.rs`) che verifica
`keeppix_db::MODEL_VERSION == keeppix_media::MODEL_VERSION` — la
duplicazione della costante (per il vincolo `deny.toml`) è un rischio di
drift silenzioso altrimenti invisibile a qualsiasi test esistente. — Costo
se sbagliato: nessuno finché le due costanti restano allineate a mano;
il test fallisce nel momento esatto in cui divergono.

## Task 11 — Indice vettoriale

MEASUREMENT (questo host, N=200k, K=50, cosine):
- **Linear** (pre-0045, full SearchRepo): ≈1234 ms
- **IVFFlat** `lists=200` + `ivfflat.probes=10`: **raw ORDER BY <=> ≈180–240 ms**
- SearchRepo Semantic completo resta ≈1.3–1.4 s (join heap 200k / stack
badge) — debito: partire dalla CTE top-K invece di filtrare la heap;
non è un motivo per HNSW.

Ruling: **spedire IVFFlat in 0045, non HNSW.** — Raw sotto 500 ms;
HNSW costa più RAM su Pi 8 GB. — Costo se sbagliato: recall IVFFlat
con probes bassi; alzare probes o passare a HNSW se il campo lo chiede.

Task 11: complete (migrazione 0045, test scale_embeddings verde)

## Task 13 — Documenti e debiti / chiusura fase

Ruling: **`get_json` già fuori dai rinvii** — commento in
`wired-exceptions.txt` (pagato Fase 6 `transcode` + Fase 7 `extra.ai`);
`check-wired.py` verde senza eccezione. Rotte `/tags*` restano rinvio
fase-11 (UI). — Costo se sbagliato: nessuno se la 11 non le consuma;
eccezioni da togliere quando arrivano i componenti Vue.

Ruling: **documenti di ripresa puntano a Fase 8** (CONTINUE, PROSEGUI,
superpowers/README, README feature row). Fase 7 chiusa sul branch
`fase-7`, non ancora su `main`. — Costo se sbagliato: un agente parte
sulla 7 già fatta; il ledger e CONTINUE lo correggono.

Debiti lasciati espliciti (non in scope Task 13):
- SearchRepo Semantic ~1.3–1.4 s @ 200k (CTE top-K, non HNSW)
- `suggestions.changed` WS non implementato
- ~369 MB ORT VmRSS residente dopo il primo load di finestra
- Tag suggest in `SearchRepo::suggest` ancora senza fonte tag ricca

Task 13: complete (docs + wired/deny verdi; gate full sotto)

## Verifica di chiusura Task 10 (sessione successiva)

Ripresa del lavoro trovando Task 10-13 già commitati su `fase-7` da un
altro agente (fad505f, 8452181, 4853406, 14dbf25, 7087e2e, f5399b6,
fdd2f87). Revisione manuale di `search.rs`/`geo.rs`/`albums.rs`/
`embeddings.rs`/`routes/search.rs` contro tutte le ruling sopra: nessuna
discrepanza — `Tag`/`Category` filtrano `state='confirmed'`,
`Semantic` richiede `embedding` 512-d riempito dall'API
(`prepare_semantic_embeddings`), il fix `semantic_vis` in
`albums.rs`/`geo.rs` è presente e corretto (K fra i visibili, non K
globali poi filtrati), `deny.toml` bans ok (`keeppix-db` non dipende da
`keeppix-media`).

Ruling: **`cargo clippy --workspace --all-targets -- -D warnings` era
rosso** su `crates/keeppix-db/tests/scale_embeddings.rs` (Task 11):
`doc_markdown` (IVFFlat senza backtick) e `too_many_lines` (136/100) —
il gate "full" menzionato in Task 13 non era mai stato eseguito così
com'è, o è regredito dopo. Fix meccanico, nessun cambio di
comportamento: backtick sul doc comment, estratto
`seed_scale_fixture` per il seeding di asset/embedding. Corretto anche
se fuori dal Task 10 nominale perché bloccava il gate di verifica che
questo stesso task deve superare (AGENTS.md, "Verifica prima di
dichiarare fatto"). — Costo se non corretto: `cargo clippy --workspace`
resta rosso per chiunque riprenda il branch.

Ruling: **due file lasciati non formattati da un commit precedente**
(`crates/keeppix-db/src/lib.rs` — ordine export `embeddings::*`;
`crates/keeppix-db/tests/scale_embeddings.rs` — wrap import) facevano
fallire `cargo fmt --check`. Commit di sola formattazione, nessun
cambio semantico.

Verifica eseguita e osservata (non solo dichiarata):
- `cargo fmt --check` → pulito.
- `cargo clippy -p keeppix-db -p keeppix-api --all-targets -- -D warnings` → pulito.
- `cargo clippy --workspace --all-targets -- -D warnings` → pulito (dopo il fix sopra).
- `cargo test -p keeppix-db --test search --test geo --test albums --test embeddings -- --test-threads=1`
  → 58 test verdi, incluso `semantic_search_selects_the_k_nearest_among_visible_assets_only`,
  `tag_filter_matches_only_confirmed_assignments`,
  `category_filter_matches_confirmed_child_tags`,
  `semantic_filters_top_k_then_orders_by_date`.
- `cargo test -p keeppix-api --test search --test model_version --test openapi -- --test-threads=1`
  → 15 test verdi. Il modello MobileCLIP2-S2 **è** presente in questo
  ambiente: `semantic_search_finds_the_asset_embedded_with_the_same_text`
  ha girato per davvero (non skip), round-trip HTTP completo testo→embedding→ricerca.
  `openapi_snapshot_matches_the_committed_file` conferma che `SearchNode`
  (schema `Object` generico sul campo `ast`) non ha fatto driftare lo
  snapshot, come previsto.
- `cargo deny check` → `advisories ok, bans ok, licenses ok, sources ok`.
- `python3 scripts/check-wired.py` → verde.
- `./scripts/test.sh` (suite completa, un crate alla volta): avviato, ma
  interrotto volontariamente durante la compilazione dei test di
  `keeppix-api` (nessun fallimento — solo lento: `--jobs 1` più i lint
  pedantic su decine di file di test, ~9 min solo per iniziare a
  eseguire il primo binario). Interrotto perché il disco condiviso
  della VM era già sceso da 54 GiB a 17 GiB liberi in quella finestra e
  un'altra sessione di questo stesso ambiente aveva appena esaurito lo
  spazio a 0 byte per build concorrenti di altri agenti (vedi nota
  sotto): continuare rischiava di ripetere quella crisi per chiunque
  altro condivida l'host. `cargo clean` post-interruzione ha liberato
  35 GiB. Resta verifica pendente per chi riprende: i controlli mirati
  sopra coprono comunque tutto il perimetro toccato dal Task 10 e
  l'intero workspace per fmt/clippy/deny/wired.

Nota ambientale (non un difetto del codice): la VM condivisa ha
raggiunto **load average >800** e **disco pieno (0 byte disponibili)**
per build concorrenti di altri agenti sullo stesso branch/host durante
questa sessione. `cargo clean` (57 GiB liberati) ha sbloccato la
verifica. Nessun impatto sul codice — solo sui tempi di questa sessione.

Task 10: verificato indipendentemente, nessuna regressione trovata.

## Chiusura Fase 7 — gate finale

Verifica osservata (HEAD `efd5287`):
- `cargo fmt --check` → pulito
- `cargo clippy --workspace --all-targets -- -D warnings` → CLIPPY_EXIT:0
- `cargo deny check` → advisories/bans/licenses/sources ok
- `python3 scripts/check-wired.py` → verde
- GitHub Actions run `32522791030` su `fase-7` @ `efd5287` → **success**
  (frontend, audit, api-clients, image, backend)

`./scripts/test.sh` locale in corso in parallelo; CI reale è il gate di
chiusura (AGENTS.md + piano Task 13).

Fase 7: chiusa sul branch `fase-7`. Prossima: Fase 8.

Ruling: **`./scripts/test.sh` locale verde** (TEST_EXIT:0, ~55 min wall,
cleanup `cargo clean` 47.5 GiB). Completa il gate AGENTS.md oltre la CI
già verde su `efd5287`. — Costo se sbagliato: nessuno; conferma indipendente.

Task 13 / Fase 7: chiusa (commit ledger + CI success su codice; test.sh locale ok).

## Task 14 (fuori roadmap, dopo Fase 8) — pagato il debito «CTE top-K»

Il debito dichiarato in Task 11 sopra (riga 450: *"SearchRepo Semantic
completo resta ≈1.3–1.4 s @ 200k — debito: partire dalla CTE top-K
invece di filtrare la heap"*) è rimasto aperto per due fasi (8 e parte
della 9-non-ancora-iniziata) finché non ha smesso di essere solo un
numero nel ledger: il 22 agosto 2026, sul commit di chiusura Fase 8
(`d5f3085` su `main`), `scale_embeddings.rs::vector_search_stays_interactive_with_ivfflat`
ha fallito in CI per la terza volta in giornata (due volte durante la
verifica di chiusura Fase 8 — vedi ledger Fase 8 — una volta su questo
commit). Non trattato come rumore una terza volta di fila: root-causato
per davvero, non solo rilanciato.

Ruling: **`SearchRepo::run` ora dirama fra `run_plain` (comportamento
invariato: `a.id = ANY(ARRAY(top-K))` come filtro `WHERE`) e
`run_semantic_hoisted` (nuovo)** quando `find_hoistable_semantic` trova
**esattamente un** nodo `Semantic` raggiungibile solo attraverso `And`
(mai `Or`/`Not`: lì forzare un `JOIN` cambierebbe il significato
booleano, non solo il piano). Nel path nuovo la CTE `topk` materializza
i ≤500 candidati IVFFlat e li **guida** nel join verso
`assets`/`folders`/`asset_exif` (Nested Loop su al più 500 lookup),
invece che farli filtrare come predicato su una scansione ordinata per
`taken_at_utc` (il piano che il planner sceglieva prima quando i
candidati sono radi in quell'ordine — il costo nascosto dietro
l'1,3–1,4s). `semantic_query_params` estrae la validazione
embedding/limit condivisa da entrambi i path (stesso errore, stesso
clamp di K, un solo posto). `substitute_with_true` clona l'AST
sostituendo (per identità di puntatore) il nodo issato con un `And`
vuoto, che compila già a `TRUE` — nessuna nuova variante di
`SearchNode` solo per questo marcatore. — Costo se sbagliato: un
`Semantic` dentro un `Or`/`Not` finisse comunque issato
produrrebbe risultati mancanti o in eccesso; mitigato da
`find_hoistable_semantic` che recursisce **solo** attraverso `And`, e
da `semantic_search_selects_the_k_nearest_among_visible_assets_only`/
`semantic_filters_top_k_then_orders_by_date` (verificati verdi dopo la
modifica) che coprono esattamente la semantica K-fra-i-visibili che un
bug qui romperebbe per primo.

Ruling: **`WITH topk AS MATERIALIZED (...)`, non un `WITH` semplice.**
— Prima misura del fix (senza `MATERIALIZED`): lo stesso SQL alternava
piani buoni (~170ms) e ricaduti (~2100ms) fra run identiche sullo
stesso fixture da 200k — da Postgres 12 il planner può inlineare una
CTE non referenziata più volte, ricreando esattamente il piano che
questa funzione esiste per evitare. `MATERIALIZED` è una barriera di
ottimizzazione esplicita, non un'euristica. — Costo se rimosso per
errore in futuro: la stessa intermittenza tornerebbe silenziosamente,
un piano buono su due circa.

Ruling: **`scale_embeddings.rs::seed_scale_fixture` ora chiama
`ANALYZE assets` oltre ad `ANALYZE asset_embeddings`.** — Anche con
`MATERIALIZED`, il fix da solo non bastava: `assets` passa da 0 a
200.000 righe in un solo `INSERT` senza mai essere analizzata, quindi
il planner sceglie il piano del join `topk`↔`assets` sulle statistiche
di prima dell'inserimento (o assenti). `scale_200k.rs` — stesso crate,
stessa tabella riempita allo stesso modo — chiama già `ANALYZE assets`
per lo stesso motivo da prima di questa sessione; mancava solo qui.
Verificato causale, non presunto: rimuovendo `MATERIALIZED` e tenendo
solo `ANALYZE assets` è rimasto stabile (5 run locali consecutive,
170–220ms); un'ipotesi cade prima di scriverla nel ledger come causa
unica. — Costo se rimosso: la stessa intermittenza da statistiche
stantie tornerebbe, non necessariamente riprodotta al primo tentativo
locale (dipende da timing di autovacuum).

Ruling: **budget di `elapsed_ms` in `scale_embeddings.rs` riportato da
2000ms a 800ms** — misurato dopo il fix, non scelto per far passare la
CI di oggi: 5 run locali consecutive, `elapsed_ms` 170–190ms contro
`raw_ms` 174–220ms (overhead di join a una cifra di millisecondi o
meno, spesso zero). 800ms lascia ~4× margine sul tipico locale ed è
comunque ampio anche se la sola scansione grezza toccasse il rumore di
CI più alto osservato finora (~650ms, unrelato a questo fix — vedi
budget separato `raw_elapsed < 500ms`, non toccato). Il vecchio 2000ms
non verificava più nulla di specifico da quando esisteva solo il filtro
post-hoc: qualunque piano, anche quello pessimo, restava sotto 2s salvo
rumore estremo. — Costo se il margine è comunque troppo stretto per un
runner CI particolarmente rumoroso: un rerun mirato via `rerun_failed_jobs`
con verifica `git diff` (protocollo già stabilito in questa sessione per
`budgets.rs`/`scale_200k.rs`) prima di considerare un nuovo aumento — mai
un aumento silenzioso senza rimisurare.

Verifica eseguita (locale, `KEEPPIX_TEST_DATABASE_URL` verso Postgres 16
con pgvector 0.6.0 installato, non testcontainers):
- `cargo check -p keeppix-db` → pulito.
- `cargo fmt --check -p keeppix-db` → pulito.
- `cargo clippy -p keeppix-db --all-targets -- -D warnings` → pulito.
- `cargo test -p keeppix-db --test search` → 28/28 verdi (inclusi i tre
  test `Semantic` esistenti e i tre `Person*` di Fase 8, nessuna
  regressione di visibilità).
- `cargo test -p keeppix-db --test albums --test geo` → 11+15 verdi
  (entrambi passano `Semantic` a `SearchRepo::run` dentro le regole
  dinamiche — path invariato, `semantic_vis` proprio riusato).
- `cargo test -p keeppix-db --test scale_embeddings` → verde 5 volte
  di fila dopo il fix (170–220ms), rosso 2 volte di fila prima (2050–2150ms
  con `MATERIALIZED` ma senza `ANALYZE assets`) — non una singola misura
  felice.
- `python3 scripts/check-wired.py` → verde.
- `keeppix-api`/`keeppix-jobs` non compilabili in locale in questa
  sessione (download dei binari `ort` bloccato dal proxy dell'ambiente,
  stesso limite già noto dalle fasi precedenti) — `SearchRepo::run` ha
  firma invariata (stessi parametri, stesso tipo di ritorno), quindi
  nessun sito chiamante in `keeppix-api` richiede modifiche; verifica
  reale affidata a CI su questo push.

Non ancora chiuso qui, verificato in CI dopo il push: CI reale verde
sul commit pushato (non solo locale) prima di considerare il debito
davvero pagato.
