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

