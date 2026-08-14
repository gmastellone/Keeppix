# Keeppix Fase 1b — Pipeline di ingestione

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Puntare Keeppix a una cartella reale e trovarla indicizzata, con miniature su disco, senza aver toccato un originale.

**Architecture:** Coda job in Postgres (`SKIP LOCKED`). `keeppix-media` è puro (`path → dati`): nessun database. `keeppix-jobs` unisce i due. Quattro tipi di job (discovery, metadati, hash, derivati), non un job monolitico. I decoder C girano in un processo usa-e-getta.

**Tech Stack:** Rust 1.88 (edition 2024) · sqlx 0.8 · PostgreSQL 17 + PostGIS · blake3 · zune-jpeg · fast_image_resize · walkdir · notify · testcontainers

**Spec:** [`../specs/fase-1b-ingestione.md`](../specs/fase-1b-ingestione.md) — vince sul piano
**Design:** [`../specs/2026-08-13-keeppix-design.md`](../specs/2026-08-13-keeppix-design.md)
**Roadmap:** [`2026-08-13-keeppix-roadmap.md`](2026-08-13-keeppix-roadmap.md)
**Ledger 1a:** `.superpowers/sdd/2026-08-14-keeppix-fase-1a/progress.md`

## Numeri ereditati dalla 1a

- ~189 esecuzioni Rust, keeppix-db ~6–7 min, ~4 s/test (boot container).
- Ruling 1a Task 8: **cambiare l'harness in 1b** (un container per processo).
- `AssetRepo` già ha `upsert_discovered`, `set_hash`, `set_indexed`, `set_error`, `mark_offline`.
- `Indexed` si imposta **dopo i metadati** (così la timeline `assets_timeline_idx` esiste prima dei derivati). I derivati non cambiano lo status. Divergenza col commento di dominio «metadati e derivati»: vince lo spec 1b §1.

## Global Constraints

Valgono per **ogni** task. Come 1a, più:

- **`keeppix-media` non dipende da `keeppix-db`.** `cargo deny check bans` deve restare verde; `keeppix-jobs` va aggiunto ai `wrappers` di `keeppix-db` quando l'arco nasce.
- **SQL solo in `keeppix-db`.** I job handler chiamano i repository.
- **Scanner / worker senza `AuthContext`**, documentato nel doc comment (stesso precedente di `ensure_*` / `mark_scanned`).
- **Nessun percorso dal client.** Il walker legge `libraries.root_path` dal database.
- **Decoder C (ffmpeg, libraw) in processo separato** con rlimit. JPEG/WebP/PNG restano in-process (Rust).
- **File RAW non si riscrivono.** In 1b si riconoscono e se ne leggono gli EXIF; preview incorporata = Fase 2.
- **Niente HTTP, WebSocket, TimelineRepo, frontend, ricerca.** Sono 1c.
- TDD: test che fallisce, osservato, poi il minimo. Clippy `-D warnings`, `fmt --check`, `frontend && npm ci && npm run build`, `cargo test --workspace -- --test-threads=1` dopo ogni task.
- Commit convenzionali in inglese. Ledger in `.superpowers/sdd/2026-08-14-keeppix-fase-1b/progress.md`.
- Branch `fase-1`. No merge su `main`. Push ok se l'utente l'ha chiesto.

## Cosa NON è in 1b

