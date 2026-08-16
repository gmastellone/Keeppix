# Task 2 report — Derivati da un buffer

**Status:** DONE  
**Commit:** `55e5e70` (`refactor(media): derive from bytes so raw previews reuse the pipeline`)  
**Branch:** `fase-2`

## Obiettivo

Estrarre `derive_from_bytes(bytes, data_dir, hash)` da `derive_jpeg`, lasciando
quest'ultima come sottile wrapper (`fs::read` + `derive_from_bytes`). Il
controllo di idempotenza (`thumb.is_file()`) resta in **entrambe** le funzioni.

## TDD

### Step 1 — Baseline verde

```text
cargo test -p keeppix-media
```

**Risultato:** 22 test passed (derive 1, exif 2, hash 2, kind 5, raw 8,
sandbox 1, video 1, walk 2).

### Step 2 — RED

Aggiunto `deriving_from_bytes_matches_deriving_from_a_file` in
`crates/keeppix-media/tests/derive.rs`.

**Nota fixture:** il brief cita `sample.jpg`; in `tests/fixtures/` esiste solo
`tiny.jpg` (oltre ai RAW). Il test usa `tiny.jpg` — stesso scopo (JPEG piccolo
per la pipeline derivati).

```text
cargo test -p keeppix-media --test derive deriving_from_bytes_matches_deriving_from_a_file
```

**Risultato (RED):**

```text
error[E0432]: unresolved import `keeppix_media::derive_from_bytes`
 --> crates/keeppix-media/tests/derive.rs:3:21
  |
3 | use keeppix_media::{derive_from_bytes, derive_jpeg, hash_file};
  |                     ^^^^^^^^^^^^^^^^^ no `derive_from_bytes` in the root
```

### Step 3 — GREEN (refactor)

- Estratto corpo di decodifica/resize/write in `derive_from_bytes`.
- `derive_jpeg`: idempotenza → `fs::read` → `derive_from_bytes`.
- `derive_from_bytes`: idempotenza duplicata → pipeline JPEG.
- Re-export in `lib.rs`.

### Step 4 — Verifica finale

```text
cargo test -p keeppix-media
cargo clippy -p keeppix-media --all-targets -- -D warnings
```

**Risultato:** 23 test passed (+1 nuovo), clippy senza warning.

Test esistente `derive_writes_thumb_and_leaves_original` invariato nel
comportamento (idempotenza, mtime, original intatto).

## File modificati

| File | Modifica |
|---|---|
| `crates/keeppix-media/src/derive.rs` | `derive_from_bytes` + wrapper `derive_jpeg` |
| `crates/keeppix-media/src/lib.rs` | re-export `derive_from_bytes` |
| `crates/keeppix-media/tests/derive.rs` | nuovo test parità bytes vs file |

## Interfaccia prodotta

```rust
pub fn derive_from_bytes(
    bytes: &[u8],
    data_dir: &Path,
    hash: &[u8; 32],
) -> Result<DeriveResult, DeriveError>
```

Pronta per Task 3 (`DeriveRaw`): preview JPEG estratta in RAM → stessa
pipeline senza file temporaneo.
