# Task 3 review — Job di derivazione dei RAW (`JobKind::DeriveRaw`)

**Verdict: CHANGES_REQUESTED**

Scope: task-scoped gate against `task-3-brief.md`, `task-3-report.md`,
`task-3-review.diff`, and the global constraints listed in the review
request. Read-only review; no git state mutated, no full suite re-run.

## Summary

The cascade itself (embedded preview ≥1440px → skip demosaic; otherwise
sandbox demosaic; failure → `set_error`) is implemented correctly, tested
by counting mock invocations (not timing) as required, and the sandbox/SQL/
`unwrap` invariants all hold. However, the new pipeline silently drops the
`thumbhash` that `derive_from_bytes`/`derive_from_rgb` compute: unlike the
sibling `DeriveAsset` job, `DeriveRaw` never calls
`AssetRepo::set_thumbhash_for_hash`. Every RAW asset processed by this job
will have `thumbhash IS NULL` forever (the idempotency check exits before
retrying once the thumb file exists), which breaks the blur-up placeholder
for the entire RAW asset population. No test in the diff would have caught
this because none of the four tests assert on the asset's `thumbhash`
column — they only assert on `demosaic.calls()` and thumbnail file
existence. This needs to be fixed before merge.

## Global constraints — checked

| Constraint | Status | Evidence |
|---|---|---|
| Cascade: embedded ≥1440 → no demosaic; else sandbox demosaic; else set_error | ✅ | `crates/keeppix-jobs/src/raw.rs:113-117` matches spec §2.1 exactly (long side `width.max(height) >= 1440`); failure path in `run_with:99-103` calls `set_error` on every asset sharing the hash and still returns `Ok(())`. |
| Count demosaic calls in tests, not timing | ✅ | `MockDemosaic` in `crates/keeppix-jobs/tests/raw.rs` uses an `AtomicUsize` counter; no `Instant`/timing assertions in any of the 4 tests. Report also documents a red/green mutation proving the counting assertion actually bites. |
| libraw/dcraw only via `sandbox::run` | ✅ | `demosaic_half` and `dcraw_emu_available` in `crates/keeppix-media/src/raw.rs` both go through `crate::sandbox::run`; no direct `Command::new("dcraw_emu")` elsewhere. |
| No `unwrap` in prod code | ✅ | Scanned `crates/keeppix-jobs/src/raw.rs` and the new code in `crates/keeppix-media/src/raw.rs`/`derive.rs` — no `unwrap()`/`expect()` outside `#[allow(clippy::unwrap_used)]`-gated test modules. |
| No SQL outside `keeppix-db` | ✅ | `crates/keeppix-jobs/src/raw.rs` only calls `AssetRepo`/`FolderRepo` methods; no `sqlx` import. |
| `keeppix-media` doesn't know the DB | ✅ | No `keeppix_db` import added to `keeppix-media`; report states `cargo deny check bans` stayed green (not independently re-run in this review, but the diff itself introduces no such edge). |
| Spec wins over plan | ✅ (no conflict found) | Brief and spec §2.1 agree on the cascade; no divergence spotted. |

## Defect — CHANGES_REQUESTED

### `DeriveRaw` never writes `thumbhash`, unlike `DeriveAsset`

`derive.rs`'s existing `DeriveAsset` job propagates the computed thumbhash:

```26:33:crates/keeppix-jobs/src/derive.rs
    let result = derive_jpeg(&src, data_dir, &hash).map_err(|e| JobError::Worker(e.to_string()))?;
    if result.skipped {
        return Ok(());
    }
    assets
        .set_thumbhash_for_hash(&hash, &result.thumbhash)
        .await?;
    Ok(())
```

`raw.rs`'s `derive_raw` calls the equivalent derivation functions but
discards the `DeriveResult` they return (including `result.thumbhash`),
and `run_with` never calls `set_thumbhash_for_hash` at all:

```107:129:crates/keeppix-jobs/src/raw.rs
fn derive_raw(
    src: &Path,
    data_dir: &Path,
    hash: &[u8; 32],
    demosaic: &dyn Demosaic,
) -> Result<(), String> {
    let preview = extract_embedded_preview(src).ok().flatten();
    let chosen = match preview {
        Some(p) if p.width.max(p.height) >= MIN_PREVIEW_LONG_SIDE => p,
        _ => demosaic.demosaic(src).map_err(|e| e.to_string())?,
    };

    match chosen.source {
        PreviewSource::Embedded => {
            derive_from_bytes(&chosen.bytes, data_dir, hash).map_err(|e| e.to_string())?;
        }
        PreviewSource::Demosaic => {
            derive_from_rgb(&chosen.bytes, chosen.width, chosen.height, data_dir, hash)
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
```

