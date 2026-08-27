use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use std::sync::atomic::{AtomicU8, Ordering};

use fast_image_resize::images::Image;
use fast_image_resize::{FilterType, PixelType, ResizeAlg, ResizeOptions, Resizer};
use thiserror::Error;
use webp::Encoder as WebPEncoder;
use zune_jpeg::JpegDecoder;

use crate::sandbox;

/// Version of the derivative **recipe**, not of the file format.
///
/// The `/media/{thumb,preview,full}/{hash}` URLs are served with
/// `Cache-Control: … immutable`, which promises the browser that URL will
/// never change: it won't revalidate for a year. But the hash addresses the
/// **source** file, not the served bytes — and those depend on how we
/// produce them. When the recipe changes, the same URL returns a different
/// image, and anyone who already has the old one cached keeps it forever.
///
/// This has happened before: switching from lossless WebP at 1440 px to
/// lossy WebP at 2048 px, and from embedded preview to demosaic for `full`,
/// left a browser that had already visited those URLs showing the old
/// images. It only surfaced through a manual comparison between what the
/// page displayed and what `curl` returned.
///
/// The frontend appends `?v=` with this number, so a new recipe produces
/// new URLs and the cache invalidates itself. The server **ignores** the
/// parameter: it only serves as a cache key.
///
/// **Must be incremented on any change that alters the bytes produced from
/// the same source**: format, quality, `method`, dimensions, encoder, or
/// the choice between embedded and demosaic. The value is tied to
/// `frontend/src/api/media.ts` by a test: changing only one of them fails
/// the build.
pub const DERIVATIVE_VERSION: u32 = 2;

const THUMB: u32 = 240;
/// Long side of the `preview` derivative. Public because `full` only uses
/// the embedded preview if it exceeds this — otherwise it would be a
/// second file with the same pixels.
pub const PREVIEW_LONG_SIDE: u32 = 2048;
const PREVIEW: u32 = PREVIEW_LONG_SIDE;
/// Cap on the lazy `full` cache. ~200-300 zooms at 1.5-2.5 MB each: a
/// culling session, not the archive. Without a cap it's just a trash can
/// wearing a different hat.
pub const DEFAULT_FULL_CACHE_BYTES: u64 = 512 * 1024 * 1024;
/// Default lossy WebP quality. Below 75 it's visible; above 88 you're
/// paying for an invisible difference. Overridable with
/// [`set_webp_quality`] / `KEEPPIX_WEBP_QUALITY`.
pub const DEFAULT_WEBP_QUALITY: u8 = 82;
static WEBP_QUALITY: AtomicU8 = AtomicU8::new(DEFAULT_WEBP_QUALITY);
/// Default libwebp `method` (0=fast … 6=slow/small). The simple API used
/// 4. 2 is ~2× faster in release for ~3% more weight. Overridable with
/// [`set_webp_method`] / `KEEPPIX_WEBP_METHOD`.
pub const DEFAULT_WEBP_METHOD: u8 = 2;
static WEBP_METHOD: AtomicU8 = AtomicU8::new(DEFAULT_WEBP_METHOD);
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
    /// The `full` level would require a demosaic and that isn't available
    /// (binary missing, timeout, unreadable file). Not a 404: the file
    /// exists, just missing the extra detail.
    #[error("full resolution unavailable")]
    FullUnavailable,
}

#[derive(Debug, Clone)]
pub struct DeriveResult {
    pub thumb: PathBuf,
    pub preview: Option<PathBuf>,
    pub thumbhash: Vec<u8>,
    pub skipped: bool,
}

/// WebP encoding quality (1-100). Called at startup by `Config`.
pub fn set_webp_quality(quality: u8) {
    WEBP_QUALITY.store(quality.clamp(1, 100), Ordering::Relaxed);
}

/// libwebp encode method (0-6). Called at startup by `Config`.
pub fn set_webp_method(method: u8) {
    WEBP_METHOD.store(method.min(6), Ordering::Relaxed);
}

/// The embedded preview only counts as `full` if it's **strictly** larger
/// than the preview derivative. Equal or smaller would be a useless second
/// file.
#[must_use]
pub fn embedded_usable_as_full(long_side: u32) -> bool {
    long_side > PREVIEW_LONG_SIDE
}

fn webp_quality() -> u8 {
    std::env::var("KEEPPIX_WEBP_QUALITY")
        .ok()
        .and_then(|s| s.parse().ok())
        .filter(|q: &u8| (1..=100).contains(q))
        .unwrap_or_else(|| WEBP_QUALITY.load(Ordering::Relaxed))
}

