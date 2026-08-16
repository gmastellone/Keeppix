# Keeppix Fase 2 — RAW, metadati e culling

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rendere utilizzabili gli 11.000 RAW: vederli a risoluzione piena senza demosaic, votarli da tastiera, correggerne i metadati in blocco in modo istantaneo e reversibile, e cancellare gli scarti con una scelta esplicita su cosa succede al file.

**Architecture:** L'estrazione della preview incorporata nei RAW riusa la pipeline dei derivati già esistente: `derive_jpeg` non cambia, riceve un buffer JPEG invece di un percorso. Le modifiche dell'utente vivono in `asset_overrides` accanto a `asset_exif`, che resta immutabile; il valore mostrato è `COALESCE(override, exif)`. I sidecar `.xmp` sono scritti da un job asincrono, mai in linea con la richiesta.

**Tech Stack:** Rust 1.88 (edition 2024) · `rawler` o estrazione manuale dei box TIFF/ISO-BMFF · `libraw` in processo sandbox come fallback · `quick-xml` per XMP · Vue 3 + Tailwind per il culling

**Spec:** [`../specs/fase-2-raw-culling.md`](../specs/fase-2-raw-culling.md) — **leggerla prima**; se piano e spec divergono, vince la spec
**Stato Fase 1:** [`2026-08-14-keeppix-fase-1c-STATO.md`](2026-08-14-keeppix-fase-1c-STATO.md)

---

## Global Constraints

Valgono per **ogni** task. Sono gli invarianti di [`/AGENTS.md`](../../../AGENTS.md), più quelli specifici di questa fase.

- **Rust edition 2024, toolchain 1.88.0.**
- **`keeppix-db` è l'unico crate con SQL.** `keeppix-media` non conosce il database.
- **Ogni metodo di repository che legge dati di un utente prende un `AuthContext` come primo parametro.** Le eccezioni ammesse sono solo quelle chiamate dallo scanner, ognuna con il motivo nel doc comment.
- **`Forbidden`, mai `NotFound`**, quando si sonda un id altrui — anche se l'id non esiste.
- **Query sempre parametrizzate.** L'unica interpolazione ammessa in `format!` è di costanti del codice.
- **Nessun `unwrap()`/`expect()` in produzione.** Nei test con `#[allow]` locale.
- Clippy `all` + `pedantic` a warn, `-D warnings` pulito. `cargo fmt --check` pulito.
- **Commit convenzionali in inglese**, uno per unità logica.

### Specifici della Fase 2

