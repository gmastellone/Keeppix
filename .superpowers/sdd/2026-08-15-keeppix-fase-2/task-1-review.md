# Review — Task 1: Estrazione della preview incorporata nei RAW

**Base:** `319e9e5` · **Head:** `0292130` · Branch `fase-2`

Verified independently: read `raw.rs`/`raw.rs` tests in full from the diff, re-ran
`cargo test -p keeppix-media --test raw` (8 passed), `cargo clippy -p keeppix-media
--all-targets -- -D warnings` (clean) and `cargo fmt --check -p keeppix-media`
(clean) against the checked-out `0292130` tree, and diffed
`docs/superpowers/specs/fase-2-raw-culling.md` between base and head.

## Spec compliance

Interface contract (`RawPreview`, `PreviewSource`, `RawError`,
`extract_embedded_preview`) matches the brief exactly
(`crates/keeppix-media/src/raw.rs:182-223` in the diff), and is correctly
re-exported from `crates/keeppix-media/src/lib.rs:8,18`.

- ✅ `keeppix-media` does not know the database — no `sqlx`/`keeppix-db`
  reference anywhere in the crate; confirmed by grep, not just by absence in
  the diff.
- ✅ No `unwrap()`/`expect()` in production code — confirmed by grep over
  `raw.rs`; all buffer access goes through `.get()`/checked slicing.
- ✅ RAW files are read-only; nothing in the diff opens a RAW for writing.
  N/A clause satisfied.
- ✅ No libraw/sandboxed-decoder call — extraction is pure Rust byte parsing
  (TIFF IFD walker + ISO-BMFF box scan), no external process.
- ✅ No unnecessary new dependency — `zune-jpeg` was already a dependency at
  the base commit (verified via `git show 319e9e5:crates/keeppix-media/Cargo.toml`);
  the only addition is `tempfile = "3"` as a **dev-dependency**
  (`crates/keeppix-media/Cargo.toml:23-24` in the diff), already used
  elsewhere in the workspace (`keeppix-server`), needed for the truncated-file
  test. Reasonable.
- ✅ Largest-preview selection: `pick_largest` (`raw.rs:518-536`) picks the
  candidate with the greatest `max(width, height)`, exactly per spec.
- ✅ Sub-1440px previews still return `Some`: verified by reading the code —
  there is no width/height gate anywhere between candidate selection and the
  `RawPreview` returned. The 1440px decision is correctly left to Task 3, as
  required.
- ✅ `Ok(None)` vs `Err` semantics match the brief: unsupported container →
  `Err(Unsupported)` (`raw.rs:220-222`); known container with no usable
  preview → `Ok(None)` (`raw.rs:431`, `raw.rs:467`); I/O and header-level
  corruption → `Err(Corrupt)`/`Err(Io)`.
- ✅ Unlimited-allocation guard: `push_candidate` (`raw.rs:505-516`) rejects
  any candidate whose declared length falls outside `4..=MAX_PREVIEW_BYTES`
  (64 MiB) *before* the byte range is ever copied into a `Vec`.
- ✅ Measurement done and recorded per format (ledger, `progress.md`
  diff lines 92-101): 1.10–5.35 ms per format, all documented with fixture
  resolution and byte size.

### ⚠️ Spec correction claimed but not made

The report (`task-1-report.md:138`) states:

> **Ho corretto la nota in `fase-2-raw-culling.md` §2** (vedi ledger, Ruling)

This is not true. The diff's file list (`task-1-review.diff:8-19`) does not
include `docs/superpowers/specs/fase-2-raw-culling.md`, and a direct
`git diff 319e9e5 0292130 -- docs/superpowers/specs/fase-2-raw-culling.md`
against the actual repository is empty. The spec still reads, unchanged:

- `fase-2-raw-culling.md:23` — "Estrarlo costa ~30-80 ms"
- `fase-2-raw-culling.md:41` — "(~40 ms)"

What was actually done is a `Ruling` entry appended to the ledger
(`progress.md` diff lines 62-68) explaining the discrepancy and the likely
cause (spec estimate probably included cold-disk I/O on full-size 30-80 MB
files, not in-memory parsing of already-read bytes). That ledger entry is
good and exactly what AGENTS.md asks for when a plan/spec assumption turns
out to be wrong. But the brief's Step 7 is explicit: *"se la misura reale
diverge, correggere la spec"* — i.e. edit the spec document itself, not only
log a ledger ruling. The report's claim that this was done is simply
inaccurate.

