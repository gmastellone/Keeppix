use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use std::sync::atomic::{AtomicU8, Ordering};

use fast_image_resize::images::Image;
use fast_image_resize::{FilterType, PixelType, ResizeAlg, ResizeOptions, Resizer};
use thiserror::Error;
use webp::Encoder as WebPEncoder;
use zune_jpeg::JpegDecoder;

const THUMB: u32 = 240;
const PREVIEW: u32 = 2048;
/// Tetto della cache `full` pigra. ~200-300 zoom da 1,5-2,5 MB: una sessione
/// di culling, non l'archivio. Senza tetto è il cestino in un'altra forma.
pub const DEFAULT_FULL_CACHE_BYTES: u64 = 512 * 1024 * 1024;
/// Default della qualità WebP con perdita. Sotto 75 si vede; sopra 88
/// si paga per una differenza invisibile. Sovrascrivibile con
/// [`set_webp_quality`] / `KEEPPIX_WEBP_QUALITY`.
pub const DEFAULT_WEBP_QUALITY: u8 = 82;
static WEBP_QUALITY: AtomicU8 = AtomicU8::new(DEFAULT_WEBP_QUALITY);
const MAX_PIXELS: u64 = 200_000_000;
const SKIP_PREVIEW_PX: u32 = 1600;
const SKIP_PREVIEW_BYTES: u64 = 400 * 1024;

#[derive(Debug, Error)]
pub enum DeriveError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("decode: {0}")]
    Decode(String),
    #[error("image exceeds 200 megapixels")]
    TooManyPixels,
}

#[derive(Debug, Clone)]
pub struct DeriveResult {
    pub thumb: PathBuf,
    pub preview: Option<PathBuf>,
    pub thumbhash: Vec<u8>,
    pub skipped: bool,
}

/// Qualità di encoding WebP (1–100). Chiamato all'avvio da `Config`.
pub fn set_webp_quality(quality: u8) {
    WEBP_QUALITY.store(quality.clamp(1, 100), Ordering::Relaxed);
}

fn webp_quality() -> u8 {
    std::env::var("KEEPPIX_WEBP_QUALITY")
        .ok()
        .and_then(|s| s.parse().ok())
        .filter(|q: &u8| (1..=100).contains(q))
        .unwrap_or_else(|| WEBP_QUALITY.load(Ordering::Relaxed))
}

/// Una decodifica, write su `.tmp`, `rename`. Idempotente se i file ci sono già.
///
/// # Errors
/// I/O, JPEG illeggibile, o immagine oltre 200 MP.
pub fn derive_jpeg(
    src: &Path,
    data_dir: &Path,
    hash: &[u8; 32],
) -> Result<DeriveResult, DeriveError> {
    let (thumb, preview) = derivative_paths(data_dir, hash);
    if thumb.is_file() {
        let preview = preview.is_file().then_some(preview);
        return Ok(DeriveResult {
            thumb,
            preview,
            thumbhash: Vec::new(),
            skipped: true,
        });
    }
    derive_from_bytes(&fs::read(src)?, data_dir, hash)
}

/// Stessa pipeline di [`derive_jpeg`], ma i byte JPEG sono già in memoria.
///
/// # Errors
/// I/O, JPEG illeggibile, o immagine oltre 200 MP.
pub fn derive_from_bytes(
    bytes: &[u8],
    data_dir: &Path,
    hash: &[u8; 32],
) -> Result<DeriveResult, DeriveError> {
    let (thumb, preview) = derivative_paths(data_dir, hash);
    if thumb.is_file() {
        let preview = preview.is_file().then_some(preview);
        return Ok(DeriveResult {
            thumb,
            preview,
            thumbhash: Vec::new(),
            skipped: true,
        });
    }

    let mut decoder = JpegDecoder::new(bytes);
    decoder
        .decode_headers()
        .map_err(|e| DeriveError::Decode(e.to_string()))?;
    let info = decoder
        .info()
        .ok_or_else(|| DeriveError::Decode("missing jpeg info".to_owned()))?;
    let width = u32::from(info.width);
    let height = u32::from(info.height);
    if u64::from(width).saturating_mul(u64::from(height)) > MAX_PIXELS {
        return Err(DeriveError::TooManyPixels);
    }
    let rgb = decoder
        .decode()
        .map_err(|e| DeriveError::Decode(e.to_string()))?;

    let skip_preview = width.max(height) <= SKIP_PREVIEW_PX
        && u64::try_from(bytes.len()).unwrap_or(u64::MAX) <= SKIP_PREVIEW_BYTES;
    build_derivatives(&rgb, width, height, skip_preview, &thumb, &preview)
}

