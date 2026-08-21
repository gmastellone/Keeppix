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

/// Versione della **ricetta** dei derivati, non del formato del file.
///
/// Le URL `/media/{thumb,preview,full}/{hash}` escono con
/// `Cache-Control: … immutable`, che promette al browser che quell'URL non
/// cambierà mai: non rivalida per un anno. Ma l'hash indirizza il file
/// **sorgente**, non i byte serviti — e quelli dipendono da come li
/// produciamo. Quando la ricetta cambia, lo stesso URL restituisce
/// un'immagine diversa, e chi ha già in cache la vecchia se la tiene per
/// sempre.
///
/// È già successo: passando da WebP senza perdita a 1440 px a WebP con
/// perdita a 2048 px, e da anteprima incorporata a demosaic per `full`, un
/// browser che aveva visitato quelle URL continuava a mostrare le immagini
/// vecchie. Se ne è accorto solo un confronto fatto a mano fra ciò che
/// mostrava la pagina e ciò che rispondeva `curl`.
///
/// Il frontend appende `?v=` a questo numero, così una ricetta nuova produce
/// URL nuove e la cache si invalida da sola. Il server **ignora** il
/// parametro: serve solo come chiave di cache.
///
/// **Va incrementato a ogni modifica che cambi i byte prodotti a parità di
/// sorgente**: formato, qualità, `method`, dimensioni, encoder, o la scelta
/// fra incorporata e demosaic. Il valore è legato a
/// `frontend/src/api/media.ts` da un test: cambiarne uno solo fa fallire la
/// build.
pub const DERIVATIVE_VERSION: u32 = 2;

const THUMB: u32 = 240;
/// Lato lungo del derivato `preview`. Pubblico perché `full` usa
/// l'incorporata solo se la supera — altrimenti è un secondo file
/// con gli stessi pixel.
pub const PREVIEW_LONG_SIDE: u32 = 2048;
const PREVIEW: u32 = PREVIEW_LONG_SIDE;
/// Tetto della cache `full` pigra. ~200-300 zoom da 1,5-2,5 MB: una sessione
/// di culling, non l'archivio. Senza tetto è il cestino in un'altra forma.
pub const DEFAULT_FULL_CACHE_BYTES: u64 = 512 * 1024 * 1024;
/// Default della qualità WebP con perdita. Sotto 75 si vede; sopra 88
/// si paga per una differenza invisibile. Sovrascrivibile con
/// [`set_webp_quality`] / `KEEPPIX_WEBP_QUALITY`.
pub const DEFAULT_WEBP_QUALITY: u8 = 82;
static WEBP_QUALITY: AtomicU8 = AtomicU8::new(DEFAULT_WEBP_QUALITY);
/// Default `method` di libwebp (0=veloce … 6=lento/piccolo). L'API semplice
/// usava 4. 2 è ~2× più veloce in release con ~3% in più di peso.
/// Sovrascrivibile con [`set_webp_method`] / `KEEPPIX_WEBP_METHOD`.
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
    /// Il livello `full` richiederebbe un demosaic e quello non è
    /// disponibile (binario assente, timeout, file illeggibile). Non è un
    /// 404: il file c'è, manca il dettaglio in più.
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

/// Qualità di encoding WebP (1–100). Chiamato all'avvio da `Config`.
pub fn set_webp_quality(quality: u8) {
    WEBP_QUALITY.store(quality.clamp(1, 100), Ordering::Relaxed);
}

/// Metodo di encode libwebp (0–6). Chiamato all'avvio da `Config`.
pub fn set_webp_method(method: u8) {
    WEBP_METHOD.store(method.min(6), Ordering::Relaxed);
}

/// L'incorporata vale come `full` solo se è **strettamente** più grande
/// del derivato preview. Uguale o più piccola è un secondo file inutile.
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

/// Una decodifica, write su `.tmp`, `rename`. Idempotente se i file ci sono già.
/// Il nome resta storico (era JPEG-only in Fase 1b): dispatcha su
/// [`derive_from_bytes`], che riconosce il formato dai magic byte — vedi
/// [`decode_source`].
///
/// # Errors
/// I/O, formato non riconosciuto o illeggibile, o immagine oltre 200 MP.
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

/// Stessa pipeline di [`derive_jpeg`], ma i byte sono già in memoria. Il
/// formato non è dato per scontato: si annusa dai magic byte (JPEG, PNG,
/// TIFF, WebP, HEIF/HEIC) e si decodifica di conseguenza — vedi
/// [`decode_source`]. Prima di questo task decodificava solo JPEG, e ogni
/// altro formato che `kind::detect_kind` classifica come `Image` falliva
/// silenziosamente a generare miniatura e preview (debito Task 22).
///
/// # Errors
/// I/O, formato non riconosciuto o illeggibile, o immagine oltre 200 MP.
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

/// Ceiling per il decode `heif-convert` in sandbox, stesso valore di
/// `video::MEM`/`raw::DEMOSAIC_MEMORY_BYTES`: libheif mappa più plugin
/// codec (libde265/aom) prima di iniziare a lavorare, e un HEIC 10 bit
/// reale deve tenere in memoria un frame YUV/RGB pieno più i buffer di
/// tone-mapping prima dell'encode PNG di uscita. Non rimisurato byte a
/// byte su questo host — è lo stesso tetto già accettato per gli altri
/// decoder in C, non uno nuovo inventato per l'occasione.
const HEIF_MEMORY_BYTES: u64 = 1024 * 1024 * 1024;
const HEIF_CPU_SECS: u64 = 30;

