//! Estrazione della preview JPEG incorporata nei file RAW, senza demosaic.
//!
//! ARW, NEF, CR2 e DNG sono contenitori TIFF: la preview vive in una IFD
//! (spesso figlia, via il tag `SubIFDs`) che punta a un JPEG completo tramite
//! `JPEGInterchangeFormat`/`Length` oppure `StripOffsets`/`StripByteCounts`
//! con `Compression` JPEG. CR3 è ISO-BMFF: la preview vive nel box `PRVW`,
//! dentro un `uuid` di primo livello (non in `moov`, vedi commento su
//! `extract_from_cr3`).
//!
//! In entrambi i casi si accetta un candidato solo se i suoi byte iniziano
//! con SOI (`FFD8`) e contengono un marker SOF *baseline o progressivo*
//! (0xC0/0xC1/0xC2): questo scarta automaticamente i dati Bayer compressi
//! con schemi "JPEG-like" ma non decodificabili (es. NEF lossy, o gli strip
//! JPEG-lossless dei sensori CR2), che altrimenti supererebbero il controllo
//! SOI pur non essendo un'immagine utilizzabile.

use std::collections::HashSet;
use std::fs;
use std::path::Path;

use thiserror::Error;

use crate::sandbox;

const MAX_PREVIEW_BYTES: usize = 64 * 1024 * 1024;
const MAX_IFDS: usize = 64;
const MAX_SUBIFD_FANOUT: u32 = 16;
const MAX_BMFF_BOXES: usize = 1024;

