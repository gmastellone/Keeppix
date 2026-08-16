# Review — Task 2: `derive_from_bytes`

**Base:** `38ee07f` · **Head:** `55e5e70` (implementation commit; docs-only `2d72067` excluded from gate) · Branch `fase-2`

Verified independently: read `derive.rs`, `lib.rs`, and `tests/derive.rs` in full; diffed
`38ee07f..55e5e70 -- crates/keeppix-media/`; grepped for `unwrap`/`expect`, `sqlx`, and
`keeppix-db`; re-ran `cargo test -p keeppix-media --test derive` (2 passed) and
`cargo clippy -p keeppix-media --all-targets -- -D warnings` (clean). Full workspace
suite not re-run per review instructions.

## Spec compliance

Interface contract matches the brief exactly:

```rust
pub fn derive_from_bytes(
    bytes: &[u8],
    data_dir: &Path,
    hash: &[u8; 32],
) -> Result<DeriveResult, DeriveError>
```

Re-exported from `crates/keeppix-media/src/lib.rs`. `derive_jpeg` signature unchanged; body
is the prescribed thin wrapper (idempotency → `fs::read` → `derive_from_bytes`).

- ✅ **Idempotency in both entry points** — `thumb.is_file()` early-return with
  `skipped: true` and empty `thumbhash` is duplicated verbatim in `derive_jpeg`
  (`derive.rs:45-52`) and `derive_from_bytes` (`derive.rs:67-74`), as the brief
  requires. `derive_jpeg` still avoids reading the source file when derivatives already
  exist.
- ✅ **Existing tests unchanged in behavior** — `derive_writes_thumb_and_leaves_original`
  is byte-identical in the diff; only additions are the new parity test and imports.
  Targeted re-run confirms both derive tests pass, including idempotency/mtime/original
  assertions on the pre-existing test.
- ✅ **No `unwrap()` / `expect()` in production** — grep over `derive.rs` finds only
  `.unwrap_or(...)` fallbacks on `try_from` (pre-existing pattern, not panics).
- ✅ **`keeppix-media` does not know the database** — no `sqlx` / `keeppix-db`
  references anywhere in the crate.
- ✅ **Pure refactor, no pipeline drift** — diff against base shows the decode/resize/write
  body moved unchanged into `derive_from_bytes`; the only semantic delta is
  `JpegDecoder::new(&bytes)` → `JpegDecoder::new(bytes)` (equivalent for `&[u8]` input).
  The second idempotency check inside `derive_from_bytes` is redundant when called from
  `derive_jpeg` but correct for direct callers (Task 3) and slightly safer under races
  (skip after an concurrent write rather than regenerating).
- ✅ **TDD evidence credible** — report's RED (`unresolved import derive_from_bytes`) matches
  the expected failure mode; GREEN adds exactly the three files the brief names.
- ✅ **Test count** — +1 test in `tests/derive.rs` (2 derive tests now); report's 22→23
  crate-wide count is consistent (not independently re-counted across all integration tests).

### Fixture substitution (documented, acceptable)

Brief Step 2 cites `sample.jpg`; repository has `tiny.jpg`. Implementer used `tiny.jpg`
and recorded a `Ruling` in `progress.md`. Same role (small JPEG for the derivative pipeline);
no behavioral impact.

## Code quality

**Critical:** none.

**Important:** none.

**Minor:**

- The new parity test asserts equal `thumbhash` and equal thumb **file lengths**, not
  byte-identical thumb/preview files or matching `preview` `Option` paths. That matches
  the brief's Step 2 assertions verbatim; a stricter equality check would be nice but is
  out of scope for this task.
- Idempotency when **`derive_from_bytes` is called directly** (the Task 3 path) is
  implemented but not covered by a dedicated test — only `derive_jpeg`'s skip path is
  exercised by `derive_writes_thumb_and_leaves_original`. The duplicated guard is
  trivially identical to the tested one; consider a one-liner test before Task 3 if you
  want belt-and-suspenders, not required to unblock this task.
- When `derive_jpeg` misses the early skip but a concurrent writer creates the thumb before
  `derive_from_bytes` runs, the source file is read into RAM then discarded on the inner
  skip. Rare, acceptable; cheaper than regenerating.

## Overall: **APPROVED**

Mechanical extraction done correctly: interface matches the brief, idempotency preserved
in both public entry points, existing behavior and tests intact, no production panics, no
DB coupling. Ready for Task 3 to feed in-memory RAW preview bytes.
