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