fn webp_method() -> u8 {
    std::env::var("KEEPPIX_WEBP_METHOD")
        .ok()
        .and_then(|s| s.parse().ok())
        .filter(|m: &u8| *m <= 6)
        .unwrap_or_else(|| WEBP_METHOD.load(Ordering::Relaxed))
}

/// One decode, write to `.tmp`, `rename`. Idempotent if the files already
/// exist. The name is historical (this used to be JPEG-only): it dispatches
/// to [`derive_from_bytes`], which recognizes the format from magic
/// bytes — see [`decode_source`].
///
/// # Errors
/// I/O, unrecognized or unreadable format, or image over 200 MP.
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

/// Same pipeline as [`derive_jpeg`], but the bytes are already in memory.
/// The format isn't assumed: it's sniffed from magic bytes (JPEG, PNG,
/// TIFF, WebP, HEIF/HEIC) and decoded accordingly — see [`decode_source`].
/// This used to decode JPEG only, silently failing to produce a thumbnail
/// and preview for any other format that `kind::detect_kind` classifies as
/// `Image`.
///
/// # Errors
/// I/O, unrecognized or unreadable format, or image over 200 MP.
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

    let (rgb, width, height) = decode_source(bytes)?;
    let skip_preview = width.max(height) <= SKIP_PREVIEW_PX
        && u64::try_from(bytes.len()).unwrap_or(u64::MAX) <= SKIP_PREVIEW_BYTES;
    build_derivatives(&rgb, width, height, skip_preview, &thumb, &preview)
}

/// Like [`derive_from_bytes`], but for already-decoded RGB8 pixels — the
/// output of RAW demosaicing, which isn't a JPEG and doesn't go through
/// `JpegDecoder`. No source-byte threshold for skipping the preview: there
/// is no "original file weight" for already-decoded pixels, only
/// dimensions matter.
///
/// # Errors
/// I/O, or image over 200 MP.
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

/// Ceiling for the sandboxed `heif-convert` decode, same value as
/// `video::MEM`/`raw::DEMOSAIC_MEMORY_BYTES`: libheif maps several codec
/// plugins (libde265/aom) before it starts working, and a real 10-bit HEIC
/// needs to hold a full YUV/RGB frame plus tone-mapping buffers in memory
/// before the output PNG encode. Not remeasured byte-for-byte on this
/// host — it's the same ceiling already accepted for the other C decoders,
/// not a new one invented for the occasion.
const HEIF_MEMORY_BYTES: u64 = 1024 * 1024 * 1024;
const HEIF_CPU_SECS: u64 = 30;

/// `false` if `heif-convert` is not on `PATH`, following the same pattern
/// as [`crate::raw::dcraw_emu_available`] /
/// [`crate::video::ffprobe_available`]: lets tests skip cleanly on a
/// machine without `libheif-examples` installed, instead of failing.
#[must_use]
pub fn heif_convert_available() -> bool {
    sandbox::run("heif-convert", &["--version"], 64 * 1024 * 1024, 5)
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Image format recognized from magic bytes, for dispatching in
/// [`decode_source`]. Not [`crate::kind::AssetKind`]: that classifies for
/// import (RAW vs Image vs Video), this one picks *which decoder* to use
/// for a byte stream we already know is a non-RAW image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SourceFormat {
    Jpeg,
    Png,
    Tiff,
    WebP,
    Heif,
}

fn sniff_source_format(bytes: &[u8]) -> Option<SourceFormat> {
    if bytes.len() >= 3 && bytes[0] == 0xFF && bytes[1] == 0xD8 && bytes[2] == 0xFF {
        return Some(SourceFormat::Jpeg);
    }
    if bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]) {
        return Some(SourceFormat::Png);
    }
    if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        return Some(SourceFormat::WebP);
    }
    if bytes.len() >= 4
        && (bytes.starts_with(&[0x49, 0x49, 0x2A, 0x00])
            || bytes.starts_with(&[0x4D, 0x4D, 0x00, 0x2A]))
    {
        return Some(SourceFormat::Tiff);
    }
    if bytes.len() >= 12 && &bytes[4..8] == b"ftyp" && is_heif_ftyp(&bytes[8..]) {
        return Some(SourceFormat::Heif);
    }
    None
}

/// The "still image" subset of HEIF/HEIC brands (deliberately excludes
/// `avif`/`avis`: not needed here, and `detect_kind` classifies them as
/// `Image` anyway — a known gap, not introduced by this code).
fn is_heif_ftyp(after_ftyp: &[u8]) -> bool {
    after_ftyp.chunks_exact(4).any(|c| {
        matches!(
            c,
            b"heic" | b"heix" | b"heif" | b"heim" | b"heis" | b"mif1" | b"msf1"
        )
    })
}

