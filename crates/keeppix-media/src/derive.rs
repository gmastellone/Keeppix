use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use fast_image_resize::images::Image;
use fast_image_resize::{FilterType, PixelType, ResizeAlg, ResizeOptions, Resizer};
use image_webp::WebPEncoder;
use thiserror::Error;
use zune_jpeg::JpegDecoder;

const THUMB: u32 = 240;
const PREVIEW: u32 = 1440;
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
    let mut buf = Vec::new();
    WebPEncoder::new(&mut buf)
        .encode(rgb, w, h, image_webp::ColorType::Rgb8)
        .map_err(|e| DeriveError::Decode(e.to_string()))?;
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(&buf)?;
    }
    fs::rename(tmp, path)?;
    Ok(())
}

#[must_use]
pub fn derivative_paths(data_dir: &Path, hash: &[u8; 32]) -> (PathBuf, PathBuf) {
    let hex = hex32(hash);
    let dir = data_dir
        .join("derivatives")
        .join(&hex[0..2])
        .join(&hex[2..4]);
    (
        dir.join(format!("{hex}-thumb.webp")),
        dir.join(format!("{hex}-preview.webp")),
    )
}

fn hex32(hash: &[u8; 32]) -> String {
    let mut hex = String::with_capacity(64);
    for b in hash {
        use std::fmt::Write as _;
        let _ = write!(hex, "{b:02x}");
    }
    hex
}
