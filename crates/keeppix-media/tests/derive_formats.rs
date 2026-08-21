#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Task 22: `derive_from_bytes`/`ensure_full_from_bytes` decodificavano solo
//! JPEG anche se `kind::detect_kind` classifica PNG/TIFF/WebP/HEIF come
//! `Image` fin dalla Fase 1b. Un fixture reale per formato, più uno
//! malformato per formato: nessun crash, nessun processo orfano, fallimento
//! pulito (`DeriveError::Decode`), non un panic o un hang.

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
        .unwrap_or_else(|e| panic!("{name} deve decodificare, invece: {e}"));

    assert!(result.thumb.is_file(), "{name}: manca il thumb");
    assert!(
        std::fs::metadata(&result.thumb).unwrap().len() > 0,
        "{name}: thumb vuoto"
    );
    assert!(!result.thumbhash.is_empty(), "{name}: thumbhash vuoto");

    let full = ensure_full_from_bytes(&bytes, dir.path(), &hash)
        .unwrap_or_else(|e| panic!("{name}: ensure_full_from_bytes: {e}"));
    assert!(full.is_file(), "{name}: manca il full");
    assert!(
        std::fs::metadata(&full).unwrap().len() > 0,
        "{name}: full vuoto"
    );
}

fn assert_fails_cleanly(name: &str, seed: u8) {
    let bytes = fixture(name);
    let dir = tempfile::tempdir().unwrap();
    let hash = [seed; 32];

    let err = derive_from_bytes(&bytes, dir.path(), &hash);
    assert!(
        err.is_err(),
        "{name}: un file malformato non deve produrre un derivato"
    );
    // Nessun thumb parziale lasciato sul disco: o tutto o niente (write via
    // .tmp + rename, mai un file visibile a metà).
    assert!(
        !dir.path().join("derivatives").exists() || walk_is_empty(&dir.path().join("derivatives")),
        "{name}: non deve restare un derivato parziale"
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
        eprintln!("heif-convert assente: salto (vedi libheif-examples)");
        return;
    }
    assert_produces_thumb_and_preview("sample8.heic", 0x36);
}

/// Il fixture è un HEIC 10-bit *reale*, non sintetico: generato con
/// `heif-enc -b 10 -L -p chroma=444 --matrix_coefficients=0` da un PNG RGB a
/// 16 bit, e confermato con `heif-info` (`bit depth: 10`) prima di essere
/// copiato nel repository — non un'assunzione, un fatto verificato. Se
/// libheif decodesse solo Main (8 bit) e non Main10, questo fallirebbe con
/// `DeriveError::Decode`, non silenziosamente con un'immagine sbagliata.
#[test]
fn heif_10bit_source_produces_thumb_and_preview() {
    if !keeppix_media::heif_convert_available() {
        eprintln!("heif-convert assente: salto (vedi libheif-examples)");
        return;
    }
    assert_produces_thumb_and_preview("sample10.heic", 0x37);
}

#[test]
fn malformed_heif_fails_cleanly() {
    if !keeppix_media::heif_convert_available() {
        eprintln!("heif-convert assente: salto (vedi libheif-examples)");
        return;
    }
    assert_fails_cleanly("malformed.heic", 0x38);
}

/// Garanzia anti-orfano: un file HEIF malformato che fa fallire
/// `heif-convert` in sandbox non deve lasciare processi in giro. Non c'è un
/// modo diretto per contare i processi del sistema in un test portabile, ma
/// si verifica che la chiamata ritorni in tempo utile invece di bloccarsi —
/// un hang qui vorrebbe dire che l'`RLIMIT_CPU` della sandbox non ha
/// funzionato.
#[test]
fn malformed_heif_does_not_hang() {
    if !keeppix_media::heif_convert_available() {
        eprintln!("heif-convert assente: salto (vedi libheif-examples)");
        return;
    }
    let bytes = fixture("malformed.heic");
    let dir = tempfile::tempdir().unwrap();
    let start = std::time::Instant::now();
    let _ = derive_from_bytes(&bytes, dir.path(), &[0x39; 32]);
    assert!(
        start.elapsed() < std::time::Duration::from_secs(20),
        "heif-convert su un file corrotto deve fallire subito, non restare appeso"
    );
}

#[test]
fn unrecognized_bytes_fail_cleanly_without_a_kind_match() {
    let dir = tempfile::tempdir().unwrap();
    let err = derive_from_bytes(b"not an image of any known format", dir.path(), &[0x3A; 32]);
    assert!(err.is_err());
}
