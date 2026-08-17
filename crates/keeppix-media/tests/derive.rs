#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::cast_possible_truncation,
    clippy::map_unwrap_or
)]

use keeppix_media::{
    derivative_paths, derive_from_bytes, derive_from_rgb, derive_jpeg, extract_embedded_preview,
    hash_file,
};

fn fixture(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

#[test]
fn deriving_from_bytes_matches_deriving_from_a_file() {
    let dir = tempfile::tempdir().unwrap();
    let src = fixture("tiny.jpg");
    let bytes = std::fs::read(&src).unwrap();
    let hash = [7u8; 32];

    let from_bytes = derive_from_bytes(&bytes, dir.path(), &hash).unwrap();

    let dir2 = tempfile::tempdir().unwrap();
    let from_file = derive_jpeg(&src, dir2.path(), &hash).unwrap();

    assert_eq!(
        from_bytes.thumbhash, from_file.thumbhash,
        "la stessa immagine deve produrre lo stesso thumbhash da entrambe le vie"
    );
    assert_eq!(
        std::fs::read(&from_bytes.thumb).unwrap().len(),
        std::fs::read(&from_file.thumb).unwrap().len()
    );
}

#[test]
fn derive_writes_thumb_and_leaves_original() {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tiny.jpg");
    let original = std::fs::read(&fixture).unwrap();
    let hash = hash_file(&fixture).unwrap();
    let data = std::env::temp_dir().join(format!("kpx-der-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&data);
    let first = derive_jpeg(&fixture, &data, &hash).unwrap();
    assert!(first.thumb.is_file());
    assert!(!first.skipped);
    let mtime = std::fs::metadata(&first.thumb).unwrap().modified().unwrap();
    let second = derive_jpeg(&fixture, &data, &hash).unwrap();
    assert!(second.skipped);
    let mtime2 = std::fs::metadata(&second.thumb)
        .unwrap()
        .modified()
        .unwrap();
    assert_eq!(mtime, mtime2);
    assert_eq!(std::fs::read(&fixture).unwrap(), original);
    let _ = std::fs::remove_dir_all(&data);
}

fn vm_hwm_kb() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .unwrap()
        .lines()
        .find_map(|l| l.strip_prefix("VmHWM:"))
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|n| n.parse().ok())
        .unwrap_or(0)
}

/// Misura di Task 1: tempo e RAM di picco della derivazione su JPEG reale
/// (preview incorporata di un ARW). Va eseguito da solo (`--exact`) perché
/// `VmHWM` è del processo. Prima e dopo il cambio di encoder.
#[test]
fn measure_derive_time_and_peak_rss() {
    let preview = extract_embedded_preview(&fixture("sample.arw"))
        .unwrap()
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let hash = [0x11u8; 32];
    let before = vm_hwm_kb();
    let start = std::time::Instant::now();
    let result = derive_from_bytes(&preview.bytes, dir.path(), &hash).unwrap();
    let elapsed = start.elapsed();
    let after = vm_hwm_kb();
    let preview_len = result
        .preview
        .as_ref()
        .map(|p| std::fs::metadata(p).unwrap().len())
        .unwrap_or(0);
    let thumb_len = std::fs::metadata(&result.thumb).unwrap().len();
    eprintln!(
        "MEASUREMENT derive ARW-embedded JPEG: {elapsed:?} VmHWM {before}k→{after}k Δ{}k thumb={thumb_len} preview={preview_len}",
        after.saturating_sub(before)
    );
}

fn noisy_rgb(w: u32, h: u32) -> Vec<u8> {
    let mut rgb = Vec::with_capacity((w * h * 3) as usize);
    for y in 0..h {
        for x in 0..w {
            rgb.push((x.wrapping_mul(13) % 256) as u8);
            rgb.push((y.wrapping_mul(7) % 256) as u8);
            rgb.push(((x + y).wrapping_mul(3) % 256) as u8);
        }
    }
    rgb
}

fn webp_chunk_at(bytes: &[u8]) -> &[u8] {
    assert!(bytes.len() >= 16, "webp troppo corto");
    assert_eq!(&bytes[0..4], b"RIFF");
    assert_eq!(&bytes[8..12], b"WEBP");
    &bytes[12..16]
}

fn lossless_webp(rgb: &[u8], w: u32, h: u32) -> Vec<u8> {
    let mut buf = Vec::new();
    image_webp::WebPEncoder::new(&mut buf)
        .encode(rgb, w, h, image_webp::ColorType::Rgb8)
        .unwrap();
    buf
}

fn fit(w: u32, h: u32, longest: u32) -> (u32, u32) {
    let m = w.max(h);
    if m <= longest {
        return (w.max(1), h.max(1));
    }
    let nw = ((u64::from(w) * u64::from(longest) / u64::from(m)).max(1)) as u32;
    let nh = ((u64::from(h) * u64::from(longest) / u64::from(m)).max(1)) as u32;
    (nw, nh)
}

fn resize_lanczos(rgb: &[u8], w: u32, h: u32, longest: u32) -> (Vec<u8>, u32, u32) {
    use fast_image_resize::images::Image;
    use fast_image_resize::{FilterType, PixelType, ResizeAlg, ResizeOptions, Resizer};
    let (nw, nh) = fit(w, h, longest);
    if nw == w && nh == h {
        return (rgb.to_vec(), w, h);
    }
    let src = Image::from_vec_u8(w, h, rgb.to_vec(), PixelType::U8x3).unwrap();
    let mut dst = Image::new(nw, nh, PixelType::U8x3);
    Resizer::new()
        .resize(
            &src,
            &mut dst,
            &ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::Lanczos3)),
        )
        .unwrap();
    (dst.buffer().to_vec(), nw, nh)
}

