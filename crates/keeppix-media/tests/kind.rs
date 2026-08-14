use keeppix_domain::AssetKind;
use keeppix_media::detect_kind;

#[test]
fn jpeg_magic_is_an_image() {
    assert_eq!(detect_kind(&[0xFF, 0xD8, 0xFF, 0xE0]), AssetKind::Image);
}

#[test]
fn png_magic_is_an_image() {
    assert_eq!(
        detect_kind(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]),
        AssetKind::Image
    );
}

#[test]
fn sony_arw_tiff_header_is_raw() {
    let mut header = vec![0x49, 0x49, 0x2A, 0x00, 0x08, 0x00, 0x00, 0x00];
    header.extend_from_slice(&[0; 32]);
    header.extend_from_slice(b"SONY");
    assert_eq!(detect_kind(&header), AssetKind::RawImage);
}

#[test]
fn mp4_ftyp_is_video() {
    let mut header = vec![0, 0, 0, 0x20];
    header.extend_from_slice(b"ftypisom");
    header.extend_from_slice(&[0; 16]);
    assert_eq!(detect_kind(&header), AssetKind::Video);
}

#[test]
fn a_jpg_named_file_with_text_content_is_unknown() {
    assert_eq!(detect_kind(b"this is not a jpeg"), AssetKind::Unknown);
}