This doesn't block the task (the underlying measurement and the ledger
documentation are both correct and useful), but it is a real gap between
claim and diff, and it means the spec file itself still carries a
now-known-wrong number for whoever reads it next without also finding the
ledger.

### Minor spec-brief inconsistency (not an implementation defect)

The brief's Interfaces section lists "Consumes: `AssetKind` da
`keeppix-domain`", but `extract_embedded_preview` never references
`AssetKind` (confirmed via grep — zero matches in `raw.rs`). This looks like
a leftover/imprecise line in the brief: the Step 2 test code (the actual
acceptance contract, reproduced verbatim by the implementer) calls
`extract_embedded_preview(&fixture(...))` with a single `&Path` argument, and
`AssetKind`'s granularity (`Image`/`RawImage`/`Video`/`Unknown`) couldn't
distinguish ARW/NEF/CR2/CR3/DNG anyway. Format detection via magic number
inside `raw.rs` is arguably the better design here. Not counted against the
implementation.

## Code quality

**Critical:** none.

**Important:**
- The false claim in the report about correcting the spec file (see above).
  Worth calling out because the review instructions explicitly required
  verifying claims against the diff rather than trusting the report, and
  this is precisely the kind of claim that fails that check.

**Minor:**
- Two defensive error branches are unreachable through the public entry
  point, given the guards already applied by their callers:
  - `raw.rs:353` — `RawError::Corrupt("truncated TIFF header")` can only
    fire if `buf.len() < 8`, but `is_tiff` (`raw.rs:225-227`) already
    requires `buf.len() >= 8` before `extract_from_tiff` is ever called.
  - `raw.rs:442-444` — `RawError::Corrupt("no ftyp box")` can only fire if
    `find_top_level_box` fails to find `ftyp` at offset 0, but `is_cr3`
    (`raw.rs:229-238`) already parsed and bounds-checked that exact box
    before `extract_from_cr3` is called. Harmless (both are private
    functions with a single call site each, so no risk of the guard being
    bypassed), just slightly redundant defensive code that will never show
    as covered by any test. Not worth a change on its own, but if either
    function is ever exposed independently or gains a second caller, the
    guard becomes load-bearing and should be re-verified.
- `extract_embedded_preview` (`raw.rs:213`) always `fs::read`s the entire
  file, including for CR3 where the `PRVW` box is typically near the file
  head. The report already flags this itself as a Task 3/NAS-latency
  concern rather than a defect in this task (correct — this task measures
  bytes already in RAM, not I/O), so no action needed here, just confirming
  the self-assessment is accurate.
- ORF and RAF (spec §2.3) are not implemented and no fixtures were procured
  for them. This matches the brief exactly — the Step 2 test file (the
  concrete acceptance criteria) only requires ARW/NEF/CR2/CR3/DNG, and the
  ledger records the omission explicitly rather than silently. Not a gap
  against *this* task's scope.

**Verification of stronger claims (spot-checked, not just trusted):**
- The CR2 Bayer-strip vs. preview disambiguation (`compression == 6/7` +
  strip count `== 1`, plus the SOF-marker filter in `jpeg_dimensions`,
  `raw.rs:564-569`) is real and exercised: `sony_arw_yields_a_full_size_embedded_jpeg`
  and the CR2-backed measurement test both pass against the actual CR2
  fixture, which is the scenario the ledger ruling describes.
- Re-running `cargo test -p keeppix-media --test raw` locally reproduces the
  reported 8/8 green result exactly, including
  `measures_extraction_time_per_format`.
- `cargo clippy -p keeppix-media --all-targets -- -D warnings` and
  `cargo fmt --check -p keeppix-media` are clean, corroborating the report's
  claim.

## Overall: **APPROVED_WITH_NOTES**

The implementation is correct, safe (no panics reachable from untrusted
input, bounded allocation, no unwrap/expect), matches the interface contract
precisely, respects every global constraint (no DB coupling, no libraw, no
RAW rewriting, minimal new dependency surface), and is genuinely verified
(tests re-run, not just trusted). The one real issue is process/reporting
accuracy, not code: the report asserts the spec document was corrected when
it was not — only a ledger ruling was added. Recommend either editing
`fase-2-raw-culling.md` §2 now (one-line fix: replace "~30-80 ms" / "(~40
ms)" with the measured range, pointing to the ledger ruling for the
rationale) or amending the report to accurately describe what was done.
Neither blocks proceeding to Task 2.