#[test]
fn derived_preview_uses_lossy_vp8_not_vp8l() {
    let dir = tempfile::tempdir().unwrap();
    let rgb = noisy_rgb(1920, 1080);
    let result = derive_from_rgb(&rgb, 1920, 1080, dir.path(), &[0x22u8; 32]).unwrap();
    let preview = result.preview.expect("1920px deve produrre una preview");
    let bytes = std::fs::read(preview).unwrap();
    let chunk = webp_chunk_at(&bytes);
    assert_eq!(
        chunk, b"VP8 ",
        "il derivato deve essere WebP con perdita (VP8), non lossless (VP8L); trovato {chunk:?}"
    );
    assert_ne!(chunk, b"VP8L");
}

#[test]
fn derived_preview_is_under_one_third_of_lossless() {
    // Foto reale, non un pattern ad alta frequenza: il 1/3 del piano è
    // calibrato su quello che si vede a schermo, non sul worst-case DCT.
    let jpeg = extract_embedded_preview(&fixture("sample.arw"))
        .unwrap()
        .unwrap();
    let mut decoder = zune_jpeg::JpegDecoder::new(&jpeg.bytes);
    decoder.decode_headers().unwrap();
    let info = decoder.info().unwrap();
    let rgb = decoder.decode().unwrap();
    let (prgb, pw, ph) = resize_lanczos(&rgb, u32::from(info.width), u32::from(info.height), 2048);
    let lossless = lossless_webp(&prgb, pw, ph);

    let dir = tempfile::tempdir().unwrap();
    let result = derive_from_bytes(&jpeg.bytes, dir.path(), &[0x23u8; 32]).unwrap();
    let preview = result.preview.expect("la preview Sony è oltre 1600 px");
    let lossy = std::fs::metadata(preview).unwrap().len();
    assert!(
        lossy * 3 < lossless.len() as u64,
        "preview lossy {lossy} deve pesare meno di 1/3 del lossless {} (soglia larga: regressione al VP8L)",
        lossless.len()
    );
}

