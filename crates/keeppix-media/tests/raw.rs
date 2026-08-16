use std::path::Path;

use keeppix_media::raw::{PreviewSource, extract_embedded_preview};

fn fixture(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

#[test]
#[allow(clippy::unwrap_used)]
fn sony_arw_yields_a_full_size_embedded_jpeg() {
    let preview = extract_embedded_preview(&fixture("sample.arw"))
        .unwrap()
        .unwrap();

    assert_eq!(preview.source, PreviewSource::Embedded);
    // Un JPEG valido inizia con SOI.
    assert_eq!(&preview.bytes[..2], &[0xFF, 0xD8]);
    assert!(
        preview.width >= 1440,
        "Sony incorpora una preview grande: {}",
        preview.width
    );
}

#[test]
#[allow(clippy::unwrap_used)]
fn canon_cr3_yields_the_prvw_box() {
    let preview = extract_embedded_preview(&fixture("sample.cr3"))
        .unwrap()
        .unwrap();
    assert_eq!(&preview.bytes[..2], &[0xFF, 0xD8]);
    // CR3 espone una preview più piccola delle altre: ~1620 px.
    assert!(preview.width >= 1024);
}

#[test]
#[allow(clippy::unwrap_used)]
fn nikon_nef_yields_a_preview() {
    let preview = extract_embedded_preview(&fixture("sample.nef"))
        .unwrap()
        .unwrap();
    assert_eq!(&preview.bytes[..2], &[0xFF, 0xD8]);
}

#[test]
#[allow(clippy::unwrap_used)]
fn dng_yields_a_preview() {
    let preview = extract_embedded_preview(&fixture("sample.dng"))
        .unwrap()
        .unwrap();
    assert_eq!(&preview.bytes[..2], &[0xFF, 0xD8]);
}

#[test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
fn the_extracted_bytes_decode_as_a_real_image() {
    // Non basta che inizi con SOI: deve essere decodificabile davvero,
    // altrimenti il derivato fallirebbe più a valle con un errore oscuro.
    let preview = extract_embedded_preview(&fixture("sample.arw"))
        .unwrap()
        .unwrap();
    let mut decoder = zune_jpeg::JpegDecoder::new(&preview.bytes);
    decoder
        .decode_headers()
        .expect("la preview è un JPEG decodificabile");
    let info = decoder.info().expect("dimensioni leggibili");
    assert_eq!(
        u32::from(info.width),
        preview.width,
        "le dimensioni dichiarate combaciano"
    );
}

#[test]
#[allow(clippy::unwrap_used)]
fn a_truncated_raw_is_an_error_not_a_panic() {
    let dir = tempfile::tempdir().unwrap();
    let truncated = dir.path().join("broken.arw");
    // Primi 512 byte di un ARW valido: header presente, corpo assente.
    let full = std::fs::read(fixture("sample.arw")).unwrap();
    std::fs::write(&truncated, &full[..512.min(full.len())]).unwrap();

    let result = extract_embedded_preview(&truncated);
    assert!(
        result.is_err() || result.unwrap().is_none(),
        "mai un panico su file corrotto"
    );
}

#[test]
#[allow(clippy::unwrap_used)]
fn measures_extraction_time_per_format() {
    // Non un'asserzione: un log per il ledger. `cargo test --release --
    // --nocapture` lo stampa. Vedi Step 7 del piano.
    for name in [
        "sample.arw",
        "sample.nef",
        "sample.cr2",
        "sample.cr3",
        "sample.dng",
    ] {
        let path = fixture(name);
        let start = std::time::Instant::now();
        let preview = extract_embedded_preview(&path).unwrap();
        let elapsed = start.elapsed();
        match preview {
            Some(p) => println!(
                "{name}: {:.2}ms, {}x{} ({} bytes)",
                elapsed.as_secs_f64() * 1000.0,
                p.width,
                p.height,
                p.bytes.len()
            ),
            None => println!(
                "{name}: {:.2}ms, no usable preview",
                elapsed.as_secs_f64() * 1000.0
            ),
        }
    }
}

#[test]
#[allow(clippy::unwrap_used)]
fn a_non_raw_file_is_unsupported_not_a_crash() {
    let dir = tempfile::tempdir().unwrap();
    let text = dir.path().join("nota.txt");
    std::fs::write(&text, b"non sono un raw").unwrap();

    assert!(extract_embedded_preview(&text).is_err());
}