- **Un file RAW non si riscrive MAI.** I metadati vanno in un sidecar `.xmp` accanto al file. Questa non è una preferenza: ARW, NEF e CR3 sono contenitori proprietari poco documentati, e una scrittura fallita a metà produce un file irrecuperabile. Lightroom, Capture One e darktable si rifiutano tutti di scriverci dentro, per lo stesso motivo.
- **`asset_exif` non viene mai riscritto.** Le modifiche vivono in `asset_overrides`.
- **Ogni scrittura su file** (sidecar, o EXIF di un JPEG quando l'utente lo chiede esplicitamente) è: temporaneo nella stessa cartella → `fsync` → rilettura e verifica → `rename()` atomico.
- **La decodifica di file non fidati** che coinvolge codice C passa da `keeppix_media::sandbox::run`, con `rlimit` su memoria e CPU.
- **Rating e pick sono per utente** (`asset_flags` ha PK `(asset_id, user_id)`). Nell'XMP finisce il rating del **proprietario della libreria**.

---

## Struttura dei file

```
crates/keeppix-domain/src/
├── flags.rs         NEW  Rating, Pick, ColorLabel, AssetFlags
├── overrides.rs     NEW  AssetOverride, OverridePatch
└── lib.rs           MOD  riesportazioni

crates/keeppix-media/src/
├── raw.rs           NEW  extract_embedded_preview() per ARW/NEF/CR2/CR3/DNG/ORF/RAF
├── xmp.rs           NEW  read_sidecar() / write_sidecar()
├── derive.rs        MOD  derive_from_bytes(), estratta da derive_jpeg
└── lib.rs           MOD

crates/keeppix-db/
├── migrations/
│   ├── 0012_overrides_flags.sql   NEW  asset_overrides, asset_flags
│   ├── 0013_stacks.sql            NEW  stacks + assets.stack_id già esistente
│   └── 0014_trash.sql             NEW  trash_entries
├── src/
│   ├── overrides.rs   NEW  OverrideRepo — batch, undo, coda sidecar
│   ├── flags.rs       NEW  FlagRepo — rating/pick per utente
│   ├── stacks.rs      NEW  StackRepo — raggruppamento RAW+JPEG
│   ├── trash.rs       NEW  TrashRepo
│   └── duplicates.rs  NEW  DuplicateRepo
crates/keeppix-jobs/src/
├── raw.rs           NEW  job ExtractRawPreview
├── xmp.rs           NEW  job WriteSidecar
└── dispatch.rs      MOD  nuovi JobKind

crates/keeppix-api/src/routes/
├── flags.rs         NEW  PATCH rating/pick, batch
├── metadata.rs      NEW  PATCH override, batch, undo
├── trash.rs         NEW  DELETE con le tre opzioni, cestino, ripristino
└── duplicates.rs    NEW

frontend/src/
├── views/CullingView.vue        NEW
├── components/Filmstrip.vue     NEW
├── components/RatingStars.vue   NEW
└── stores/culling.ts            NEW
```

**Ordine dei task:** 1 → 2 → 3 → 4 → 5 → 6 → 7 → 8 → 9. I task 1-2 sono indipendenti dal resto e possono essere invertiti.

---

## Task 1: Estrazione della preview incorporata nei RAW

Il task che decide se la fase è veloce o lenta. ARW, NEF, CR2/CR3 e DNG contengono già un JPEG scritto dalla fotocamera, spesso a piena risoluzione: estrarlo costa ~40 ms contro i ~1,5-4 s di un demosaic.

**Files:**
- Create: `crates/keeppix-media/src/raw.rs`
- Create: `crates/keeppix-media/tests/raw.rs`
- Create: `crates/keeppix-media/tests/fixtures/` (file di esempio, vedi Step 1)
- Modify: `crates/keeppix-media/src/lib.rs`, `Cargo.toml`

**Interfaces:**
- Consumes: `AssetKind` da `keeppix-domain`.
- Produces:
  - `RawPreview { bytes: Vec<u8>, width: u32, height: u32, source: PreviewSource }`
  - `PreviewSource::{Embedded, Demosaic}`
  - `extract_embedded_preview(path: &Path) -> Result<Option<RawPreview>, RawError>` — `Ok(None)` se il formato è noto ma non contiene preview utilizzabile; `Err` solo su I/O o file corrotto.
  - `RawError::{Io, Unsupported(String), Corrupt(String)}`

- [ ] **Step 1: Procurare i file di esempio**

Servono RAW veri: non si può testare l'estrazione con file sintetici.

```bash
mkdir -p crates/keeppix-media/tests/fixtures
```

Usare i **file più piccoli disponibili** per ogni formato — non foto intere da 50 MB, che gonfierebbero il repository. Fonti accettabili: scatti a bassa risoluzione fatti apposta, oppure i campioni pubblici di [raw.pixls.us](https://raw.pixls.us) (licenza CC0).

Se un formato non è procurabile, **il test per quel formato viene marcato `#[ignore]` con il motivo**, non omesso: un formato non testato deve essere visibile.

Registrare nel ledger quali formati si sono potuti testare davvero.

- [ ] **Step 2: Scrivere i test che falliscono**

`crates/keeppix-media/tests/raw.rs`:

```rust
use std::path::Path;

use keeppix_media::raw::{PreviewSource, extract_embedded_preview};

fn fixture(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
}

#[test]
#[allow(clippy::unwrap_used)]
fn sony_arw_yields_a_full_size_embedded_jpeg() {
    let preview = extract_embedded_preview(&fixture("sample.arw")).unwrap().unwrap();

    assert_eq!(preview.source, PreviewSource::Embedded);
    // Un JPEG valido inizia con SOI.
    assert_eq!(&preview.bytes[..2], &[0xFF, 0xD8]);
    assert!(preview.width >= 1440, "Sony incorpora una preview grande: {}", preview.width);
}

#[test]
#[allow(clippy::unwrap_used)]
fn canon_cr3_yields_the_prvw_box() {
    let preview = extract_embedded_preview(&fixture("sample.cr3")).unwrap().unwrap();
    assert_eq!(&preview.bytes[..2], &[0xFF, 0xD8]);
    // CR3 espone una preview più piccola delle altre: ~1620 px.
    assert!(preview.width >= 1024);
}

#[test]
#[allow(clippy::unwrap_used)]
fn nikon_nef_yields_a_preview() {
    let preview = extract_embedded_preview(&fixture("sample.nef")).unwrap().unwrap();
    assert_eq!(&preview.bytes[..2], &[0xFF, 0xD8]);
}

#[test]
#[allow(clippy::unwrap_used)]
fn dng_yields_a_preview() {
    let preview = extract_embedded_preview(&fixture("sample.dng")).unwrap().unwrap();
    assert_eq!(&preview.bytes[..2], &[0xFF, 0xD8]);
}

#[test]
#[allow(clippy::unwrap_used)]
fn the_extracted_bytes_decode_as_a_real_image() {
    // Non basta che inizi con SOI: deve essere decodificabile davvero,
    // altrimenti il derivato fallirebbe più a valle con un errore oscuro.
    let preview = extract_embedded_preview(&fixture("sample.arw")).unwrap().unwrap();
    let mut decoder = zune_jpeg::JpegDecoder::new(&preview.bytes);
    decoder.decode_headers().expect("la preview è un JPEG decodificabile");
    let info = decoder.info().expect("dimensioni leggibili");
    assert_eq!(u32::from(info.width), preview.width, "le dimensioni dichiarate combaciano");
}

#[test]
#[allow(clippy::unwrap_used)]
fn a_truncated_raw_is_an_error_not_a_panic() {
    let dir = tempfile::tempdir().unwrap();
    let truncated = dir.path().join("broken.arw");
    // Primi 512 byte di un ARW valido: header presente, corpo assente.
    let full = std::fs::read(fixture("sample.arw")).unwrap();
    std::fs::write(&truncated, &full[..512.min(full.len())]).unwrap();

    let result = extract_embedded_preview(&truncated);
    assert!(result.is_err() || result.unwrap().is_none(), "mai un panico su file corrotto");
}

#[test]
#[allow(clippy::unwrap_used)]
fn a_non_raw_file_is_unsupported_not_a_crash() {
    let dir = tempfile::tempdir().unwrap();
    let text = dir.path().join("nota.txt");
    std::fs::write(&text, b"non sono un raw").unwrap();

    assert!(extract_embedded_preview(&text).is_err());
}
```

- [ ] **Step 3: Eseguire e verificare il fallimento**

Run: `cargo test -p keeppix-media --test raw`
Expected: FAIL — `unresolved import keeppix_media::raw`.

- [ ] **Step 4: Scegliere la strategia di estrazione e registrarla**

Due strade, da valutare in quest'ordine:

1. **Crate `rawler`** (o `rawloader`): copre i formati principali e restituisce le preview. Da preferire se la copertura è sufficiente.
2. **Estrazione manuale**: TIFF/EXIF IFD per ARW, NEF, CR2, DNG, ORF; box ISO-BMFF `PRVW` per CR3; JPEG in coda per RAF.

Verificare quale copre i formati richiesti **eseguendo i test**, non leggendo la documentazione. Registrare la scelta nel ledger come `Ruling`, con la copertura reale per formato.

- [ ] **Step 5: Implementare `raw.rs`**

Requisiti che il codice deve rispettare:

- **Selezione della preview più grande** quando il file ne contiene più d'una (i DNG spesso hanno una thumbnail e una preview): si prende quella con il lato lungo maggiore.
- **Soglia di utilizzabilità**: una preview sotto 1440 px sul lato lungo restituisce comunque `Some`, ma il chiamante (Task 3) decide se accontentarsi o passare al demosaic. `raw.rs` non prende quella decisione.
- **Nessun `unwrap`**: un file corrotto è un `Err`, mai un panico.
- **Nessuna allocazione illimitata**: se l'header dichiara una preview da 2 GB, rifiutare invece di allocare.

- [ ] **Step 6: Eseguire i test**

Run: `cargo test -p keeppix-media --test raw`
Expected: PASS per i formati procurati; `ignored` con motivo per gli altri.

- [ ] **Step 7: Misurare, e registrare la misura**

Il numero che serve alle fasi successive:

```bash
cargo test -p keeppix-media --test raw --release -- --nocapture
```

Registrare nel ledger: **ms per estrazione, per formato**, e la **risoluzione della preview ottenuta**. La spec stima 30-80 ms e ≥90% di copertura al passo «preview trovata»: se la misura reale diverge, correggere la spec.

- [ ] **Step 8: Commit**

```bash
git add crates/keeppix-media
git commit -m "feat(media): extract the embedded preview from raw files"
```

---

## Task 2: Derivati da un buffer, non solo da un percorso

`derive_jpeg(src: &Path, …)` legge da disco. Per i RAW il JPEG è già in memoria: serve la stessa pipeline che accetta byte.

**Files:**
- Modify: `crates/keeppix-media/src/derive.rs`
- Modify: `crates/keeppix-media/tests/` (i test esistenti non devono cambiare comportamento)

**Interfaces:**
- Produces: `derive_from_bytes(bytes: &[u8], data_dir: &Path, hash: &[u8; 32]) -> Result<DeriveResult, DeriveError>`
- `derive_jpeg` resta invariata nella firma e diventa un sottile wrapper: `fs::read` + `derive_from_bytes`.

- [ ] **Step 1: Verificare il verde di partenza**

Run: `cargo test -p keeppix-media`
Expected: PASS. Annotare il numero di test: deve restare identico più i nuovi.

- [ ] **Step 2: Scrivere il test che fallisce**

```rust
#[test]
#[allow(clippy::unwrap_used)]
fn deriving_from_bytes_matches_deriving_from_a_file() {
    let dir = tempfile::tempdir().unwrap();
    let src = fixture("sample.jpg");
    let bytes = std::fs::read(&src).unwrap();
    let hash = [7u8; 32];

    let from_bytes = derive_from_bytes(&bytes, dir.path(), &hash).unwrap();

    let dir2 = tempfile::tempdir().unwrap();
    let from_file = derive_jpeg(&src, dir2.path(), &hash).unwrap();

    assert_eq!(from_bytes.thumbhash, from_file.thumbhash,
        "la stessa immagine deve produrre lo stesso thumbhash da entrambe le vie");
    assert_eq!(
        std::fs::read(&from_bytes.thumb).unwrap().len(),
        std::fs::read(&from_file.thumb).unwrap().len()
    );
}
```

- [ ] **Step 3: Rifattorizzare**

Estrarre il corpo di `derive_jpeg` in `derive_from_bytes`, lasciando `derive_jpeg` come:

```rust
pub fn derive_jpeg(src: &Path, data_dir: &Path, hash: &[u8; 32]) -> Result<DeriveResult, DeriveError> {
    let (thumb, preview) = derivative_paths(data_dir, hash);
    if thumb.is_file() {
        // Il ramo di idempotenza resta qui: evita di leggere il file
        // quando i derivati esistono già.
        let preview = preview.is_file().then_some(preview);
        return Ok(DeriveResult { thumb, preview, thumbhash: Vec::new(), skipped: true });
    }
    derive_from_bytes(&fs::read(src)?, data_dir, hash)
}
```

**Attenzione**: il controllo di idempotenza (`thumb.is_file()`) deve restare in **entrambe** le funzioni, altrimenti `derive_from_bytes` chiamata direttamente rigenererebbe derivati già presenti.

- [ ] **Step 4: Eseguire e verificare**

Run: `cargo test -p keeppix-media && cargo clippy -p keeppix-media --all-targets -- -D warnings`
Expected: tutti i test precedenti verdi più il nuovo. **Nessun test esistente modificato**: se uno cambia comportamento, la rifattorizzazione è sbagliata.

- [ ] **Step 5: Commit**

```bash
git add crates/keeppix-media
git commit -m "refactor(media): derive from bytes so raw previews reuse the pipeline"
```

---

## Task 3: Job di derivazione dei RAW

**Files:**
- Create: `crates/keeppix-jobs/src/raw.rs`
- Modify: `crates/keeppix-jobs/src/dispatch.rs`, `lib.rs`
- Create/Modify: test di integrazione dei job

**Interfaces:**
- Consumes: `extract_embedded_preview` (Task 1), `derive_from_bytes` (Task 2), `AssetRepo::set_indexed`/`set_error`, `sandbox::run`.
- Produces: `JobKind::DeriveRaw`, gestito nel dispatcher con la stessa forma degli altri job.

**La cascata da implementare**, nell'ordine della spec §2.1:

```
1. Sidecar .xmp presente?  → lo legge il Task 5, non questo job
2. extract_embedded_preview()
3. Preview ≥1440 px?       → derive_from_bytes() sulla preview. Fine.
4. Preview piccola/assente? → demosaic con libraw in sandbox, half-size
5. Fallita anche quella?    → AssetRepo::set_error, compare in Problemi
```

- [ ] **Step 1: Scrivere il test di integrazione che fallisce**

Il test deve verificare la **cascata**, non solo il caso felice:

```rust
#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn a_raw_with_a_large_preview_never_calls_libraw() {
    // Il punto del task: se la preview basta, il demosaic non deve partire.
    // Si verifica contando le invocazioni della sandbox, non i tempi.
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn a_raw_without_a_preview_falls_back_to_demosaic() { /* … */ }

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn a_corrupt_raw_sets_the_asset_to_error_and_does_not_block_the_queue() {
    // Il job successivo in coda deve comunque essere processato.
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn the_job_is_idempotent() {
    // Eseguirlo due volte non deve rigenerare i derivati né duplicare righe.
}
```

Il primo test è quello che vale: senza, si può implementare una cascata che chiama sempre libraw e nessuno se ne accorge finché la scansione non impiega venti ore invece di due.

- [ ] **Step 2: Eseguire, verificare il fallimento, implementare**

Requisiti:

- **libraw gira in `sandbox::run`**, mai in-process: apre file non fidati ed è codice C.
- Il demosaic è **half-size** con bilanciamento del bianco della fotocamera: è per il culling, non per l'esportazione.
- Il timeout della sandbox va dimensionato sul caso peggiore reale misurato al Task 1, non a occhio.
- **Nessun `unwrap`** nel percorso del job: un RAW corrotto è un `set_error`, non un panico che uccide il worker.

- [ ] **Step 3: Verificare e committare**

Run: `cargo test -p keeppix-jobs -- --test-threads=1 && cargo clippy --workspace --all-targets -- -D warnings`

```bash
git commit -m "feat(jobs): derive raw assets from the embedded preview"
```

---

## Task 4: `asset_overrides` e `asset_flags`

**Files:**
- Create: `crates/keeppix-db/migrations/0012_overrides_flags.sql`
- Create: `crates/keeppix-domain/src/flags.rs`, `overrides.rs`
- Create: `crates/keeppix-db/src/overrides.rs`, `flags.rs`
- Create: `crates/keeppix-db/tests/overrides.rs`, `flags.rs`

**Interfaces:**
- Produces:
  - `Rating(u8)` — 0..=5, `Rating::parse` rifiuta fuori range.
  - `Pick::{None, Pick, Reject}`
  - `AssetFlags { rating, pick, color_label }`
  - `OverridePatch { title, description, taken_at, location, place_id, orientation }` — ogni campo `Option<Option<T>>`: `None` = non toccare, `Some(None)` = azzera, `Some(Some(v))` = imposta.
  - `FlagRepo::{set, get, batch_set}` — tutti con `AuthContext`, tutti per utente.
  - `OverrideRepo::{apply, apply_batch, undo_batch, effective, pending_sidecars}`

**La migrazione:**

```sql
CREATE TABLE asset_overrides (
    asset_id       uuid PRIMARY KEY REFERENCES assets(id) ON DELETE CASCADE,
    title          text,
    description    text,
    taken_at       timestamptz,
    location       geography(Point, 4326),
    place_id       bigint,
    orientation    smallint,
    updated_by     uuid REFERENCES users(id),
    updated_at     timestamptz NOT NULL DEFAULT now(),
    -- NULL = mai scritto su file. Il job dei sidecar seleziona
    -- WHERE updated_at > COALESCE(xmp_written_at, '-infinity').
    xmp_written_at timestamptz
);

CREATE INDEX asset_overrides_pending_idx ON asset_overrides (updated_at)
    WHERE xmp_written_at IS NULL OR xmp_written_at < updated_at;

CREATE TABLE asset_flags (
    asset_id    uuid NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
    user_id     uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    rating      smallint CHECK (rating BETWEEN 0 AND 5),
    pick        text CHECK (pick IN ('none','pick','reject')),
    color_label text,
    updated_at  timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (asset_id, user_id)
);

-- Il culling filtra per "gli scarti di questo utente": indice su (user_id, pick).
CREATE INDEX asset_flags_user_pick_idx ON asset_flags (user_id, pick)
    WHERE pick <> 'none';

-- Registro delle operazioni batch, per l'annullamento.
CREATE TABLE metadata_batches (
    id          uuid PRIMARY KEY,
    actor_id    uuid NOT NULL REFERENCES users(id),
    applied_at  timestamptz NOT NULL DEFAULT now(),
    undone_at   timestamptz,
    -- Valori precedenti, per asset. Serve solo all'annullamento.
    previous    jsonb NOT NULL
);
```

- [ ] **Step 1: Scrivere i test che falliscono**

Devono pinnare almeno:

- `effective()` restituisce `COALESCE(override, exif)` campo per campo — un override parziale non azzera i campi non toccati;
- `apply_batch` su 500 asset è **una** operazione, non 500 round-trip;
- `undo_batch` ripristina esattamente i valori precedenti, **anche quando il valore precedente era NULL**;
- `undo_batch` su un batch già annullato è idempotente, non raddoppia;
- il rating è **per utente**: due utenti sullo stesso asset non si sovrascrivono;
- un utente non proprietario riceve `Forbidden`, e su un id inesistente **anch'esso** `Forbidden`;
- `pending_sidecars` restituisce solo gli asset con `updated_at > xmp_written_at`.

Il test sull'`undo` con valore precedente `NULL` è quello che si dimentica: senza, «annulla» trasforma un campo mai valorizzato in stringa vuota.

- [ ] **Step 2-4: Fallimento, implementazione, verifica**

Run: `cargo test -p keeppix-db -- --test-threads=1`

- [ ] **Step 5: Commit**

```bash
git commit -m "feat(db): add metadata overrides and per-user flags"
```

---

## Task 5: Sidecar XMP

**Files:**
- Create: `crates/keeppix-media/src/xmp.rs`, `crates/keeppix-media/tests/xmp.rs`
- Create: `crates/keeppix-jobs/src/xmp.rs`

**Interfaces:**
- Produces:
  - `SidecarData { rating, description, title, tags, gps, taken_at, label }`
  - `read_sidecar(path: &Path) -> Result<Option<SidecarData>, XmpError>`
  - `write_sidecar(path: &Path, data: &SidecarData) -> Result<(), XmpError>`
  - `JobKind::WriteSidecar`

**Mappatura dei campi** (spec §3.4):

| Keeppix | XMP |
|---|---|
| rating (del proprietario) | `xmp:Rating` |
| description | `dc:description` |
| title | `dc:title` |
| tag | `dc:subject` |
| GPS | `exif:GPSLatitude` / `exif:GPSLongitude` |
| taken_at | `exif:DateTimeOriginal` |
| pick/reject | `xmp:Label` |

- [ ] **Step 1: Scrivere i test che falliscono**

Il test che conta più di tutti:

```rust
#[test]
#[allow(clippy::unwrap_used)]
fn writing_preserves_fields_we_do_not_manage() {
    // Un sidecar prodotto da Lightroom contiene campi che Keeppix non
    // conosce (crs:Exposure2012, crs:Temperature, …). Riscriverlo da zero
    // li cancellerebbe: perdere il lavoro di sviluppo altrui è peggio che
    // non scrivere affatto.
    let dir = tempfile::tempdir().unwrap();
    let sidecar = dir.path().join("IMG_0001.ARW.xmp");
    std::fs::write(&sidecar, LIGHTROOM_SIDECAR).unwrap();

    write_sidecar(&sidecar, &SidecarData { rating: Some(4), ..Default::default() }).unwrap();

    let after = std::fs::read_to_string(&sidecar).unwrap();
    assert!(after.contains("crs:Exposure2012"), "i campi altrui sopravvivono");
    assert!(after.contains("xmp:Rating=\"4\""), "il nostro campo è aggiornato");
}
```

Più: lettura di un sidecar reale di darktable; round-trip; file malformato che dà `Err` e non panico; scrittura atomica verificata (il file non esiste mai in stato parziale).

- [ ] **Step 2: Implementare**

Requisiti:

- **Leggere, modificare, riscrivere** — mai generare da zero.
- **Scrittura atomica**: `.xmp.tmp` nella stessa cartella → `fsync` → **rilettura e verifica** → `rename()`.
- Se la cartella è in sola lettura, il job fallisce con retry, e **l'applicazione continua a funzionare**: l'override resta nel database.

- [ ] **Step 3: Il job**

`WriteSidecar` prende gli asset da `pending_sidecars`, priorità 3 (background), ritentabile. Registra `xmp_written_at` solo dopo la verifica.

**Il rating scritto è quello del proprietario della libreria**, non di chi ha fatto l'ultima modifica: `xmp:Rating` è un valore singolo.

- [ ] **Step 4: Verificare e committare**

```bash
git commit -m "feat(media): read and write xmp sidecars without losing foreign fields"
```

---

## Task 6: Stack RAW+JPEG

**Files:**
- Create: `crates/keeppix-db/migrations/0013_stacks.sql`, `crates/keeppix-db/src/stacks.rs`, `tests/stacks.rs`

**Regole di raggruppamento** (spec §5):

1. Stesso nome base nella stessa cartella: `DSC_0042.ARW` + `DSC_0042.JPG`.
2. Scatti entro 2 secondi con stesso corpo macchina e stesso numero di scatto.

**L'asset primario è il RAW** quando presente: ha più informazione.

- [ ] **Step 1: Test che falliscono**

Devono coprire: raggruppamento per nome base; il RAW è primario; un JPEG solo **non** forma uno stack; tre file con lo stesso nome base ma estensioni diverse; cancellare il primario promuove un altro membro invece di orfanare lo stack; il raggruppamento è idempotente su riscansione.

L'ultimo è quello che rompe: senza, ogni scansione crea uno stack nuovo.

- [ ] **Step 2-4: Implementazione, verifica, commit**

```bash
git commit -m "feat(db): group raw and jpeg shots into stacks"
```

---

## Task 7: Cestino e cancellazione a tre opzioni

**Files:**
- Create: `crates/keeppix-db/migrations/0014_trash.sql`, `crates/keeppix-db/src/trash.rs`
- Create: `crates/keeppix-api/src/routes/trash.rs`

**Le tre opzioni** (spec §6), presentate **ogni volta**:

| Opzione | Cosa fa | `disk_action` |
|---|---|---|
| Rimuovi dall'indice | il file resta, l'asset sparisce (tornerà alla prossima scansione) | `kept` |
| Sposta nel cestino | `rename()` in `.keeppix-trash/` dentro la stessa libreria, 30 giorni | `moved_to_trash` |
| Elimina dal disco | irreversibile | `purged` |

- [ ] **Step 1: Test che falliscono**

Devono pinnare:

- il cestino è un `rename()` **dentro la stessa libreria**, non una copia — verificabile controllando che l'inode non cambi;
- `.keeppix-trash/` è **escluso dalla scansione** (altrimenti gli asset cestinati vengono reindicizzati al giro dopo);
- il ripristino rimette il file al percorso originale e riporta l'asset a `indexed`;
- il ripristino quando il percorso originale è occupato da un altro file **non sovrascrive**;
- **solo owner e admin** possono usare `purged`; un editor riceve `Forbidden`;
- la pulizia oltre i 30 giorni cancella dal disco e rimuove la riga.

Il secondo è quello che si dimentica e che produce un ciclo infinito visibile solo su una libreria grande.

- [ ] **Step 2-4: Implementazione, verifica, commit**

```bash
git commit -m "feat: add the trash with an explicit choice on every delete"
```

---

## Task 8: Duplicati ed editing batch

**Files:**
- Create: `crates/keeppix-db/src/duplicates.rs`
- Create: `crates/keeppix-api/src/routes/duplicates.rs`, `metadata.rs`, `flags.rs`

**Duplicati**: gruppi con `content_hash` uguale e `count > 1`, con lo spazio recuperabile. Deduplica **esatta per hash**, nessun ML.

**Editing batch**: il caso d'uso centrale della fase — «seleziono 5.000 foto e metto la posizione».

- [ ] **Step 1: Test che falliscono**

- `apply_batch` su 5.000 asset resta sotto un secondo (misurare, non assumere);
- lo **scostamento di N ore** sulla data di scatto è offerto come operazione a sé, non calcolato dall'utente — è il rimedio quando si torna da un viaggio con l'orologio della macchina sbagliato;
- l'annullamento funziona finché il sidecar non è stato scritto;
- i duplicati non contano gli asset `trashed`;
- lo spazio recuperabile è `size_bytes × (copie − 1)`, non la somma totale.

- [ ] **Step 2-4: Implementazione, verifica, commit**

---

## Task 9: Modalità culling nel frontend

**Files:**
- Create: `frontend/src/views/CullingView.vue`, `components/Filmstrip.vue`, `RatingStars.vue`, `stores/culling.ts`
- Modify: router, i18n (it + en)

**Un unico punto d'ingresso**: il pulsante *Culling* nella barra di una cartella o di una selezione. Non tre scorciatoie sparse.

**La regola dura**: il visualizzatore normale **non diventa una modalità**. Lì entrano solo rating (`1-5`) e preferito (`f`); tutto il resto vive solo nel culling.

Scorciatoie: `1-5` rating · `p` pick · `x` reject · `←→` naviga · `z` zoom 1:1 · `c` confronto · `Canc` elimina.

- [ ] **Step 1: Test che falliscono**

- l'avanzamento automatico dopo il voto porta alla foto successiva;
- le scorciatoie **non sparano mentre l'utente scrive** in un campo di testo;
- il filtro «solo scarti» mostra ciò che dichiara;
- lo store non perde i voti se la rete cade: si accodano e si ritentano.

- [ ] **Step 2: Implementare**

Requisito di prestazione: **zoom 1:1 istantaneo**. Serve l'originale, non la preview: si precarica il ritaglio centrale a piena risoluzione delle **3 foto successive**. È l'unico posto dove si legge l'originale in modo aggressivo, ed è giustificato — è il controllo della messa a fuoco, il motivo per cui esiste il culling.

- [ ] **Step 3: Verificare**

```bash
cd frontend && npx vue-tsc --noEmit && npx vitest run && npm run build
```

Il bundle d'ingresso deve restare **sotto 150 KB gzip**: `CullingView` va in un chunk lazy, come mappa e impostazioni.

- [ ] **Step 4: Commit**

---

## Criteri di completamento della Fase 2

- [ ] `cargo test --workspace -- --test-threads=1` verde.
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` pulito, `cargo fmt --check` pulito.
- [ ] **Misurato e registrato nel ledger**: ms per estrazione preview per formato, e percentuale reale di RAW che si fermano al passo «preview trovata» senza demosaic.
- [ ] Una sessione di culling su ≥100 scatti reali si completa da tastiera senza attese percepibili, con zoom 1:1 istantaneo.
- [ ] Editing batch su ≥1.000 asset sotto un secondo, annullabile.
- [ ] Un sidecar prodotto da Lightroom sopravvive a una riscrittura di Keeppix con i suoi campi intatti.
- [ ] Il cestino è un `rename()`, `.keeppix-trash/` è escluso dalla scansione, e il ripristino funziona.
- [ ] **Nessun file RAW è stato riscritto**: verificabile confrontando gli hash prima e dopo un ciclo completo di editing.
- [ ] Bundle frontend sotto 150 KB gzip.
- [ ] CI verde sulla PR.

## Cosa NON è in Fase 2

Condivisione e permessi (Fase 3), mappa e geocoding (Fase 4 — in Fase 2 si può scrivere `location` negli override, ma l'interfaccia per sceglierla arriva dopo), import GPX (predisposto da `location_source = 'gpx'`, non implementato), WebDAV (Fase 5), video e backup (Fase 6).

## Debiti della Fase 1 da saldare qui

| Voce | Perché ora |
|---|---|
| **Il percorso testcontainers in locale è instabile** (`PortNotExposed` su 1-3 test a caso) | La CI non lo vede perché usa un service container: la CI è verde e chi sviluppa vede rossi casuali. Serve un retry con backoff attorno a `get_host_port_ipv4` |
| **Prefissi delle migrazioni incoerenti** (`0009_` a 4 cifre, `00010_` a 5) | L'ordinamento è corretto, è solo confuso. Uniformare **dalle nuove migrazioni** di questa fase; non rinominare quelle applicate, cambierebbe il checksum |