/// Come [`derive_from_bytes`], ma per pixel RGB8 già decodificati — l'uscita
/// del demosaic RAW (Task 3 Fase 2), che non è un JPEG e non passa dallo
/// `JpegDecoder`. Nessuna soglia sui byte sorgente per lo skip della preview:
/// non esiste un "peso del file originale" per pixel già decodificati, solo
/// le dimensioni contano.
///
/// # Errors
/// I/O, o immagine oltre 200 MP.
pub fn derive_from_rgb(
    rgb: &[u8],
    width: u32,
    height: u32,
    data_dir: &Path,
    hash: &[u8; 32],
) -> Result<DeriveResult, DeriveError> {
    let (thumb, preview) = derivative_paths(data_dir, hash);
    if thumb.is_file() {
        let preview = preview.is_file().then_some(preview);
        return Ok(DeriveResult {
            thumb,
            preview,
            thumbhash: Vec::new(),
            skipped: true,
        });
    }
    if u64::from(width).saturating_mul(u64::from(height)) > MAX_PIXELS {
        return Err(DeriveError::TooManyPixels);
    }
    let skip_preview = width.max(height) <= SKIP_PREVIEW_PX;
    build_derivatives(rgb, width, height, skip_preview, &thumb, &preview)
}

/// Coda condivisa da [`derive_from_bytes`] e [`derive_from_rgb`]: resize,
/// scrittura webp atomica dei derivati, thumbhash. Il chiamante ha già
/// gestito idempotenza e limite di pixel.
fn build_derivatives(
    rgb: &[u8],
    width: u32,
    height: u32,
    skip_preview: bool,
    thumb: &Path,
    preview: &Path,
) -> Result<DeriveResult, DeriveError> {
    if let Some(parent) = thumb.parent() {
        fs::create_dir_all(parent)?;
    }

    let thumb_rgb = resize_rgb(rgb, width, height, THUMB)?;
    write_webp_atomic(thumb, &thumb_rgb.pixels, thumb_rgb.width, thumb_rgb.height)?;
    // Il livello `full` non si scrive qui: è pigro, alla prima richiesta di zoom.

    let preview_path = if skip_preview {
        None
    } else {
        let p = resize_rgb(rgb, width, height, PREVIEW)?;
        write_webp_atomic(preview, &p.pixels, p.width, p.height)?;
        Some(preview.to_path_buf())
    };

    let hash_src = resize_rgb(rgb, width, height, 100)?;
    let rgba = rgb_to_rgba(&hash_src.pixels);
    let thumbhash = thumbhash::rgba_to_thumb_hash(
        usize::try_from(hash_src.width).unwrap_or(1),
        usize::try_from(hash_src.height).unwrap_or(1),
        &rgba,
    );

    Ok(DeriveResult {
        thumb: thumb.to_path_buf(),
        preview: preview_path,
        thumbhash,
        skipped: false,
    })
}

struct RgbImg {
    pixels: Vec<u8>,
    width: u32,
    height: u32,
}

fn resize_rgb(rgb: &[u8], width: u32, height: u32, longest: u32) -> Result<RgbImg, DeriveError> {
    let (nw, nh) = fit(width, height, longest);
    if nw == width && nh == height {
        return Ok(RgbImg {
            pixels: rgb.to_vec(),
            width,
            height,
        });
    }
    let src = Image::from_vec_u8(width, height, rgb.to_vec(), PixelType::U8x3)
        .map_err(|e| DeriveError::Decode(e.to_string()))?;
    let mut dst = Image::new(nw, nh, PixelType::U8x3);
    let mut resizer = Resizer::new();
    resizer
        .resize(
            &src,
            &mut dst,
            &ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::Lanczos3)),
        )
        .map_err(|e| DeriveError::Decode(e.to_string()))?;
    Ok(RgbImg {
        pixels: dst.buffer().to_vec(),
        width: nw,
        height: nh,
    })
}

fn fit(w: u32, h: u32, longest: u32) -> (u32, u32) {
    let m = w.max(h);
    if m <= longest {
        return (w.max(1), h.max(1));
    }
    let nw = u32::try_from((u64::from(w) * u64::from(longest) / u64::from(m)).max(1)).unwrap_or(1);
    let nh = u32::try_from((u64::from(h) * u64::from(longest) / u64::from(m)).max(1)).unwrap_or(1);
    (nw, nh)
}

fn rgb_to_rgba(rgb: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(rgb.len() / 3 * 4);
    for chunk in rgb.chunks_exact(3) {
        out.extend_from_slice(&[chunk[0], chunk[1], chunk[2], 255]);
    }
    out
}

