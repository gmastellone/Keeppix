# Task 22 — Report

Not blocked. `derive_from_bytes`/`ensure_full_from_bytes` now decode PNG, TIFF,
WebP and HEIF (8+10 bit), all normalized to RGB8. PNG/TIFF are pure Rust
(`png`, `tiff` crates); WebP reuses the existing `webp` binding read-side.
HEIF uses `heif-convert` (from `libheif-examples`) run through the existing
`sandbox::run` (RLIMIT_AS/CPU) — not `libheif-rs`, which would bind libheif
in-process against the plan's sandboxing ruling.

10-bit HEIF confirmed, not assumed: `sample10.heic` is a real file
(`heif-enc -b 10`, `heif-info` shows `bit depth: 10`), and this VM already
had `libheif-plugin-libde265` installed, so the 10-bit test actually ran
(11/11 `derive_formats.rs` green) instead of skipping.

Dockerfile: added a `heif` build stage mirroring `libraw`'s, copying
`heif-convert` + libs into the distroless runtime. Debian bookworm bundles
codecs into `libheif1` directly (no plugin dir needed), unlike Ubuntu
24.04's `libheif-plugin-*` packages — CI installs those separately.

`cargo fmt`/`clippy -D warnings`/`cargo deny check` all clean (only
pre-existing informational duplicate-version warnings for zune-jpeg 0.4 vs
0.5, no errors). 4 commits on `fase-10`, ledger updated, no push.
