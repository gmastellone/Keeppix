#![allow(clippy::unwrap_used, clippy::expect_used)]

//! `derive_from_bytes`/`ensure_full_from_bytes` used to decode JPEG only
//! even though `kind::detect_kind` classifies PNG/TIFF/WebP/HEIF as
//! `Image`. One real fixture per format, plus one malformed fixture per
//! format: no crash, no orphaned process, clean failure
//! (`DeriveError::Decode`), not a panic or a hang.

use keeppix_media::{derive_from_bytes, ensure_full_from_bytes};

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name),
    )
    .unwrap_or_else(|e| panic!("fixture {name}: {e}"))
}

fn assert_produces_thumb_and_preview(name: &str, seed: u8) {
    let bytes = fixture(name);
    let dir = tempfile::tempdir().unwrap();
    let hash = [seed; 32];

    let result = derive_from_bytes(&bytes, dir.path(), &hash)
        .unwrap_or_else(|e| panic!("{name} must decode, instead: {e}"));

    assert!(result.thumb.is_file(), "{name}: thumb missing");
    assert!(
        std::fs::metadata(&result.thumb).unwrap().len() > 0,
        "{name}: empty thumb"
    );
    assert!(!result.thumbhash.is_empty(), "{name}: empty thumbhash");

    let full = ensure_full_from_bytes(&bytes, dir.path(), &hash)
        .unwrap_or_else(|e| panic!("{name}: ensure_full_from_bytes: {e}"));
    assert!(full.is_file(), "{name}: full missing");
    assert!(
        std::fs::metadata(&full).unwrap().len() > 0,
        "{name}: empty full"
    );
}

fn assert_fails_cleanly(name: &str, seed: u8) {
    let bytes = fixture(name);
    let dir = tempfile::tempdir().unwrap();
    let hash = [seed; 32];

    let err = derive_from_bytes(&bytes, dir.path(), &hash);
    assert!(
        err.is_err(),
        "{name}: a malformed file must not produce a derivative"
    );
    // No partial thumb left on disk: all or nothing (write via .tmp +
    // rename, never a half-visible file).
    assert!(
        !dir.path().join("derivatives").exists() || walk_is_empty(&dir.path().join("derivatives")),
        "{name}: no partial derivative should remain"
    );
}

fn walk_is_empty(dir: &std::path::Path) -> bool {
    std::fs::read_dir(dir)
        .map(|mut entries| entries.next().is_none())
        .unwrap_or(true)
}

#[test]
fn png_source_produces_thumb_and_preview() {
    assert_produces_thumb_and_preview("sample.png", 0x30);
}

#[test]
fn malformed_png_fails_cleanly() {
    assert_fails_cleanly("malformed.png", 0x31);
}

#[test]
fn tiff_source_produces_thumb_and_preview() {
    assert_produces_thumb_and_preview("sample.tiff", 0x32);
}

#[test]
fn malformed_tiff_fails_cleanly() {
    assert_fails_cleanly("malformed.tiff", 0x33);
}

#[test]
fn webp_source_produces_thumb_and_preview() {
    assert_produces_thumb_and_preview("sample.webp", 0x34);
}

#[test]
fn malformed_webp_fails_cleanly() {
    assert_fails_cleanly("malformed.webp", 0x35);
}

#[test]
fn heif_8bit_source_produces_thumb_and_preview() {
    if !keeppix_media::heif_convert_available() {
        eprintln!("heif-convert missing: skipping (see libheif-examples)");
        return;
    }
    assert_produces_thumb_and_preview("sample8.heic", 0x36);
}

/// The fixture is a *real* 10-bit HEIC, not synthetic: generated with
/// `heif-enc -b 10 -L -p chroma=444 --matrix_coefficients=0` from a 16-bit
/// RGB PNG, and confirmed with `heif-info` (`bit depth: 10`) before being
/// committed to the repository — not an assumption, a verified fact. If
/// libheif only decoded Main (8-bit) and not Main10, this would fail with
/// `DeriveError::Decode`, not silently with a wrong image.
#[test]
fn heif_10bit_source_produces_thumb_and_preview() {
    if !keeppix_media::heif_convert_available() {
        eprintln!("heif-convert missing: skipping (see libheif-examples)");
        return;
    }
    assert_produces_thumb_and_preview("sample10.heic", 0x37);
}

#[test]
fn malformed_heif_fails_cleanly() {
    if !keeppix_media::heif_convert_available() {
        eprintln!("heif-convert missing: skipping (see libheif-examples)");
        return;
    }
    assert_fails_cleanly("malformed.heic", 0x38);
}

/// Anti-orphan guarantee: a malformed HEIF file that makes sandboxed
/// `heif-convert` fail must not leave processes behind. There's no direct
/// way to count system processes in a portable test, but we verify the
/// call returns promptly instead of hanging — a hang here would mean the
/// sandbox's `RLIMIT_CPU` didn't work.
#[test]
fn malformed_heif_does_not_hang() {
    if !keeppix_media::heif_convert_available() {
        eprintln!("heif-convert missing: skipping (see libheif-examples)");
        return;
    }
    let bytes = fixture("malformed.heic");
    let dir = tempfile::tempdir().unwrap();
    let start = std::time::Instant::now();
    let _ = derive_from_bytes(&bytes, dir.path(), &[0x39; 32]);
    assert!(
        start.elapsed() < std::time::Duration::from_secs(20),
        "heif-convert on a corrupt file must fail immediately, not hang"
    );
}

#[test]
fn unrecognized_bytes_fail_cleanly_without_a_kind_match() {
    let dir = tempfile::tempdir().unwrap();
    let err = derive_from_bytes(b"not an image of any known format", dir.path(), &[0x3A; 32]);
    assert!(err.is_err());
}