fn write_webp_atomic(path: &Path, rgb: &[u8], w: u32, h: u32) -> Result<(), DeriveError> {
    let tmp = path.with_extension(format!("webp.{}.tmp", std::process::id()));
    let encoded = WebPEncoder::from_rgb(rgb, w, h).encode(f32::from(webp_quality()));
    if encoded.is_empty() {
        return Err(DeriveError::Decode("webp encode returned empty".to_owned()));
    }
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(&encoded)?;
    }
    fs::rename(tmp, path)?;
    Ok(())
}

fn derivative_dir(data_dir: &Path, hash: &[u8; 32]) -> (String, PathBuf) {
    let hex = hex32(hash);
    let dir = data_dir
        .join("derivatives")
        .join(&hex[0..2])
        .join(&hex[2..4]);
    (hex, dir)
}

#[must_use]
pub fn derivative_paths(data_dir: &Path, hash: &[u8; 32]) -> (PathBuf, PathBuf) {
    let (hex, dir) = derivative_dir(data_dir, hash);
    (
        dir.join(format!("{hex}-thumb.webp")),
        dir.join(format!("{hex}-preview.webp")),
    )
}

#[must_use]
pub fn full_derivative_path(data_dir: &Path, hash: &[u8; 32]) -> PathBuf {
    let (hex, dir) = derivative_dir(data_dir, hash);
    dir.join(format!("{hex}-full.webp"))
}

/// Scrive il WebP a piena risoluzione se manca. Idempotente: se il file c'è
/// già non si ricodifica (si aggiorna solo l'atime per lo LRU).
///
/// # Errors
/// I/O, JPEG illeggibile, o immagine oltre 200 MP.
pub fn ensure_full_from_bytes(
    bytes: &[u8],
    data_dir: &Path,
    hash: &[u8; 32],
) -> Result<PathBuf, DeriveError> {
    let path = full_derivative_path(data_dir, hash);
    if path.is_file() {
        touch_accessed(&path)?;
        return Ok(path);
    }

    let mut decoder = JpegDecoder::new(bytes);
    decoder
        .decode_headers()
        .map_err(|e| DeriveError::Decode(e.to_string()))?;
    let info = decoder
        .info()
        .ok_or_else(|| DeriveError::Decode("missing jpeg info".to_owned()))?;
    let width = u32::from(info.width);
    let height = u32::from(info.height);
    if u64::from(width).saturating_mul(u64::from(height)) > MAX_PIXELS {
        return Err(DeriveError::TooManyPixels);
    }
    let rgb = decoder
        .decode()
        .map_err(|e| DeriveError::Decode(e.to_string()))?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    write_webp_atomic(&path, &rgb, width, height)?;
    Ok(path)
}

fn touch_accessed(path: &Path) -> Result<(), DeriveError> {
    let file = fs::File::open(path)?;
    let times = fs::FileTimes::new().set_accessed(std::time::SystemTime::now());
    file.set_times(times)?;
    Ok(())
}

/// Sfratta i `*-full.webp` meno usati di recente finché il totale sta nel tetto.
///
/// # Errors
/// I/O sulla directory dei derivati.
pub fn enforce_full_cache_cap(data_dir: &Path, cap_bytes: u64) -> Result<(), DeriveError> {
    let root = data_dir.join("derivatives");
    if !root.is_dir() {
        return Ok(());
    }
    let mut files: Vec<(std::time::SystemTime, u64, PathBuf)> = Vec::new();
    for entry in walkdir::WalkDir::new(&root)
        .into_iter()
        .filter_map(Result::ok)
    {
        let path = entry.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if !name.ends_with("-full.webp") || !path.is_file() {
            continue;
        }
        let meta = fs::metadata(path)?;
        let accessed = meta.accessed().or_else(|_| meta.modified())?;
        files.push((accessed, meta.len(), path.to_path_buf()));
    }
    files.sort_by_key(|(accessed, _, _)| *accessed);
    let mut total: u64 = files.iter().map(|(_, len, _)| *len).sum();
    for (_, len, path) in files {
        if total <= cap_bytes {
            break;
        }
        if fs::remove_file(&path).is_ok() {
            total = total.saturating_sub(len);
        }
    }
    Ok(())
}

fn hex32(hash: &[u8; 32]) -> String {
    let mut hex = String::with_capacity(64);
    for b in hash {
        use std::fmt::Write as _;
        let _ = write!(hex, "{b:02x}");
    }
    hex
}