#[test]
fn webp_quality_changes_the_preview_size() {
    let rgb = noisy_rgb(1920, 1080);
    unsafe { std::env::set_var("KEEPPIX_WEBP_QUALITY", "40") };
    let dir_lo = tempfile::tempdir().unwrap();
    let lo = derive_from_rgb(&rgb, 1920, 1080, dir_lo.path(), &[0x40u8; 32]).unwrap();
    let lo_len = std::fs::metadata(lo.preview.as_ref().unwrap())
        .unwrap()
        .len();

    unsafe { std::env::set_var("KEEPPIX_WEBP_QUALITY", "90") };
    let dir_hi = tempfile::tempdir().unwrap();
    let hi = derive_from_rgb(&rgb, 1920, 1080, dir_hi.path(), &[0x90u8; 32]).unwrap();
    let hi_len = std::fs::metadata(hi.preview.as_ref().unwrap())
        .unwrap()
        .len();

    unsafe { std::env::remove_var("KEEPPIX_WEBP_QUALITY") };

    assert!(
        lo_len < hi_len,
        "q40 ({lo_len}) deve pesare meno di q90 ({hi_len})"
    );
}

#[test]
fn derived_preview_long_side_is_2048() {
    let dir = tempfile::tempdir().unwrap();
    let rgb = noisy_rgb(3000, 2000);
    let result = derive_from_rgb(&rgb, 3000, 2000, dir.path(), &[0x20u8; 32]).unwrap();
    let preview = result.preview.expect("3000px deve produrre una preview");
    let bytes = std::fs::read(&preview).unwrap();
    let features = webp::BitstreamFeatures::new(&bytes).expect("preview WebP decodificabile");
    assert_eq!(
        features.width().max(features.height()),
        2048,
        "l'anteprima deve essere 2048 px sul lato lungo, non 1440"
    );

    let full = keeppix_media::full_derivative_path(dir.path(), &[0x20u8; 32]);
    assert!(
        !full.is_file(),
        "la derivazione iniziale non deve scrivere il livello full"
    );
}

#[test]
fn sample_arw_embedded_jpeg_does_not_beat_the_preview_budget() {
    let jpeg = extract_embedded_preview(&fixture("sample.arw"))
        .unwrap()
        .unwrap();
    assert!(
        !keeppix_media::embedded_usable_as_full(jpeg.width.max(jpeg.height)),
        "sample.arw is the field-test class (embedded {}x{}): full must demosaic, not re-encode this JPEG",
        jpeg.width,
        jpeg.height
    );
    assert!(!keeppix_media::embedded_usable_as_full(2048));
    assert!(keeppix_media::embedded_usable_as_full(2049));
}

#[test]
fn ensure_full_from_rgb_keeps_native_resolution() {
    let dir = tempfile::tempdir().unwrap();
    let hash = [0x23u8; 32];
    let width = 3000u32;
    let height = 2000u32;
    let rgb = vec![80u8; (width * height * 3) as usize];
    let path = keeppix_media::ensure_full_from_rgb(&rgb, width, height, dir.path(), &hash).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    let features = webp::BitstreamFeatures::new(&bytes).expect("full WebP");
    assert_eq!(features.width(), width);
    assert_eq!(features.height(), height);
}

#[test]
fn ensure_full_is_lazy_and_cached() {
    let dir = tempfile::tempdir().unwrap();
    let jpeg = extract_embedded_preview(&fixture("sample.arw"))
        .unwrap()
        .unwrap();
    let hash = [0x21u8; 32];
    derive_from_bytes(&jpeg.bytes, dir.path(), &hash).unwrap();
    let full = keeppix_media::full_derivative_path(dir.path(), &hash);
    assert!(!full.is_file(), "nessun full prima dello zoom");

    keeppix_media::ensure_full_from_bytes(&jpeg.bytes, dir.path(), &hash).unwrap();
    assert!(full.is_file());
    let first_len = std::fs::metadata(&full).unwrap().len();
    let first_mtime = std::fs::metadata(&full).unwrap().modified().unwrap();

    std::thread::sleep(std::time::Duration::from_millis(20));
    keeppix_media::ensure_full_from_bytes(&jpeg.bytes, dir.path(), &hash).unwrap();
    let second_mtime = std::fs::metadata(&full).unwrap().modified().unwrap();
    assert_eq!(first_len, std::fs::metadata(&full).unwrap().len());
    assert_eq!(
        first_mtime, second_mtime,
        "la seconda richiesta non deve rigenerare il file"
    );
}