/// Decodes bytes of a format not known in advance into interleaved RGB8.
/// Each branch checks `MAX_PIXELS` against dimensions (header) alone before
/// decoding pixels, to avoid paying for a full decode on a file that will
/// be rejected anyway.
///
/// # Errors
/// Unrecognized format, corrupt file, or image over 200 MP.
pub fn decode_to_rgb8(bytes: &[u8]) -> Result<(Vec<u8>, u32, u32), DeriveError> {
    decode_source(bytes)
}

/// Decodes bytes of a format not known in advance into interleaved RGB8.
/// Each branch checks `MAX_PIXELS` against dimensions (header) alone before
/// decoding pixels, to avoid paying for a full decode on a file that will
/// be rejected anyway.
///
/// # Errors
/// Unrecognized format, corrupt file, or image over 200 MP.
fn decode_source(bytes: &[u8]) -> Result<(Vec<u8>, u32, u32), DeriveError> {
    match sniff_source_format(bytes) {
        Some(SourceFormat::Jpeg) => decode_jpeg(bytes),
        Some(SourceFormat::Png) => decode_png(bytes),
        Some(SourceFormat::Tiff) => decode_tiff(bytes),
        Some(SourceFormat::WebP) => decode_webp(bytes),
        Some(SourceFormat::Heif) => decode_heif(bytes),
        None => Err(DeriveError::Decode("unrecognized image format".to_owned())),
    }
}

fn decode_jpeg(bytes: &[u8]) -> Result<(Vec<u8>, u32, u32), DeriveError> {
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
    let pixels = decoder
        .decode()
        .map_err(|e| DeriveError::Decode(e.to_string()))?;
    let expected_rgb = (width as usize)
        .saturating_mul(height as usize)
        .saturating_mul(3);
    let expected_gray = (width as usize).saturating_mul(height as usize);
    let rgb = if pixels.len() == expected_rgb {
        pixels
    } else if pixels.len() == expected_gray {
        // Single-component JPEG: expand to interleaved RGB.
        gray8_to_rgb8(&pixels)
    } else {
        return Err(DeriveError::Decode(format!(
            "jpeg: unexpected buffer length {} for {width}x{height}",
            pixels.len()
        )));
    };
    Ok((rgb, width, height))
}

/// Pure Rust (crate `png`), no new C dependency. `normalize_to_color8`
/// always reduces to 8 bits/channel (`STRIP_16`) and expands palette/tRNS/
/// gray to full bit depth (`EXPAND`) — the final color stays `Grayscale`,
/// `GrayscaleAlpha`, `Rgb`, or `Rgba` depending on the source, not forced
/// to RGB by the crate: we do that by hand below.
fn decode_png(bytes: &[u8]) -> Result<(Vec<u8>, u32, u32), DeriveError> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder
        .read_info()
        .map_err(|e| DeriveError::Decode(format!("png: {e}")))?;
    let info = reader.info();
    let (width, height) = (info.width, info.height);
    if u64::from(width).saturating_mul(u64::from(height)) > MAX_PIXELS {
        return Err(DeriveError::TooManyPixels);
    }
    let buf_len = reader
        .output_buffer_size()
        .ok_or_else(|| DeriveError::Decode("png: image too large to buffer".to_owned()))?;
    let mut buf = vec![0u8; buf_len];
    let out_info = reader
        .next_frame(&mut buf)
        .map_err(|e| DeriveError::Decode(format!("png: {e}")))?;
    let pixels = &buf[..out_info.buffer_size()];
    let rgb = match out_info.color_type {
        png::ColorType::Rgb => pixels.to_vec(),
        png::ColorType::Rgba => rgba8_to_rgb8(pixels),
        png::ColorType::Grayscale => gray8_to_rgb8(pixels),
        png::ColorType::GrayscaleAlpha => graya8_to_rgb8(pixels),
        other @ png::ColorType::Indexed => {
            return Err(DeriveError::Decode(format!(
                "png: unsupported color type after normalization: {other:?}"
            )));
        }
    };
    Ok((rgb, width, height))
}

