use std::path::Path;

use keeppix_media::raw::{
    PreviewSource, dcraw_emu_available, demosaic_half, extract_embedded_preview,
};

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
    // Log per il ledger; l'asserzione di budget è in
    // `embedded_preview_extraction_stays_within_budget`.
    for name in raw_fixture_names() {
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

fn raw_fixture_names() -> [&'static str; 5] {
    [
        "sample.arw",
        "sample.nef",
        "sample.cr2",
        "sample.cr3",
        "sample.dng",
    ]
}

/// Budget Fase 2R Task 8: < 50 ms per file su Pi 5 (release). In debug
/// `sample.cr3` può superare 50 ms (~80 ms misurati); 100 ms cattura
/// regressioni di ordine di grandezza senza falsi positivi in CI.
#[test]
#[allow(clippy::unwrap_used)]
fn embedded_preview_extraction_stays_within_budget() {
    let budget_ms = if cfg!(debug_assertions) { 100 } else { 50 };
    let budget = std::time::Duration::from_millis(budget_ms);

    for name in raw_fixture_names() {
        let path = fixture(name);
        if !path.is_file() {
            eprintln!("skip {name}: fixture missing at {}", path.display());
            continue;
        }
        let start = std::time::Instant::now();
        let preview = extract_embedded_preview(&path).unwrap();
        let elapsed = start.elapsed();
        eprintln!(
            "MEASUREMENT RAW preview {name}: {:.2}ms (budget {budget_ms}ms)",
            elapsed.as_secs_f64() * 1000.0
        );
        assert!(
            preview.is_some(),
            "{name} deve restituire una preview incorporata per il budget"
        );
        assert!(
            elapsed < budget,
            "estrazione preview {name}: {elapsed:?} (budget {budget_ms} ms)"
        );
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

/// Il percorso di produzione (Task 3): `dcraw_emu` gira davvero in sandbox e
/// produce pixel RGB8 utilizzabili. Serve a non fidarsi solo del mock usato
/// nei test del job — se il parsing del PPM si rompe, questo test lo vede.
#[test]
#[allow(clippy::unwrap_used)]
fn demosaic_half_produces_usable_half_size_rgb() {
    if !dcraw_emu_available() {
        eprintln!("skipping: dcraw_emu not in PATH");
        return;
    }
    let preview = demosaic_half(&fixture("sample.arw"), 512 * 1024 * 1024, 30).unwrap();
    assert_eq!(preview.source, PreviewSource::Demosaic);
    assert!(preview.width > 0 && preview.height > 0);
    assert_eq!(
        preview.bytes.len(),
        preview.width as usize * preview.height as usize * 3,
        "un pixel RGB8 per componente, niente header residuo"
    );
}

#[test]
#[allow(clippy::unwrap_used)]
fn demosaic_half_on_a_corrupt_file_is_an_error_not_a_panic() {
    if !dcraw_emu_available() {
        eprintln!("skipping: dcraw_emu not in PATH");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let garbage = dir.path().join("garbage.raw");
    std::fs::write(&garbage, b"not a raw file at all").unwrap();

    assert!(demosaic_half(&garbage, 512 * 1024 * 1024, 30).is_err());
}
