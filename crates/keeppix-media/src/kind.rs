use keeppix_domain::AssetKind;

/// Asset type from magic numbers, not from the extension.
///
/// A text file renamed to `.jpg` stays `Unknown`. A Sony ARW stays
/// `RawImage` even though its container is TIFF.
#[must_use]
pub fn detect_kind(header: &[u8]) -> AssetKind {
    if header.len() >= 3 && header[0] == 0xFF && header[1] == 0xD8 && header[2] == 0xFF {
        return AssetKind::Image;
    }
    if header.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
        return AssetKind::Image;
    }
    if header.starts_with(b"GIF8") {
        return AssetKind::Image;
    }
    if header.starts_with(b"FUJIFILMCCD-RAW") {
        return AssetKind::RawImage;
    }
    if header.starts_with(b"IIRO") || header.starts_with(b"MMOR") || header.starts_with(b"IIRS") {
        return AssetKind::RawImage;
    }
    if header.len() >= 12 && header.starts_with(b"RIFF") {
        let fourcc = &header[8..12];
        if fourcc == b"WEBP" {
            return AssetKind::Image;
        }
        if fourcc == b"AVI " {
            return AssetKind::Video;
        }
    }
    if header.starts_with(&[0x1A, 0x45, 0xDF, 0xA3]) {
        return AssetKind::Video;
    }
    if header.len() >= 12 && &header[4..8] == b"ftyp" {
        return ftyp_kind(&header[8..]);
    }
    if is_tiff(header) {
        if looks_like_camera_raw(header) {
            return AssetKind::RawImage;
        }
        return AssetKind::Image;
    }
    AssetKind::Unknown
}

fn ftyp_kind(after_ftyp: &[u8]) -> AssetKind {
    for chunk in after_ftyp.chunks_exact(4) {
        match chunk {
            b"crx " => return AssetKind::RawImage,
            b"heic" | b"heix" | b"heif" | b"heim" | b"heis" | b"mif1" | b"msf1" | b"avif"
            | b"avis" => return AssetKind::Image,
            b"isom" | b"iso2" | b"mp41" | b"mp42" | b"avc1" | b"qt  " | b"M4V " | b"3gp4"
            | b"3gp5" | b"dash" | b"mmp4" => return AssetKind::Video,
            _ => {}
        }
    }
    AssetKind::Video
}

fn is_tiff(header: &[u8]) -> bool {
    header.starts_with(&[0x49, 0x49, 0x2A, 0x00]) || header.starts_with(&[0x4D, 0x4D, 0x00, 0x2A])
}

fn looks_like_camera_raw(header: &[u8]) -> bool {
    if header.len() >= 10 && header[8] == b'C' && header[9] == b'R' {
        return true;
    }
    contains(header, b"SONY")
        || contains(header, b"NIKON")
        || contains(header, b"Canon")
        || contains(header, b"Panasonic")
        || contains(header, b"OLYMPUS")
        || contains(header, b"FUJIFILM")
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}