/// `false` se `heif-convert` non è in `PATH`, sullo stesso modello di
/// [`crate::raw::dcraw_emu_available`] / [`crate::video::ffprobe_available`]:
/// permette ai test di saltare pulito su una macchina senza
/// `libheif-examples` installato, invece di fallire.
#[must_use]
pub fn heif_convert_available() -> bool {
    sandbox::run("heif-convert", &["--version"], 64 * 1024 * 1024, 5)
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Formato immagine riconosciuto dai magic byte, per il dispatch di
/// [`decode_source`]. Non è [`crate::kind::AssetKind`]: quello classifica
/// per l'import (RAW vs Image vs Video), questo sceglie *quale decoder*
/// usare per un byte stream già sappiamo essere un'immagine non-RAW.
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

/// Sottoinsieme "immagine fissa" dei brand HEIF/HEIC (esclude `avif`/`avis`
/// di proposito: non richiesto da questo task, e `detect_kind` li classifica
/// comunque come `Image` — resta debito noto, non introdotto qui).
fn is_heif_ftyp(after_ftyp: &[u8]) -> bool {
    after_ftyp.chunks_exact(4).any(|c| {
        matches!(
            c,
            b"heic" | b"heix" | b"heif" | b"heim" | b"heis" | b"mif1" | b"msf1"
        )
    })
}

/// Decodifica byte di formato non noto a priori in RGB8 interleaved. Ogni
/// ramo controlla `MAX_PIXELS` sulle sole dimensioni (header) prima di
/// decodificare i pixel, per non pagare il costo di un decode completo su
/// un file che verrà comunque rifiutato.
///
/// # Errors
/// Formato non riconosciuto, file corrotto, o immagine oltre 200 MP.
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
    let rgb = decoder
        .decode()
        .map_err(|e| DeriveError::Decode(e.to_string()))?;
    Ok((rgb, width, height))
}

/// Puro Rust (crate `png`), nessuna dipendenza C nuova. `normalize_to_color8`
/// riduce sempre a 8 bit/canale (`STRIP_16`) ed espande palette/tRNS/gray a
/// bit-depth pieno (`EXPAND`) — il colore finale resta `Grayscale`,
/// `GrayscaleAlpha`, `Rgb` o `Rgba` secondo la sorgente, non forzato a RGB
/// dal crate: lo facciamo a mano sotto.
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

/// Puro Rust (crate `tiff`, lo stesso decoder usato da `image`-rs). Copre i
/// TIFF "fotografici" non-camera-RAW: `kind::detect_kind` manda già i RAW
/// (Bayer, `looks_like_camera_raw`) su `AssetKind::RawImage`, che non passa
/// da qui. Palette e CMYK non sono fotografie comuni in libreria: si
/// rifiuta con un errore leggibile invece di produrre colori sbagliati.
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

/// Il binding a libwebp legge (già usato oggi solo in scrittura per i
/// derivati) — qui si collega anche in lettura per un WebP *sorgente*
/// caricato in libreria. Nessuna dipendenza nuova.
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

/// HEIF/HEIC (8 e 10 bit) via `heif-convert` in sandbox — mai libheif
/// in-process. Ruling Task 22: `libheif`/HEVC hanno una storia di CVE nei
/// parser almeno quanto LibRaw/ffmpeg, quindi passano dallo stesso
/// `sandbox::run` con `RLIMIT_AS`/`RLIMIT_CPU`, non ne sono esenti perché
/// sono un binding invece di un binario invocato altrove nel codice.
///
/// `heif-convert` legge e scrive solo file reali (non stdin/stdout: verificato
/// contro libheif 1.17 — `-o -` fallisce con "Unknown file type in -"), quindi
/// i byte transitano per due file temporanei che si autodistruggono al
/// `Drop` di `NamedTempFile`. L'uscita è un PNG — 16-bit se la sorgente è
/// 10-bit, confermato con `heif-info` sulla fixture `sample10.heic` — che
/// rientra nel decoder PNG scritto per questo stesso task, il quale
/// normalizza a 8 bit come tutto il resto della pipeline.
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

/// Scrive il WebP a piena risoluzione se manca. Idempotente: se il file c'è
/// già non si ricodifica (si aggiorna solo l'atime per lo LRU). Stesso
/// riconoscimento del formato di [`derive_from_bytes`].
///
/// # Errors
/// I/O, formato non riconosciuto o illeggibile, o immagine oltre 200 MP.
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

/// Come [`ensure_full_from_bytes`], da pixel RGB8 già decodificati — l'uscita
/// del demosaic, che non è un JPEG.
///
/// # Errors
/// I/O, o immagine oltre 200 MP.
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

/// Sfratta i `*-full.webp` meno usati di recente finché il totale sta nel tetto.
/// Cammina solo `data/full/`: il costo è O(full in cache), non O(tutto
/// l'archivio di thumb/preview).
///
/// # Errors
/// I/O sulla directory della cache full.
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