#[derive(Debug, Error)]
pub enum RawError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("unsupported raw format: {0}")]
    Unsupported(String),
    #[error("corrupt raw file: {0}")]
    Corrupt(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewSource {
    Embedded,
    Demosaic,
}

#[derive(Debug, Clone)]
pub struct RawPreview {
    pub bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub source: PreviewSource,
}

/// Estrae la preview JPEG più grande incorporata in un file RAW.
///
/// # Errors
/// I/O, formato non riconosciuto, o struttura del contenitore corrotta al
/// punto da non poter nemmeno leggere l'header. Un formato noto senza
/// preview utilizzabile è `Ok(None)`, non un errore.
pub fn extract_embedded_preview(path: &Path) -> Result<Option<RawPreview>, RawError> {
    let buf = fs::read(path)?;
    if is_tiff(&buf) {
        return extract_from_tiff(&buf);
    }
    if is_cr3(&buf) {
        return extract_from_cr3(&buf);
    }
    Err(RawError::Unsupported(
        "not a recognized RAW container (TIFF or CR3/ISO-BMFF)".to_owned(),
    ))
}

/// `false` se `dcraw_emu` non è in `PATH`: permette ai test di saltare senza
/// fallire su una macchina/CI senza libraw installato, come già fa
/// `video::ffprobe_available`. `dcraw_emu` non ha un flag che riesce senza un
/// file in ingresso, quindi il segnale è che il processo sia partito
/// affatto, non il suo exit status.
#[must_use]
pub fn dcraw_emu_available() -> bool {
    sandbox::run("dcraw_emu", &["-v"], 64 * 1024 * 1024, 5).is_ok()
}

/// Demosaic half-size con bilanciamento del bianco della fotocamera, via
/// `dcraw_emu` in sandbox. Non è per l'esportazione: è per avere un'anteprima
/// decente quando il RAW non porta una preview incorporata utilizzabile.
///
/// `dcraw_emu` gira **sempre** in un processo separato con `rlimit`: è codice
/// C che apre file non fidati.
///
/// # Errors
/// Il processo non parte, esce con errore (formato non supportato, file
/// corrotto), o la sua uscita non è il PPM 8-bit atteso.
pub fn demosaic_half(
    path: &Path,
    memory_bytes: u64,
    cpu_secs: u64,
) -> Result<RawPreview, RawError> {
    let path_s = path.to_string_lossy();
    let out = sandbox::run(
        "dcraw_emu",
        &["-h", "-w", "-Z", "-", path_s.as_ref()],
        memory_bytes,
        cpu_secs,
    )?;
    if !out.status.success() {
        return Err(RawError::Corrupt(format!(
            "dcraw_emu: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    parse_ppm(&out.stdout)
}

/// Parser minimo per il PPM binario (`P6`) che `dcraw_emu` scrive su stdout:
/// header ASCII (magic, larghezza, altezza, valore massimo) seguito da un
/// singolo byte di separazione e dai pixel RGB8 interleaved. Bastano tre
/// token: non serve un parser PNM completo per un formato che generiamo e
/// consumiamo noi stessi.
fn parse_ppm(bytes: &[u8]) -> Result<RawPreview, RawError> {
    if !bytes.starts_with(b"P6") {
        return Err(RawError::Corrupt("dcraw_emu: not a P6 ppm".to_owned()));
    }
    let mut pos = 2usize;
    let width = read_ppm_uint(bytes, &mut pos)?;
    let height = read_ppm_uint(bytes, &mut pos)?;
    let maxval = read_ppm_uint(bytes, &mut pos)?;
    if maxval == 0 || maxval > 255 {
        return Err(RawError::Corrupt(
            "dcraw_emu: expected an 8-bit ppm".to_owned(),
        ));
    }
    // Esattamente un byte di separazione fra l'header ASCII e i pixel binari.
    if pos >= bytes.len() {
        return Err(RawError::Corrupt(
            "dcraw_emu: truncated ppm header".to_owned(),
        ));
    }
    pos += 1;

    let pixel_count = u64::from(width)
        .saturating_mul(u64::from(height))
        .saturating_mul(3);
    let pixel_count = usize::try_from(pixel_count)
        .map_err(|_| RawError::Corrupt("dcraw_emu: ppm dimensions overflow".to_owned()))?;
    let pixels = bytes
        .get(pos..pos + pixel_count)
        .ok_or_else(|| RawError::Corrupt("dcraw_emu: truncated ppm pixel data".to_owned()))?;

    Ok(RawPreview {
        bytes: pixels.to_vec(),
        width,
        height,
        source: PreviewSource::Demosaic,
    })
}

/// Legge un token intero ASCII in un header PNM, saltando whitespace e i
/// commenti `#...\n` che il formato ammette fra i campi.
fn read_ppm_uint(bytes: &[u8], pos: &mut usize) -> Result<u32, RawError> {
    loop {
        while bytes.get(*pos).is_some_and(u8::is_ascii_whitespace) {
            *pos += 1;
        }
        if bytes.get(*pos) != Some(&b'#') {
            break;
        }
        while bytes.get(*pos).is_some_and(|b| *b != b'\n') {
            *pos += 1;
        }
    }
    let start = *pos;
    while bytes.get(*pos).is_some_and(u8::is_ascii_digit) {
        *pos += 1;
    }
    if *pos == start {
        return Err(RawError::Corrupt(
            "dcraw_emu: malformed ppm header".to_owned(),
        ));
    }
    std::str::from_utf8(&bytes[start..*pos])
        .ok()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| RawError::Corrupt("dcraw_emu: malformed ppm header".to_owned()))
}

fn is_tiff(buf: &[u8]) -> bool {
    buf.len() >= 8 && (buf.starts_with(b"II*\0") || buf.starts_with(b"MM\0*"))
}

fn is_cr3(buf: &[u8]) -> bool {
    if buf.len() < 16 || &buf[4..8] != b"ftyp" {
        return false;
    }
    let box_size = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
    if box_size < 8 || box_size > buf.len() {
        return false;
    }
    buf[8..box_size].windows(4).any(|w| w == b"crx ")
}

#[derive(Clone, Copy)]
enum Endian {
    Little,
    Big,
}

impl Endian {
    fn u16(self, buf: &[u8], off: usize) -> Option<u16> {
        let b = buf.get(off..off + 2)?;
        Some(match self {
            Endian::Little => u16::from_le_bytes([b[0], b[1]]),
            Endian::Big => u16::from_be_bytes([b[0], b[1]]),
        })
    }

    fn u32(self, buf: &[u8], off: usize) -> Option<u32> {
        let b = buf.get(off..off + 4)?;
        Some(match self {
            Endian::Little => u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            Endian::Big => u32::from_be_bytes([b[0], b[1], b[2], b[3]]),
        })
    }
}

struct Candidate {
    offset: usize,
    len: usize,
}

/// Byte per componente per i tipi TIFF che ci interessano (SHORT/LONG e
/// varianti signed): 0 per i tipi che ignoriamo.
fn type_size(field_type: u16) -> usize {
    match field_type {
        1 | 2 | 6 | 7 => 1,
        3 | 8 => 2,
        4 | 9 | 11 => 4,
        5 | 10 | 12 => 8,
        _ => 0,
    }
}

/// Legge il primo valore di un tag TIFF, gestendo sia il caso inline
/// (byte contenuti nel campo valore/offset da 4 byte) sia quello esterno.
fn resolve_first_u32(
    buf: &[u8],
    endian: Endian,
    value_off: usize,
    field_type: u16,
    count: u32,
) -> Option<u32> {
    let sz = type_size(field_type);
    if sz == 0 || count == 0 {
        return None;
    }
    let inline = sz.saturating_mul(count as usize) <= 4;
    let base = if inline {
        value_off
    } else {
        endian.u32(buf, value_off)? as usize
    };
    match field_type {
        3 | 8 => endian.u16(buf, base).map(u32::from),
        4 | 9 => endian.u32(buf, base),
        1 | 2 | 6 | 7 => buf.get(base).copied().map(u32::from),
        _ => None,
    }
}

/// Legge tutti i valori di un tag TIFF che può contenere più offset (es.
/// `SubIFDs`). Limitato a `MAX_SUBIFD_FANOUT` elementi per sicurezza.
fn resolve_all_u32(
    buf: &[u8],
    endian: Endian,
    value_off: usize,
    field_type: u16,
    count: u32,
) -> Vec<u32> {
    let sz = type_size(field_type);
    if sz == 0 || count == 0 || count > MAX_SUBIFD_FANOUT {
        return Vec::new();
    }
    let inline = sz.saturating_mul(count as usize) <= 4;
    let Some(base) = (if inline {
        Some(value_off)
    } else {
        endian.u32(buf, value_off).map(|o| o as usize)
    }) else {
        return Vec::new();
    };
    let mut out = Vec::with_capacity(count as usize);
    for i in 0..count as usize {
        let off = base + i * sz;
        let v = match field_type {
            3 | 8 => endian.u16(buf, off).map(u32::from),
            4 | 9 => endian.u32(buf, off),
            _ => None,
        };
        match v {
            Some(v) => out.push(v),
            None => break,
        }
    }
    out
}

fn extract_from_tiff(buf: &[u8]) -> Result<Option<RawPreview>, RawError> {
    let endian = if buf.starts_with(b"II") {
        Endian::Little
    } else {
        Endian::Big
    };
    let first_ifd = endian
        .u32(buf, 4)
        .ok_or_else(|| RawError::Corrupt("truncated TIFF header".to_owned()))?;

    let mut queue = vec![first_ifd as usize];
    let mut visited: HashSet<usize> = HashSet::new();
    let mut candidates: Vec<Candidate> = Vec::new();

    while let Some(ifd_off) = queue.pop() {
        if visited.len() >= MAX_IFDS || !visited.insert(ifd_off) {
            continue;
        }
        let Some(entry_count) = endian.u16(buf, ifd_off) else {
            continue;
        };
        let entries_start = ifd_off + 2;

        let mut jpeg_start = None;
        let mut jpeg_len = None;
        let mut strip_offset = None;
        let mut strip_len = None;
        let mut strip_count = 0u32;
        let mut compression = None;

        for i in 0..usize::from(entry_count) {
            let entry_off = entries_start + i * 12;
            let (Some(tag), Some(field_type), Some(count)) = (
                endian.u16(buf, entry_off),
                endian.u16(buf, entry_off + 2),
                endian.u32(buf, entry_off + 4),
            ) else {
                break;
            };
            let value_off = entry_off + 8;

            match tag {
                0x0103 => {
                    compression = resolve_first_u32(buf, endian, value_off, field_type, count);
                }
                0x0111 => {
                    strip_offset = resolve_first_u32(buf, endian, value_off, field_type, count);
                    strip_count = count;
                }
                0x0117 => strip_len = resolve_first_u32(buf, endian, value_off, field_type, count),
                0x0201 => jpeg_start = resolve_first_u32(buf, endian, value_off, field_type, count),
                0x0202 => jpeg_len = resolve_first_u32(buf, endian, value_off, field_type, count),
                0x014A => queue.extend(
                    resolve_all_u32(buf, endian, value_off, field_type, count)
                        .into_iter()
                        .map(|o| o as usize),
                ),
                0x8769 => {
                    if let Some(off) = resolve_first_u32(buf, endian, value_off, field_type, count)
                    {
                        queue.push(off as usize);
                    }
                }
                _ => {}
            }
        }

        if let (Some(start), Some(len)) = (jpeg_start, jpeg_len) {
            push_candidate(buf, &mut candidates, start as usize, len as usize);
        }
        // Compression 6 (old-style JPEG) o 7 (JPEG): valido solo a strip
        // singolo, altrimenti sono i dati Bayer spezzati su più strip.
        if strip_count == 1 && matches!(compression, Some(6 | 7)) {
            if let (Some(start), Some(len)) = (strip_offset, strip_len) {
                push_candidate(buf, &mut candidates, start as usize, len as usize);
            }
        }

        let next_ifd_off = entries_start + usize::from(entry_count) * 12;
        if let Some(next) = endian.u32(buf, next_ifd_off) {
            if next != 0 {
                queue.push(next as usize);
            }
        }
    }

    Ok(pick_largest(buf, &candidates))
}

/// Il box `PRVW` non vive in `moov` ma in un box `uuid` di primo livello
/// separato (`eaf42b5e-1c98-4b88-b9fb-b7dc406e4d16`, verificato sui
/// campioni EOS R6/RP): il suo esatto genitore varia poco fra modelli, ma il
/// marker ASCII "PRVW" seguito dall'header a lunghezza fissa è stabile.
/// Si scansiona quindi l'intero file — `mdat` (i dati Bayer) arriva sempre
/// dopo `moov`/`uuid`, ed è statisticamente irrilevante che contenga per
/// caso la stessa sequenza di 4 byte.
fn extract_from_cr3(buf: &[u8]) -> Result<Option<RawPreview>, RawError> {
    if find_top_level_box(buf, *b"ftyp").is_none() {
        return Err(RawError::Corrupt("no ftyp box".to_owned()));
    }

    let mut candidates = Vec::new();
    let mut search_from = 0usize;
    for _ in 0..MAX_BMFF_BOXES {
        let Some(found) = find_subslice(&buf[search_from..], b"PRVW") else {
            break;
        };
        let marker = search_from + found;
        search_from = marker + 4;
        if marker < 4 || marker + 24 > buf.len() {
            continue;
        }
        let header = marker - 4;
        let jpeg_size = u32::from_be_bytes([
            buf[header + 20],
            buf[header + 21],
            buf[header + 22],
            buf[header + 23],
        ]);
        push_candidate(buf, &mut candidates, header + 24, jpeg_size as usize);
    }

    Ok(pick_largest(buf, &candidates))
}

/// Cerca un box di primo livello per tipo, seguendo la catena `size+type`
/// dell'ISO-BMFF. Usato solo per confermare la presenza di `ftyp` prima di
/// scansionare il resto del file: non gestisce size estesa a 64 bit (0/1),
/// che in pratica riguarda solo `mdat`, box che non ci serve raggiungere.
fn find_top_level_box(buf: &[u8], want: [u8; 4]) -> Option<&[u8]> {
    let mut offset = 0usize;
    for _ in 0..MAX_BMFF_BOXES {
        if offset + 8 > buf.len() {
            return None;
        }
        let size = u32::from_be_bytes([
            buf[offset],
            buf[offset + 1],
            buf[offset + 2],
            buf[offset + 3],
        ]) as usize;
        if size < 8 {
            return None;
        }
        let end = offset.checked_add(size)?;
        if end > buf.len() {
            return None;
        }
        if buf[offset + 4..offset + 8] == want {
            return Some(&buf[offset + 8..end]);
        }
        offset = end;
    }
    None
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn push_candidate(buf: &[u8], out: &mut Vec<Candidate>, offset: usize, len: usize) {
    if !(4..=MAX_PREVIEW_BYTES).contains(&len) {
        return;
    }
    let Some(end) = offset.checked_add(len) else {
        return;
    };
    if end > buf.len() || buf[offset] != 0xFF || buf[offset + 1] != 0xD8 {
        return;
    }
    out.push(Candidate { offset, len });
}

fn pick_largest(buf: &[u8], candidates: &[Candidate]) -> Option<RawPreview> {
    let mut best: Option<(u32, u32, usize, usize)> = None;
    for c in candidates {
        let Some((w, h)) = jpeg_dimensions(&buf[c.offset..c.offset + c.len]) else {
            continue;
        };
        let long_side = w.max(h);
        let is_better = best.is_none_or(|(bw, bh, _, _)| long_side > bw.max(bh));
        if is_better {
            best = Some((w, h, c.offset, c.len));
        }
    }
    best.map(|(width, height, offset, len)| RawPreview {
        bytes: buf[offset..offset + len].to_vec(),
        width,
        height,
        source: PreviewSource::Embedded,
    })
}

/// Legge le dimensioni da un marker SOF *baseline o progressivo*
/// (0xC0/0xC1/0xC2). Qualsiasi altro SOF (lossless, aritmetico, ...) fa
/// tornare `None`: non è un'immagine che i nostri decoder sanno aprire,
/// anche se comincia con un SOI valido.
fn jpeg_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 4 || bytes[0] != 0xFF || bytes[1] != 0xD8 {
        return None;
    }
    let mut i = 2usize;
    while i + 4 <= bytes.len() {
        if bytes[i] != 0xFF {
            i += 1;
            continue;
        }
        let marker = bytes[i + 1];
        if marker == 0x01 || (0xD0..=0xD7).contains(&marker) {
            i += 2;
            continue;
        }
        if marker == 0xD9 {
            return None;
        }
        let seg_len = usize::from(u16::from_be_bytes([bytes[i + 2], bytes[i + 3]]));
        if seg_len < 2 {
            return None;
        }
        let is_sof =
            (0xC0..=0xCF).contains(&marker) && marker != 0xC4 && marker != 0xC8 && marker != 0xCC;
        if is_sof {
            if marker != 0xC0 && marker != 0xC1 && marker != 0xC2 {
                return None;
            }
            if i + 9 > bytes.len() {
                return None;
            }
            let height = u16::from_be_bytes([bytes[i + 5], bytes[i + 6]]);
            let width = u16::from_be_bytes([bytes[i + 7], bytes[i + 8]]);
            if width == 0 || height == 0 {
                return None;
            }
            return Some((u32::from(width), u32::from(height)));
        }
        let next = i.checked_add(2 + seg_len)?;
        if next <= i {
            return None;
        }
        i = next;
    }
    None
}
