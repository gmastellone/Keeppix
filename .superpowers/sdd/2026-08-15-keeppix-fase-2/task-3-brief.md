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