`thumbhash` is a real, user-visible column (`assets.thumbhash`, migration
`0008_asset_thumbhash.sql`) read by `keeppix-api`'s timeline route to render
blur-up placeholders. Because `run_with` exits early once the derivative
thumbnail file exists (the idempotency check at
`crates/keeppix-jobs/src/raw.rs:94-97`), a RAW asset that goes through this
job will **never** get its `thumbhash` populated on any subsequent run
either — it's not a transient gap that a retry fixes, it's permanent for
that content hash unless the derivative file is deleted out-of-band.

None of the four required tests would surface this: they check
`demosaic.calls()` and `derivative_paths(...).0.is_file()`, never the
`assets` row's `thumbhash` column. The existing end-to-end fixture test
(`crates/keeppix-jobs/tests/ingest_fixture.rs`) that does assert
`thumbhash IS NOT NULL` only exercises JPEGs, so it doesn't cover this path
either.

**Fix shape** (not prescriptive, but the minimal one that mirrors
`derive.rs`): have `derive_raw` return the `DeriveResult` (or just its
`thumbhash: Vec<u8>`) instead of `()`, and have `run_with` call
`assets.set_thumbhash_for_hash(hash, &result.thumbhash)` after a successful
derivation, skipping it only when `result.skipped` (matching the semantics
already used in `derive.rs::run`).

## Everything else — observations, not blockers

- **`enqueue_derive` routing** (`crates/keeppix-jobs/src/hash.rs`): correctly
  keyed off `AssetKind::RawImage`, which is the single domain discriminant
  covering all RAW containers (ARW/NEF/CR2/CR3/DNG/...), confirmed against
  `crates/keeppix-domain/src/asset.rs`. Separate dedup prefixes
  (`derive_raw:{hex}` vs `derive:{hex}`) avoid clobbering a
  `DeriveAsset` job enqueued for the same hash from another asset kind.
- **`no file for content hash` path**: `run_with` returns a `JobError`
  (not a per-asset `set_error`) when no on-disk file is found for any asset
  sharing the hash — this exactly mirrors the pre-existing `derive.rs::run`
  behavior, so it's consistent with the established pattern rather than a
  new inconsistency.
- **PPM parser** (`crates/keeppix-media/src/raw.rs::parse_ppm`/
  `read_ppm_uint`): hand-rolled, bounds-checked via `.get()`, rejects
  16-bit (`maxval > 255`) and non-`P6` input, and truncation is caught by
  the final `bytes.get(pos..pos+pixel_count)`. No panics on malformed
  input as far as static reading goes; consistent with the report's claim
  that a corrupt-file test exercises the real binary and gets a
  `RawError::Corrupt`, not a crash.
- **`derive_from_rgb`/`build_derivatives` extraction**: reasonable
  dedup of the resize/webp/thumbhash tail shared with `derive_from_bytes`;
  the two idempotency checks (`thumb.is_file()`) in `derive_from_bytes` and
  `derive_from_rgb` are now unreachable from the job's own call site
  (`run_with` already short-circuits earlier), but they're harmless
  defense-in-depth for other callers and not worth removing.
- **Timeout/RAM sizing** (30s CPU / 512MiB): generous vs. the spec's
  measured 1.5-4s, documented rationale in the ledger, consistent with
  existing `ffmpeg`/`ffprobe` sandbox sizing in `video.rs`. Fine as a
  ledger-documented ruling.
- Report claims full workspace suite green and `cargo deny check bans`
  clean; not independently re-run for this review per the read-only/
  no-full-suite instruction. No reason from the diff to doubt them, but
  they don't cover the thumbhash gap since it's a missing behavior, not a
  failing assertion.

## Recommendation

Fix the `set_thumbhash_for_hash` gap (and add a test asserting the asset's
`thumbhash` is non-empty after a `DeriveRaw` run, on both the
embedded-preview and demosaic-fallback branches) before merging. Everything
else in this task meets the brief, the spec, and the global invariants.