/// Pure Rust (crate `tiff`, the same decoder used by `image`-rs). Covers
/// "photographic" non-camera-RAW TIFFs: `kind::detect_kind` already routes
/// RAW (Bayer, `looks_like_camera_raw`) to `AssetKind::RawImage`, which
/// doesn't come through here. Palette and CMYK aren't common library
/// photos: reject with a readable error instead of producing wrong colors.
fn decode_tiff(bytes: &[u8]) -> Result<(Vec<u8>, u32, u32), DeriveError> {
    let mut decoder = tiff::decoder::Decoder::new(std::io::Cursor::new(bytes))
        .map_err(|e| DeriveError::Decode(format!("tiff: {e}")))?;
    let (width, height) = decoder
        .dimensions()
        .map_err(|e| DeriveError::Decode(format!("tiff: {e}")))?;
    if u64::from(width).saturating_mul(u64::from(height)) > MAX_PIXELS {
        return Err(DeriveError::TooManyPixels);
    }
    let color = decoder
        .colortype()
        .map_err(|e| DeriveError::Decode(format!("tiff: {e}")))?;
    let image = decoder
        .read_image()
        .map_err(|e| DeriveError::Decode(format!("tiff: {e}")))?;
    tiff_samples_to_rgb8(color, image).map(|rgb| (rgb, width, height))
}

fn tiff_samples_to_rgb8(
    color: tiff::ColorType,
    image: tiff::decoder::DecodingResult,
) -> Result<Vec<u8>, DeriveError> {
    use tiff::ColorType;
    use tiff::decoder::DecodingResult;
    match (color, image) {
        (ColorType::RGB(8), DecodingResult::U8(v)) => Ok(v),
        (ColorType::RGB(16), DecodingResult::U16(v)) => Ok(u16_to_u8(&v)),
        (ColorType::RGBA(8), DecodingResult::U8(v)) => Ok(rgba8_to_rgb8(&v)),
        (ColorType::RGBA(16), DecodingResult::U16(v)) => Ok(rgba8_to_rgb8(&u16_to_u8(&v))),
        (ColorType::Gray(8), DecodingResult::U8(v)) => Ok(gray8_to_rgb8(&v)),
        (ColorType::Gray(16), DecodingResult::U16(v)) => Ok(gray8_to_rgb8(&u16_to_u8(&v))),
        (ColorType::GrayA(8), DecodingResult::U8(v)) => Ok(graya8_to_rgb8(&v)),
        (ColorType::GrayA(16), DecodingResult::U16(v)) => Ok(graya8_to_rgb8(&u16_to_u8(&v))),
        (other, _) => Err(DeriveError::Decode(format!(
            "tiff: unsupported color type {other:?}"
        ))),
    }
}

/// The libwebp binding can read too (today it's only used for writing
/// derivatives) — here it's also wired up for reading a *source* WebP
/// imported into the library. No new dependency.
fn decode_webp(bytes: &[u8]) -> Result<(Vec<u8>, u32, u32), DeriveError> {
    let features = webp::BitstreamFeatures::new(bytes)
        .ok_or_else(|| DeriveError::Decode("webp: unreadable bitstream".to_owned()))?;
    if u64::from(features.width()).saturating_mul(u64::from(features.height())) > MAX_PIXELS {
        return Err(DeriveError::TooManyPixels);
    }
    let image = webp::Decoder::new(bytes)
        .decode()
        .ok_or_else(|| DeriveError::Decode("webp: decode failed".to_owned()))?;
    let width = image.width();
    let height = image.height();
    let rgb = if image.is_alpha() {
        rgba8_to_rgb8(&image)
    } else {
        image.to_vec()
    };
    Ok((rgb, width, height))
}

