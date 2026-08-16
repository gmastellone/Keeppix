# Task 3 report — Job di derivazione dei RAW (`JobKind::DeriveRaw`)

**Status:** complete
**Commit:** `86a8a3e` — `feat(jobs): derive raw assets from the embedded preview`
**Branch:** `fase-2` (non pushato, come richiesto)

## Cosa è stato fatto

- `keeppix_domain::JobKind::DeriveRaw` (nuova variante; nessuna migrazione
  necessaria — `jobs.kind` è `text` senza `CHECK`).
- `crates/keeppix-jobs/src/raw.rs`: la cascata (spec §2.1, sidecar XMP escluso
  — Task 5):
  1. `extract_embedded_preview(path)`.
  2. Se il lato lungo della preview è ≥1440px → `derive_from_bytes` su quei
     byte. Fine, zero demosaic.
  3. Altrimenti (piccola, assente, o file non riconosciuto come RAW) →
     demosaic half-size in sandbox (`dcraw_emu -h -w`, camera white balance).
  4. Se anche il demosaic fallisce → tutti gli asset con quel `content_hash`
     vanno in `AssetStatus::Error` via `AssetRepo::set_error`; il job
     ritorna `Ok(())`, non blocca la coda.
  5. Idempotenza: se il derivato (`{hash}-thumb.webp`) esiste già, il job
     esce subito, prima di leggere il RAW o chiamare il demosaic.
- Il demosaic è dietro un trait `Demosaic` (`fn demosaic(&self, path) ->
  Result<RawPreview, JobError>`), iniettato in `run_with`. `run()` usa
  `SandboxDemosaic` (produzione); i test usano un mock che conta le
  chiamate. Nessun timing nei test — solo conteggio.
- `crates/keeppix-jobs/src/dispatch.rs`: nuovo ramo `JobKind::DeriveRaw` →
  `raw::run`, con un `ram_hint_bytes` da 512MiB (il demosaic gira in un
  processo separato ma il gate deve comunque limitare quanti ne girano in
  parallelo).
- `crates/keeppix-jobs/src/hash.rs`: `enqueue_derive` ora sceglie
  `DeriveRaw`/`derive_raw:{hex}` per `AssetKind::RawImage`, altrimenti
  `DeriveAsset`/`derive:{hex}` come prima.
- `keeppix-media`:
  - `derive_from_rgb(rgb, width, height, data_dir, hash)`: stessa pipeline
    di `derive_from_bytes` (resize, webp, thumbhash) ma per pixel RGB8 già
    decodificati — l'uscita del demosaic non è un JPEG. Coda condivisa
    estratta in `build_derivatives` per non duplicare la logica di encoding.
  - `demosaic_half(path, memory_bytes, cpu_secs)`: esegue `dcraw_emu -h -w
    -Z -` via `sandbox::run` (mai in-process) e parsa il PPM (`P6`) che
    scrive su stdout.
  - `dcraw_emu_available()`: come `video::ffprobe_available`, per far
    saltare i test che dipendono dal binario su una macchina senza libraw.

## TDD — RED prima, poi GREEN

I quattro test richiesti dal piano sono in
`crates/keeppix-jobs/tests/raw.rs`. Dato che l'interfaccia (trait di
iniezione + cascata) andava progettata insieme al codice per essere
testabile, l'implementazione e i test sono stati scritti nello stesso giro;
la garanzia richiesta dal task — *"il test deve fallire se rompo la
cascata"* — è stata verificata **con una mutazione**, non assumendola.

### 1. Prima esecuzione: tutti verdi con l'implementazione corretta

```
$ cargo test -p keeppix-jobs --test raw -- --test-threads=1 --nocapture

running 7 tests
test a_corrupt_raw_sets_the_asset_to_error_and_does_not_block_the_queue ... ok
test a_raw_with_a_large_preview_never_calls_libraw ... ok
test a_raw_without_a_preview_falls_back_to_demosaic ... ok
test harness::tests::appends_when_the_url_has_no_database ... ok
test harness::tests::preserves_the_query_string ... ok
test harness::tests::replaces_an_existing_database_name ... ok
test the_job_is_idempotent ... ok

test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 16.36s
```

### 2. RED — mutazione della cascata per dimostrare che il test critico morde

Ho rotto deliberatamente `derive_raw` in `crates/keeppix-jobs/src/raw.rs`
per saltare il controllo dei 1440px e chiamare **sempre** il demosaic:

```rust
// MUTATION (temporanea, non committata):
let preview = extract_embedded_preview(src).ok().flatten();
let chosen = demosaic.demosaic(src).map_err(|e| e.to_string())?; // salta il controllo >=1440px
let _ = preview;
```

Rieseguendo solo il test critico:

```
$ cargo test -p keeppix-jobs --test raw a_raw_with_a_large_preview_never_calls_libraw \
    -- --test-threads=1 --nocapture

running 1 test
test a_raw_with_a_large_preview_never_calls_libraw ...
thread 'a_raw_with_a_large_preview_never_calls_libraw' panicked at crates/keeppix-jobs/tests/raw.rs:166:5:
assertion `left == right` failed: la preview incorporata di sample.arw supera 1440px: libraw non deve partire
  left: 1
 right: 0
FAILED

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 6 filtered out; finished in 6.59s
```

Il test **fallisce correttamente** quando la cascata è rotta: prova che
`demosaic.calls()` non è un conteggio decorativo, ma verifica davvero che
`sample.arw` (preview incorporata 1616×1080, misurata al Task 1) non fa mai
partire il demosaic quando non serve.