Endpoint, WS, timeline UI, ricerca, preview RAW, sidecar XMP, transcodifica HLS (on-demand al play = si predispone il probe, non si transcodifica all'ingest).

---

## Task 1: Harness — un container per processo

Il boot Postgres è ~4 s/test. Un binario con 14 test paga un minuto di Docker. `TestDb` tiene il container in uno `OnceCell` statico; ogni test crea un **database** nuovo (stesso isolamento di `KEEPPIX_TEST_DATABASE_URL`).

**Files:**
- Modify: `crates/keeppix-db/tests/harness/mod.rs`
- Modify: `crates/keeppix-api/tests/harness/mod.rs` (stessa logica, tenuta allineata a mano come oggi)

**Interfaces:**
- `TestDb::start()` / `TestServer::start()` invariati per i chiamanti.

- [ ] **Step 1: Test che due `TestDb` non condividono le righe**

In `crates/keeppix-db/tests/harness/mod.rs` sotto `mod tests`, oltre ai tre già presenti:

```rust
#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn two_starts_do_not_share_rows() {
    let a = crate::harness::TestDb::start().await;
    keeppix_db::UserRepo::new(a.db())
        .create_bootstrap_admin(/* … stesso NewUser di seed_admin … */)
        .await
        .expect("admin in A");
    let n_a: i64 = sqlx::query_scalar("SELECT count(*) FROM users")
        .fetch_one(a.db().pool())
        .await
        .unwrap();
    assert_eq!(n_a, 1);

    let b = crate::harness::TestDb::start().await;
    let n_b: i64 = sqlx::query_scalar("SELECT count(*) FROM users")
        .fetch_one(b.db().pool())
        .await
        .unwrap();
    assert_eq!(n_b, 0, "B deve essere un database vergine");
}
```

Il test vive in `tests/users.rs` (il modulo `harness` è incluso da ogni binario; un `#[tokio::test]` in `harness/mod.rs` girerebbe N volte). Metterlo in `tests/users.rs` come `two_test_databases_are_isolated`.

- [ ] **Step 2: Eseguire, deve già passare** (isolamento c'è). Poi cambiare `provision` e **rieseguire** la suite db: deve restare verde e più veloce.

- [ ] **Step 3: Implementare**

`provision` (entrambi gli harness):

```rust
static SHARED: tokio::sync::OnceCell<(ContainerAsync<Postgres>, String)> =
    tokio::sync::OnceCell::const_new();

async fn provision() -> (Option<ContainerAsync<Postgres>>, String) {
    if let Ok(server_url) = std::env::var("KEEPPIX_TEST_DATABASE_URL") {
        return named_clone(&server_url).await;
    }
    let (_container, admin_url) = SHARED
        .get_or_init(|| async {
            let container = Postgres::default()
                .with_tag("17-3.5")
                .with_name("postgis/postgis")
                .start()
                .await
                .expect("avvio del container Postgres");
            let port = container
                .get_host_port_ipv4(5432)
                .await
                .expect("porta mappata");
            let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
            (container, url)
        })
        .await;
    named_clone(admin_url).await
}

async fn named_clone(server_url: &str) -> (Option<ContainerAsync<Postgres>>, String) {
    let name = format!("keeppix_test_{}", uuid::Uuid::now_v7().simple());
    let mut admin = PgConnection::connect(server_url)
        .await
        .expect("connessione al server Postgres");
    sqlx::query(&format!("CREATE DATABASE \"{name}\""))
        .execute(&mut admin)
        .await
        .expect("creazione del database di test");
    admin.close().await.ok();
    (None, with_database(server_url, &name))
}
```

`TestDb` non tiene più `_container` quando il container è statico (`None`). Il processo tiene vivo il `OnceCell`.

- [ ] **Step 4:** `cargo test -p keeppix-db -- --test-threads=1` verde. Confrontare il tempo con i ~6–7 min della 1a. Registrare nel ledger.

- [ ] **Step 5: Commit** `perf(test): share one Postgres container per test process`

---

## Task 2: Migrazione `jobs` + tipi di dominio

**Files:**
- Create: `crates/keeppix-db/migrations/0007_jobs.sql`
- Create: `crates/keeppix-db/tests/schema_0007.rs`
- Create: `crates/keeppix-domain/src/job.rs`
- Modify: `crates/keeppix-domain/src/lib.rs`, `crates/keeppix-db/src/lib.rs`

**Interfaces:**
- Produces: tabella `jobs`; `JobKind`, `JobStatus`, `JobPriority`, `Job`.

- [ ] **Step 1: Test di schema che falliscono** (tabella assente), poi migrazione.

```sql
CREATE TABLE jobs (
    id           bigserial   PRIMARY KEY,
    kind         text        NOT NULL,
    payload      jsonb       NOT NULL,
    priority     smallint    NOT NULL DEFAULT 3
                             CHECK (priority BETWEEN 0 AND 3),
    status       text        NOT NULL DEFAULT 'pending'
                             CHECK (status IN ('pending','running','done','failed')),
    attempts     int         NOT NULL DEFAULT 0,
    max_attempts int         NOT NULL DEFAULT 3,
    last_error   text,
    run_after    timestamptz NOT NULL DEFAULT now(),
    locked_by    uuid,
    locked_at    timestamptz,
    created_at   timestamptz NOT NULL DEFAULT now(),
    dedup_key    text
);

CREATE INDEX jobs_claim_idx ON jobs (priority, run_after, id)
    WHERE status = 'pending';
CREATE UNIQUE INDEX jobs_dedup_key ON jobs (dedup_key)
    WHERE dedup_key IS NOT NULL AND status IN ('pending', 'running');
CREATE INDEX jobs_stale_idx ON jobs (locked_at) WHERE status = 'running';
```

Test `schema_0007.rs`: unique parziale su `dedup_key` (due `pending` con la stessa chiave falliscono; `done` + `pending` no); CHECK su `status`; indice `jobs_claim_idx` esiste.

- [ ] **Step 2: Tipi di dominio**

```rust
pub enum JobKind {
    DiscoverLibrary,
    ExtractMetadata,
    HashAsset,
    DeriveAsset,
    ReapStale,
}

pub enum JobStatus { Pending, Running, Done, Failed }

#[repr(i16)]
pub enum JobPriority { Interactive = 0, High = 1, Visible = 2, Background = 3 }

pub struct Job {
    pub id: i64,
    pub kind: JobKind,
    pub payload: serde_json::Value,
    pub priority: JobPriority,
    pub status: JobStatus,
    pub attempts: i32,
    pub max_attempts: i32,
    pub last_error: Option<String>,
    pub run_after: DateTime<Utc>,
    pub locked_by: Option<uuid::Uuid>,
    pub dedup_key: Option<String>,
}
```

`JobKind` serializza in snake_case string (`discover_library`, …). Test unitari sul parse.

- [ ] **Step 3: Commit** `feat(db): add the jobs table`

Tocca `lib.rs` perché `sqlx::migrate!` veda `0007`.

---

## Task 3: `JobRepo` — enqueue, claim, complete, fail

**Files:**
- Create: `crates/keeppix-db/src/jobs.rs`, `crates/keeppix-db/tests/jobs.rs`
- Modify: `deny.toml` — aggiungere `keeppix-jobs` ai wrappers **solo quando** il crate jobs dipenderà da db (Task 5). In questo task il repo sta in `keeppix-db`, nessun arco nuovo.

**Interfaces:**
- `JobRepo::enqueue(&self, kind, payload, priority, dedup_key) -> Result<Job, DbError>` — `ON CONFLICT DO NOTHING` sulla chiave parziale, poi rilettura (come `ensure_child`).
- `JobRepo::claim(&self, worker_id: Uuid, max_priority: JobPriority) -> Result<Option<Job>, DbError>` — SQL dello spec §2.3.
- `JobRepo::complete(&self, id) -> Result<(), DbError>`
- `JobRepo::fail(&self, id, error: &str) -> Result<(), DbError>` — se `attempts >= max_attempts` → `failed`, altrimenti `pending` con `run_after = now() + min(2^attempts, 300)s + jitter`.
- `JobRepo::reap_stale(&self, older_than: Duration) -> Result<u64, DbError>` — `running` con `locked_at` più vecchio → `pending`.
- `JobRepo::promote(&self, dedup_keys: &[String], priority: JobPriority) -> Result<u64, DbError>` — spec §2.5 livello 2.
- Nessun `AuthContext`: la chiama il worker.

Claim (testo esatto dello spec):

```sql
UPDATE jobs SET
    status = 'running',
    locked_by = $worker_id,
    locked_at = now(),
    attempts = attempts + 1
WHERE id = (
    SELECT id FROM jobs
     WHERE status = 'pending'
       AND run_after <= now()
       AND priority <= $max_priority
     ORDER BY priority, run_after, id
     FOR UPDATE SKIP LOCKED
     LIMIT 1
)
RETURNING *
```

Test in `jobs.rs`:
- `enqueue_is_idempotent_on_dedup_key`
- `claim_skips_a_locked_row` (`tokio::join!` di due claim: due job distinti, o uno None)
- `claim_respects_max_priority` (priority 3 non esce se max=2)
- `claim_orders_by_priority_then_run_after`
- `fail_retries_then_exhausts`
- `reap_stale_returns_a_running_job_to_pending`
- `promote_raises_pending_jobs`

- [ ] **Step: Commit** `feat(db): add job repository with skip-locked claim`

---

## Task 4: `keeppix-media` — tipo per contenuto

**Files:**
- Modify: `crates/keeppix-media/Cargo.toml`, `src/lib.rs`
- Create: `crates/keeppix-media/src/kind.rs`, `crates/keeppix-media/tests/kind.rs`

**Interfaces:**
- `pub fn detect_kind(header: &[u8]) -> AssetKind` — magic number, non estensione.
- JPEG `ff d8 ff`, PNG `89 50 4e 47`, WebP `RIFF....WEBP`, GIF `GIF8`, TIFF `II*`/`MM*`, ftyp heic/avif, ISOBMFF video, EBML mkv/webm, RAW Sony `II*\0` + maker, etc. I test pinnano almeno JPEG, PNG, un RAW Sony (header minimo), un MP4 `ftyp`, e un file `.jpg` il cui contenuto è testo → `Unknown`.

Nessuna dipendenza da db. `AssetKind` da `keeppix-domain`.

- [ ] **Commit** `feat(media): detect asset kind from magic bytes`

---

## Task 5: Worker pool + profili energetici

**Files:**
- Modify: `crates/keeppix-jobs/Cargo.toml` (dipende da `keeppix-db`, `keeppix-media`, `keeppix-domain`, tokio, tracing, uuid, chrono, serde_json)
- Modify: `deny.toml` wrappers += `keeppix-jobs`
- Create: `crates/keeppix-jobs/src/{lib,error,pool,profile,dispatch}.rs`
- Create: `crates/keeppix-jobs/tests/profile.rs`

**Interfaces:**
- `EnergyProfile::{Interactive, Background, Night, Paused}`
- `fn max_priority(profile) -> Option<JobPriority>` — `Paused` → solo 0; `Interactive` → 0-2; `Background`/`Night` → 0-3.
- `fn worker_count(cpu: usize) -> usize` = `clamp(cpu-1, 1, 8)`
- `ActivityTracker::notify_authenticated_request()` / `current_profile(now, night_window) -> EnergyProfile`
- «Interfaccia in uso»: richiesta autenticata negli ultimi 5 min, esclusi `/health` e statici. Il tracker è un `Arc<AtomicI64>` di unix-ts; l'API lo toccherà in 1c, i test lo chiamano diretto.
- `WorkerPool::run` loop: `claim` + `dispatch`. Dispatch nel Task 6+. Qui il pool chiama un `JobHandler` trait per essere testabile.
- Semaforo pesato: ogni job dichiara `ram_hint_bytes` (per un'immagine `width * height * 3` se noti, altrimenti 64 MiB). Un job che stima più della RAM del processo aspetta. Test: due job da 3/4 della capienza non girano in parallelo.

Test:
- `worker_count_leaves_one_core_for_http`
- `paused_accepts_only_interactive`
- `interactive_excludes_background_priority`
- `activity_within_five_minutes_is_interactive`
- `five_minutes_idle_becomes_background`
- `night_window_yields_night_unless_interactive`

- [ ] **Commit** `feat(jobs): add worker pool sizing and energy profiles`

---

## Task 6: Discovery (walker)

**Files:**
- Create: `crates/keeppix-media/src/walk.rs`
- Create: `crates/keeppix-jobs/src/discover.rs`
- Create: `crates/keeppix-jobs/tests/discover.rs`
- Modify: `AssetRepo` se serve `upsert_discovered` in batch (oggi è riga per riga: 1000 `INSERT` vanno bene per 1b; `COPY` è un'ottimizzazione da misurare e, se serve, un metodo `insert_discovered_batch`. Ruling: riga per riga finché i numeri non lo smentiscono.)

**Interfaces:**
- `keeppix_media::walk::iter_entries(root, exclude_globs) -> impl Iterator<Item = WalkedFile>`
- Esclusioni fisse: `@eaDir`, `.DS_Store`, `Thumbs.db`, `#recycle`, `#snapshot`, `.keeppix-trash/`, `.keeppix-tmp/`, nomi che iniziano per `.` o `._`. Più `exclude_patterns` della libreria.
- `.xmp` non diventano asset.
- Stabilità: due `stat` a 5 s. Nei test, iniettare l'attesa (`Duration::ZERO` + due size diverse → rimanda).
- Job `discover_library` payload `{ "library_id": "…" }`: `ensure_path` per ogni cartella relativa, `upsert_discovered`, enqueue `extract_metadata` con `dedup_key = format!("meta:{asset_id}")`, priority 3.
- Disco assente o root vuota mentre `count_by_status` della libreria > 0: `LibraryRepo::set_status(Offline)`, **nessun** `mark_offline` di massa. Test dedicato.
- Sparizione di massa > 20%: il job si ferma con errore `mass_disappearance`, non marca offline. Test dedicato.

Test su una tempdir con JPEG minimo, `@eaDir`, `.xmp`, un file che «cresce».

- [ ] **Commit** `feat(jobs): discover a library tree without opening files`

---

## Task 7: Metadati rapidi (EXIF)

**Files:**
- Create: `crates/keeppix-media/src/exif.rs`
- Create: `crates/keeppix-jobs/src/metadata.rs`
- Modify: `crates/keeppix-db/src/assets.rs` — `insert_exif(&self, asset_id, ExifRow)` (immutabile: `ON CONFLICT DO NOTHING`)
- Create: domain `ExifData` se non c'è

**Interfaces:**
- `keeppix_media::read_exif(path) -> Result<ExifData>` — apre solo i primi 128 KB (`File::open` + `take(128*1024)`).
- Ordine data: `DateTimeOriginal` → `CreateDate` → `mtime`. Mai birthtime.
- Senza GPS: `tz_offset_minutes` = offset del server, e un flag `tz_assumed: true` nel jsonb `raw`.
- Job: `set_indexed(taken_at, width, height)`, `insert_exif`, enqueue `hash_asset` `dedup_key = hash:{id}`.

Test con un JPEG che contiene EXIF DateTimeOriginal (fixture bytes nel test). Un file senza EXIF usa mtime.

- [ ] **Commit** `feat(media): extract exif from the first 128 kilobytes`

---

## Task 8: Hash blake3

**Files:**
- Create: `crates/keeppix-media/src/hash.rs`
- Create: `crates/keeppix-jobs/src/hash.rs`

**Interfaces:**
- `keeppix_media::hash_file(path) -> [u8; 32]` streaming blake3.
- Job: se `(size, mtime, inode)` combaciano **e** `content_hash` è già Some, non ricalcolare. Altrimenti `set_hash` e enqueue `derive_asset` con `dedup_key = derive:{hex hash}` (derivati per contenuto, non per asset: cinque copie, un thumbnail).

Test: stesso bytes → stesso hash; file invariato non riaccoda derive (dedup_key).

- [ ] **Commit** `feat(media): hash files with blake3`

---

## Task 9: Derivati (una decodifica, rename atomico)

**Files:**
- Create: `crates/keeppix-media/src/derive.rs`
- Create: `crates/keeppix-jobs/src/derive.rs`
- Modify: `0008_asset_thumbhash.sql` — `ALTER TABLE assets ADD COLUMN thumbhash bytea`
- Modify: `AssetRepo::set_thumbhash`

**Interfaces:**
- Percorso: `{data_dir}/derivatives/{ab}/{cd}/{hex}.webp` per thumb (`-thumb.webp`) e preview (`-preview.webp`). Sharding sui primi due byte dell'hash.
- Si scrive in `{path}.tmp` e `rename()`.
- Una decodifica → thumbnail 240, preview 1440, thumbhash 25 byte.
- Ottimizzazione 4: originale ≤1600 px e ≤400 KB → niente preview, si serve l'originale (il job scrive solo thumb + thumbhash).
- Ottimizzazione 1: JPEG con thumbnail EXIF ≥240 → usarlo per la thumb.
- Buffer RGB8. `fast_image_resize` Lanczos3. WebP q78.
- Job idempotente: se i file ci sono già, skip.

Test: JPEG piccolo in tempdir; dopo il job esistono i file; un secondo run non cambia mtime del derivato; l'originale è bit-identico.

- [ ] **Commit** `feat(media): build thumbnail, preview and thumbhash from one decode`

Le ottimizzazioni 2 (DCT scale), 5 (`sharp_yuv`), 6 (SSIM), 8 (zune-jpeg vs `image`) si applicano **in questo task** se le crate le rendono a costo zero; altrimenti si pinna un test «non usiamo `image::load`» e si usa `zune-jpeg`. Registrare nel ledger cosa è in e cosa slitta a un follow-up **nella 1b** prima della chiusura, non alla 1c.

---

## Task 10: Sandbox ffmpeg + poster video

**Files:**
- Create: `crates/keeppix-media/src/sandbox.rs`, `video.rs`

**Interfaces:**
- `sandbox::run(cmd, memory_bytes, cpu_secs) -> Output` — processo figlio, `rlimit`, niente rete. Su macOS (darwin) seccomp non esiste: rlimit + `std::process`. Su Linux, seccomp se disponibile. Test: un figlio che prova a scrivere fuori dal tmp viene… in 1b il test pinnato è «il figlio è un altro PID e un panic nel figlio non uccide il padre».
- Video: `ffprobe` via sandbox → durata, codec, rotazione. Poster al 10% della durata. Nessuna transcodifica all'ingest.
- Immagini > 200 MP: rifiuto prima di decodificare.

Se `ffprobe` non è nel PATH, i test video si `#[ignore]` con messaggio; il job marca `error` con dettaglio chiaro. Il test di integrazione di chiusura usa JPEG.

- [ ] **Commit** `feat(media): run ffmpeg in a child process and extract a poster`

---

## Task 11: Watcher, spostamenti, probe hardware

**Files:**
- Create: `crates/keeppix-jobs/src/watch.rs`, `r#move.rs`
- Create: `crates/keeppix-media/src/probe.rs`
- `system_settings` chiave `capabilities` (jsonb). Lo spec 1b cita `system_capabilities` «già in Fase 0»: **non esiste**; si usa `system_settings` (jsonb nato per questo). Non si aggiunge una tabella.

**Watcher:** `notify` debounce 2 s. All'avvio legge `fs.inotify.max_user_watches` (Linux); se manca, log + flag `watcher_mode = Polling { every: 15 min }`. NFS/SMB: polling. Test: tempdir, scrivere un JPEG, dopo il debounce esiste un asset.

**Spostamento:** cancellazione + creazione con stesso `(content_hash, size)` entro N secondi → non è un nuovo asset: si `upsert` sulla nuova cartella trasferendo l'id… No: identità = `(folder_id, filename)`. Lo spostamento crea un nuovo asset e marca il vecchio? Spec 1a: cancellazioni indipendenti. Spec 1b §7: «trasferisce metadati, rating e album». In 1b rating/album non esistono. Si trasferiscono `content_hash`, EXIF, thumbhash, e si marca il vecchio `offline` (il file non è più lì). Log: `rilevato spostamento`. Test su due path.

**Probe:** prova i backend in ordine; in CI resta `software`. Scrive fps in `system_settings`. Test: `probe_returns_at_least_software`.

- [ ] **Commit** `feat(jobs): watch the library tree and detect moves`

---

## Task 12: Integrazione — cartella di esempio + STATO

**Files:**
- Create: `crates/keeppix-jobs/tests/ingest_fixture.rs`
- Create: `crates/keeppix-jobs/tests/fixtures/tiny.jpg` (JPEG 64×64 con EXIF, committato)
- Modify: `crates/keeppix-server/src/main.rs` — avvia `WorkerPool` accanto ad axum (stesso tokio).
- Create: `docs/superpowers/plans/2026-08-14-keeppix-fase-1b-STATO.md`
- Modify: spec 1b stato → chiusa.

Il test: tempdir con 3 JPEG (uno «WhatsApp-size»), una sottocartella, un `.DS_Store`. Si crea la libreria, si enqueue `discover_library`, si gira il pool fino a coda vuota (timeout 60 s). Assert: 3 asset `indexed`, 3 thumbhash, file in `derivatives/`, originali invariati (mtime), `.DS_Store` assente.

Misurare sul fixture (non sul TB): ms/file metadati, ms/file derive. Scrivere i numeri nel STATO come «numeri del fixture, non del TB».

- [ ] **Commit** `feat(jobs): ingest a fixture directory end to end`
- [ ] **Commit** `docs: record Fase 1b handoff numbers`

---

## Criteri di completamento

- [ ] Workspace verde, clippy `-D warnings`, fmt pulito, frontend build.
- [ ] Fixture: 3 foto indicizzate, thumb su disco, originali intatti.
- [ ] Disco assente → libreria `offline`, zero cancellazioni.
- [ ] Coda: claim `SKIP LOCKED`, dedup, retry con jitter, stale reap.
- [ ] `keeppix-media` senza `keeppix-db` (`cargo deny check bans`).
- [ ] STATO.md con i numeri del fixture e i ruling.
- [ ] Niente 1c.

## Checkpoint harness

Dopo il Task 1, se il tempo di `keeppix-db` non è sceso in modo visibile, non si prosegue a scrivere 30 test sullo schema vecchio: si ferma e si sistema. Se è sceso, si va avanti.