/// HEIF/HEIC (8- and 10-bit) via sandboxed `heif-convert` — never libheif
/// in-process. `libheif`/HEVC have a CVE history in their parsers at least
/// as bad as LibRaw/ffmpeg, so they go through the same `sandbox::run` with
/// `RLIMIT_AS`/`RLIMIT_CPU`; being a library binding rather than a binary
/// invoked elsewhere in the code is not an exemption.
///
/// `heif-convert` only reads and writes real files (not stdin/stdout:
/// verified against libheif 1.17 — `-o -` fails with "Unknown file type in
/// -"), so the bytes go through two temp files that self-destruct on
/// `NamedTempFile`'s `Drop`. The output is a PNG — 16-bit if the source is
/// 10-bit, confirmed with `heif-info` on the `sample10.heic` fixture —
/// which feeds into the PNG decoder above, normalizing to 8 bits like the
/// rest of the pipeline.
fn decode_heif(bytes: &[u8]) -> Result<(Vec<u8>, u32, u32), DeriveError> {
    let mut input = tempfile::Builder::new()
        .prefix("kpx-heif-in-")
        .suffix(".heic")
        .tempfile()?;
    input.write_all(bytes)?;
    input.flush()?;

    let output = tempfile::Builder::new()
        .prefix("kpx-heif-out-")
        .suffix(".png")
        .tempfile()?;
    let input_s = input.path().to_string_lossy().into_owned();
    let output_s = output.path().to_string_lossy().into_owned();

    let out = sandbox::run(
        "heif-convert",
        &[input_s.as_str(), output_s.as_str()],
        HEIF_MEMORY_BYTES,
        HEIF_CPU_SECS,
    )
    .map_err(|e| DeriveError::Decode(format!("heif-convert: {e}")))?;
    if !out.status.success() {
        return Err(DeriveError::Decode(format!(
            "heif-convert: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    let png_bytes = fs::read(output.path())?;
    decode_png(&png_bytes)
}

fn u16_to_u8(samples: &[u16]) -> Vec<u8> {
    samples.iter().map(|s| (*s >> 8) as u8).collect()
}

fn rgba8_to_rgb8(rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(rgba.len() / 4 * 3);
    for chunk in rgba.chunks_exact(4) {
        out.extend_from_slice(&chunk[..3]);
    }
    out
}

fn gray8_to_rgb8(gray: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(gray.len() * 3);
    for &g in gray {
        out.extend_from_slice(&[g, g, g]);
    }
    out
}

fn graya8_to_rgb8(graya: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(graya.len() / 2 * 3);
    for chunk in graya.chunks_exact(2) {
        let g = chunk[0];
        out.extend_from_slice(&[g, g, g]);
    }
    out
}

/// Tail shared by [`derive_from_bytes`] and [`derive_from_rgb`]: resize,
/// atomic webp write of the derivatives, thumbhash. The caller has already
/// handled idempotency and the pixel limit.
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
    // The `full` level isn't written here: it's lazy, generated on the first zoom request.

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
    let mut config = webp::WebPConfig::new()
        .map_err(|()| DeriveError::Decode("webp config init failed".to_owned()))?;
    config.lossless = 0;
    config.quality = f32::from(webp_quality());
    config.method = i32::from(webp_method());
    config.alpha_compression = 1;
    let encoded = WebPEncoder::from_rgb(rgb, w, h)
        .encode_advanced(&config)
        .map_err(|e| DeriveError::Decode(format!("webp encode: {e:?}")))?;
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

fn full_cache_dir(data_dir: &Path, hash: &[u8; 32]) -> (String, PathBuf) {
    let hex = hex32(hash);
    let dir = data_dir.join("full").join(&hex[0..2]).join(&hex[2..4]);
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
    let (hex, dir) = full_cache_dir(data_dir, hash);
    dir.join(format!("{hex}-full.webp"))
}

/// Writes the full-resolution WebP if missing. Idempotent: if the file
/// already exists it isn't re-encoded (only the atime is updated for the
/// LRU). Same format recognition as [`derive_from_bytes`].
///
/// # Errors
/// I/O, unrecognized or unreadable format, or image over 200 MP.
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

    let (rgb, width, height) = decode_source(bytes)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    write_webp_atomic(&path, &rgb, width, height)?;
    Ok(path)
}

/// Like [`ensure_full_from_bytes`], from already-decoded RGB8 pixels — the
/// output of demosaicing, which isn't a JPEG.
///
/// # Errors
/// I/O, or image over 200 MP.
pub fn ensure_full_from_rgb(
    rgb: &[u8],
    width: u32,
    height: u32,
    data_dir: &Path,
    hash: &[u8; 32],
) -> Result<PathBuf, DeriveError> {
    let path = full_derivative_path(data_dir, hash);
    if path.is_file() {
        touch_accessed(&path)?;
        return Ok(path);
    }
    if u64::from(width).saturating_mul(u64::from(height)) > MAX_PIXELS {
        return Err(DeriveError::TooManyPixels);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    write_webp_atomic(&path, rgb, width, height)?;
    Ok(path)
}

fn touch_accessed(path: &Path) -> Result<(), DeriveError> {
    let file = fs::File::open(path)?;
    let times = fs::FileTimes::new().set_accessed(std::time::SystemTime::now());
    file.set_times(times)?;
    Ok(())
}

/// Evicts the least recently used `*-full.webp` files until the total fits
/// the cap. Walks only `data/full/`: the cost is O(full cache entries), not
/// O(the whole thumb/preview archive).
///
/// # Errors
/// I/O on the full cache directory.
pub fn enforce_full_cache_cap(data_dir: &Path, cap_bytes: u64) -> Result<(), DeriveError> {
    let root = data_dir.join("full");
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