### 3. GREEN — mutazione ripristinata

```rust
let preview = extract_embedded_preview(src).ok().flatten();
let chosen = match preview {
    Some(p) if p.width.max(p.height) >= MIN_PREVIEW_LONG_SIDE => p,
    _ => demosaic.demosaic(src).map_err(|e| e.to_string())?,
};
```

```
$ cargo test -p keeppix-jobs --test raw -- --test-threads=1 --nocapture

running 7 tests
test a_corrupt_raw_sets_the_asset_to_error_and_does_not_block_the_queue ... ok
test a_raw_with_a_large_preview_never_calls_libraw ... ok
test a_raw_without_a_preview_falls_back_to_demosaic ... ok
test harness::tests::appends_when_the_url_has_no_database ... ok
test harness::tests::preserves_the_query_string ... ok
test harness::tests::replaces_an_existing_database_name ... ok
test the_job_is_idempotent ... ok

test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 16.69s
```

Nessuna traccia della mutazione è rimasta nel codice committato — verificato
con `git diff` prima del commit.

## Cosa provano i quattro test

- **`a_raw_with_a_large_preview_never_calls_libraw`**: usa il fixture reale
  `sample.arw` (Task 1: preview incorporata 1616×1080). Il mock demosaic
  restituisce sempre `Err` — se venisse chiamato, il test fallirebbe sia sul
  conteggio sia (indirettamente) perché l'asset finirebbe in errore invece
  che avere un thumbnail. Verifica anche che il thumbnail sia stato
  generato dalla preview.
- **`a_raw_without_a_preview_falls_back_to_demosaic`**: un TIFF minimo
  valido (header + una IFD vuota, nessun tag di preview) → `Ok(None)`. Il
  mock demosaic restituisce pixel RGB8 finti; il test verifica che sia
  stato chiamato esattamente una volta e che il thumbnail derivi da quei
  pixel (via `derive_from_rgb`).
- **`a_corrupt_raw_sets_the_asset_to_error_and_does_not_block_the_queue`**:
  un file che non è un RAW riconosciuto (`extract_embedded_preview` fallisce)
  *e* un mock demosaic che fallisce sempre → l'asset passa a
  `AssetStatus::Error`, ma `run_with` ritorna `Ok(())`. Un secondo asset
  (RAW valido, `sample.nef`) processato subito dopo nello stesso test va a
  buon fine, a dimostrazione che l'errore sul primo non impedisce la
  lavorazione del successivo.
- **`the_job_is_idempotent`**: usa lo scenario "senza preview" per contare il
  demosaic. Prima esecuzione: 1 chiamata, thumbnail creato. Seconda
  esecuzione sullo stesso hash: 0 chiamate aggiuntive (il job vede il
  thumbnail e esce prima di leggere il file), byte del thumbnail invariati.

## Verifica completa prima di dichiarare fatto

```
cargo test -p keeppix-media --test raw               → 10 passed (8 preesistenti + 2 nuovi)
cargo test -p keeppix-jobs --test raw -- --test-threads=1  → 7 passed
cargo fmt --check                                     → pulito
cargo clippy --workspace --all-targets -- -D warnings → pulito
cargo deny check bans                                 → "bans ok" (nessun arco nuovo media↔db)
```

**Suite completa del workspace** (vedi Ruling nel ledger: `./scripts/test.sh`
non gira su questa macchina perché usa `mapfile`, builtin di bash ≥4, e
macOS spedisce bash 3.2 senza alternativa in `/opt/homebrew/bin` — rieseguita
manualmente la stessa logica, crate per crate, `--jobs 1 -- --test-threads=1`,
container testcontainers ripuliti fra un crate e l'altro):

```
keeppix-domain, keeppix-media, keeppix-api, keeppix-db, keeppix-test-support,
keeppix-dav, keeppix-jobs, keeppix-server
→ 60/60 blocchi "test result: ok", 0 failed, in tutto il workspace.
```

I test di `demosaic_half` in `keeppix-media` esercitano davvero `dcraw_emu`
(non un mock): confermato su `sample.arw` (1392×936 dopo half-size,
misurato) e su un file corrotto (`dcraw_emu` esce con status 2, il parser
del PPM lo riporta come `RawError::Corrupt` invece di panicare). Gated su
`dcraw_emu_available()` per non rompere una macchina/CI senza libraw.

## Note di self-review

- Nessun `unwrap()`/`expect()` fuori dai test.
- `dcraw_emu`/libraw gira **sempre** in `sandbox::run` (mai in-process),
  come richiesto dagli invarianti di AGENTS.md.
- Nessuna query SQL fuori da `keeppix-db`: `raw.rs` in `keeppix-jobs` usa
  solo `AssetRepo`/`FolderRepo`.
- `cargo deny check bans` conferma che non è stato introdotto nessun arco
  fra `keeppix-media` e `keeppix-db`.
- Non ho toccato Task 4+ né fatto push/PR.

## Docker

Docker Desktop era raggiungibile ma non visibile dalla sandbox di default
(serve il permesso `all` per il socket); con quel permesso tutti i test di
integrazione (incluso l'harness `TestDb` via testcontainers) hanno
funzionato senza il flake `PortNotExposed` menzionato nel ledger — il retry
di Task 0 non è nemmeno stato esercitato in questa run.