#[test]
fn full_cache_evicts_oldest_when_over_cap() {
    let dir = tempfile::tempdir().unwrap();
    let jpeg = extract_embedded_preview(&fixture("sample.arw"))
        .unwrap()
        .unwrap();
    let a = [0xa1u8; 32];
    let b = [0xb1u8; 32];
    keeppix_media::ensure_full_from_bytes(&jpeg.bytes, dir.path(), &a).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));
    keeppix_media::ensure_full_from_bytes(&jpeg.bytes, dir.path(), &b).unwrap();
    let path_a = keeppix_media::full_derivative_path(dir.path(), &a);
    let path_b = keeppix_media::full_derivative_path(dir.path(), &b);
    let size_b = std::fs::metadata(&path_b).unwrap().len();

    keeppix_media::enforce_full_cache_cap(dir.path(), size_b).unwrap();
    assert!(
        !path_a.is_file(),
        "il full meno recente deve uscire quando si supera il tetto"
    );
    assert!(path_b.is_file(), "il più recente resta");
    let remaining: u64 = walk_full_bytes(dir.path());
    assert!(
        remaining <= size_b,
        "la cache full non deve superare il tetto ({remaining} > {size_b})"
    );
}

/// Il tetto non può percorrere `derivatives/` (thumb+preview di tutto
/// l'archivio). Un `*-full.webp` messo lì è un esca: se lo sfratto lo
/// tocca, sta ancora camminando l'albero sbagliato.
#[test]
fn enforce_full_cache_cap_does_not_walk_the_derivatives_tree() {
    let dir = tempfile::tempdir().unwrap();
    let jpeg = extract_embedded_preview(&fixture("sample.arw"))
        .unwrap()
        .unwrap();

    let decoy_dir = dir.path().join("derivatives").join("ff").join("ff");
    std::fs::create_dir_all(&decoy_dir).unwrap();
    let decoy = decoy_dir.join(format!("{}-full.webp", "ff".repeat(32)));
    std::fs::write(&decoy, vec![0u8; 80_000]).unwrap();

    for i in 0..80u8 {
        let hash = [i; 32];
        let (thumb, preview) = derivative_paths(dir.path(), &hash);
        std::fs::create_dir_all(thumb.parent().unwrap()).unwrap();
        std::fs::write(&thumb, [1u8; 32]).unwrap();
        std::fs::write(&preview, [1u8; 32]).unwrap();
    }

    let a = [0xa2u8; 32];
    let b = [0xb2u8; 32];
    keeppix_media::ensure_full_from_bytes(&jpeg.bytes, dir.path(), &a).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));
    keeppix_media::ensure_full_from_bytes(&jpeg.bytes, dir.path(), &b).unwrap();
    let size_b = std::fs::metadata(&keeppix_media::full_derivative_path(dir.path(), &b))
        .unwrap()
        .len();

    keeppix_media::enforce_full_cache_cap(dir.path(), size_b).unwrap();
    assert!(
        decoy.is_file(),
        "un *-full.webp sotto derivatives/ non fa parte della cache full"
    );
}

fn walk_full_bytes(data_dir: &std::path::Path) -> u64 {
    let mut total = 0;
    let mut stack = vec![data_dir.join("full")];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with("-full.webp"))
            {
                total += entry.metadata().map(|m| m.len()).unwrap_or(0);
            }
        }
    }
    total
}
