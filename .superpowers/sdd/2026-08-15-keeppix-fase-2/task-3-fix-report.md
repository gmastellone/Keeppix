# Task 3 — fix report: thumbhash mai persistito per i RAW

**Status:** complete
**Finding:** Critical (review Task 3)
**Branch:** `fase-2` (non pushato)

## Difetto

`crates/keeppix-jobs/src/raw.rs`: `derive_raw` scartava
`DeriveResult.thumbhash` (tornava `Result<(), String>`) e `run_with` non
chiamava mai `AssetRepo::set_thumbhash_for_hash`, a differenza di
`crates/keeppix-jobs/src/derive.rs::run`, che lo fa. Per l'idempotenza
basata sull'esistenza del file `{hash}-thumb.webp` (`run_with` esce subito
se `thumb_path.is_file()`), un asset RAW derivato una volta restava con
`thumbhash IS NULL` **permanentemente**: nessuna riesecuzione del job lo
avrebbe corretto, perché il thumbnail esiste già e il job non tenta più il
derive.

## Fix

1. `derive_raw` ora ritorna `Result<DeriveResult, String>` invece di
   `Result<(), String>` — propaga semplicemente il risultato di
   `derive_from_bytes`/`derive_from_rgb`, che già lo produce.
2. `run_with`, dopo un derive riuscito con `!result.skipped`, chiama
   `assets.set_thumbhash_for_hash(&hash, &result.thumbhash)` — stessa
   chiamata già presente in `derive.rs::run`. Il ramo `Err` continua a fare
   `set_error` su tutti gli asset con quell'hash, come prima.

```68:113:crates/keeppix-jobs/src/raw.rs
pub async fn run_with(
    db: &Db,
    data_dir: &Path,
    hash: [u8; 32],
    demosaic: &dyn Demosaic,
) -> Result<(), JobError> {
    // ... risoluzione del file sorgente dall'hash ...

    // Idempotenza: se il derivato esiste già, niente da rifare — soprattutto
    // niente demosaic, che è l'unico passo davvero costoso qui.
    let (thumb_path, _) = derivative_paths(data_dir, &hash);
    if thumb_path.is_file() {
        return Ok(());
    }

    match derive_raw(&src, data_dir, &hash, demosaic) {
        Ok(result) if !result.skipped => {
            assets
                .set_thumbhash_for_hash(&hash, &result.thumbhash)
                .await?;
        }
        Ok(_) => {}
        Err(detail) => {
            for id in ids {
                assets.set_error(id, &detail).await?;
            }
        }
    }
    Ok(())
}
```

`Ok(_)` (cioè `result.skipped == true`) resta gestito ma non dovrebbe mai
verificarsi in pratica in questo percorso: `run_with` ha già controllato
`thumb_path.is_file()` prima di chiamare `derive_raw`, quindi
`derive_from_bytes`/`derive_from_rgb` non troveranno mai il thumbnail già
presente al loro interno. Il ramo resta per coerenza col contratto di
`DeriveResult` (stesso pattern di `derive.rs::run`), non perché sia
raggiungibile oggi.

## TDD — RED prima, poi GREEN

Due test nuovi in `crates/keeppix-jobs/tests/raw.rs`, uno per ciascun
percorso della cascata:

- `deriving_from_the_embedded_preview_populates_the_thumbhash`: usa il
  fixture reale `sample.arw` (preview incorporata ≥1440px, nessun
  demosaic). Dopo `run_with`, legge l'asset con `AssetRepo::get_for_scan` e
  verifica `asset.thumbhash.is_some_and(|h| !h.is_empty())`.
- `deriving_from_the_demosaic_fallback_populates_the_thumbhash`: stesso
  assert sul percorso di fallback (TIFF minimo senza preview → mock
  demosaic).

### RED

```
$ cargo test -p keeppix-jobs --test raw -- --test-threads=1 --nocapture

running 9 tests
test a_corrupt_raw_sets_the_asset_to_error_and_does_not_block_the_queue ... ok
test a_raw_with_a_large_preview_never_calls_libraw ... ok
test a_raw_without_a_preview_falls_back_to_demosaic ... ok
test deriving_from_the_demosaic_fallback_populates_the_thumbhash ...
thread 'deriving_from_the_demosaic_fallback_populates_the_thumbhash' panicked at crates/keeppix-jobs/tests/raw.rs:277:5:
il thumbhash deve essere salvato anche per il fallback al demosaic
FAILED
test deriving_from_the_embedded_preview_populates_the_thumbhash ...
thread 'deriving_from_the_embedded_preview_populates_the_thumbhash' panicked at crates/keeppix-jobs/tests/raw.rs:254:5:
il thumbhash deve essere salvato anche per la preview incorporata di un RAW
FAILED
test harness::tests::appends_when_the_url_has_no_database ... ok
test harness::tests::preserves_the_query_string ... ok
test harness::tests::replaces_an_existing_database_name ... ok
test the_job_is_idempotent ... ok

test result: FAILED. 7 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out; finished in 21.61s
```

Entrambi i nuovi test falliscono correttamente prima del fix, sul campo
`thumbhash` esattamente come descritto nella finding.

### GREEN

```
$ cargo test -p keeppix-jobs --test raw -- --test-threads=1 --nocapture

running 9 tests
test a_corrupt_raw_sets_the_asset_to_error_and_does_not_block_the_queue ... ok
test a_raw_with_a_large_preview_never_calls_libraw ... ok
test a_raw_without_a_preview_falls_back_to_demosaic ... ok
test deriving_from_the_demosaic_fallback_populates_the_thumbhash ... ok
test deriving_from_the_embedded_preview_populates_the_thumbhash ... ok
test harness::tests::appends_when_the_url_has_no_database ... ok
test harness::tests::preserves_the_query_string ... ok
test harness::tests::replaces_an_existing_database_name ... ok
test the_job_is_idempotent ... ok

test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 19.73s
```

I 7 test preesistenti restano verdi senza modifiche: il fix non cambia
l'idempotenza né la cascata scelta-preview/demosaic, solo cosa succede col
risultato dopo che il derive è andato a buon fine.

## Verifica completa

```
cargo test -p keeppix-jobs --jobs 1 -- --test-threads=1   → 34 passed, 0 failed (tutti i binari del crate)
cargo fmt --check                                          → pulito
cargo clippy --workspace --all-targets -- -D warnings      → pulito
cargo deny check bans                                       → "bans ok"
```

## Note

- Nessun cambiamento a `derive.rs`, `dispatch.rs`, `hash.rs`: il difetto
  era isolato a `raw.rs`.
- Nessun `unwrap()`/`expect()` fuori dai test.
- Non ho toccato Task 4 né fatto push/PR.
