#![allow(clippy::expect_used, clippy::unwrap_used)]

use chrono::{TimeZone as _, Utc};
use keeppix_domain::GeoPoint;
use keeppix_media::read_exif;

type Dms = [(u32, u32); 3];

fn push_u16(bytes: &mut Vec<u8>, value: u16) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn push_u32(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn push_ifd_entry(bytes: &mut Vec<u8>, tag: u16, kind: u16, count: u32, value: u32) {
    push_u16(bytes, tag);
    push_u16(bytes, kind);
    push_u32(bytes, count);
    push_u32(bytes, value);
}

fn tiff_without_gps_ifd() -> Vec<u8> {
    let mut tiff = vec![b'I', b'I', 0x2A, 0x00];
    push_u32(&mut tiff, 8);
    push_u16(&mut tiff, 0);
    push_u32(&mut tiff, 0);
    tiff
}

fn tiff_with_maker_note_gps() -> Vec<u8> {
    const EXIF_IFD_OFFSET: u32 = 26;
    const MAKER_NOTE_OFFSET: u32 = 44;
    const MAKER_NOTE_LENGTH: u32 = 102;
    const LATITUDE_OFFSET: u32 = 98;
    const LONGITUDE_OFFSET: u32 = 122;

    let mut tiff = vec![b'I', b'I', 0x2A, 0x00];
    push_u32(&mut tiff, 8);

    push_u16(&mut tiff, 1);
    push_ifd_entry(&mut tiff, 0x8769, 4, 1, EXIF_IFD_OFFSET);
    push_u32(&mut tiff, 0);

    push_u16(&mut tiff, 1);
    push_ifd_entry(&mut tiff, 0x927C, 7, MAKER_NOTE_LENGTH, MAKER_NOTE_OFFSET);
    push_u32(&mut tiff, 0);

    // A proprietary MakerNote IFD whose private tags mirror GPS DMS fields.
    // There is deliberately no standard GPSInfo pointer (0x8825) in IFD0.
    push_u16(&mut tiff, 4);
    push_ifd_entry(&mut tiff, 0x0001, 2, 2, u32::from(b'S'));
    push_ifd_entry(&mut tiff, 0x0002, 5, 3, LATITUDE_OFFSET);
    push_ifd_entry(&mut tiff, 0x0003, 2, 2, u32::from(b'W'));
    push_ifd_entry(&mut tiff, 0x0004, 5, 3, LONGITUDE_OFFSET);
    push_u32(&mut tiff, 0);
    for (numerator, denominator) in [(34, 1), (30, 1), (0, 1), (58, 1), (22, 1), (30, 1)] {
        push_u32(&mut tiff, numerator);
        push_u32(&mut tiff, denominator);
    }
    tiff
}

fn tiff_with_gps(lat_ref: u8, latitude: Dms, lon_ref: u8, longitude: Dms) -> Vec<u8> {
    const GPS_IFD_OFFSET: u32 = 26;
    const LATITUDE_OFFSET: u32 = 80;
    const LONGITUDE_OFFSET: u32 = 104;

    let mut tiff = vec![b'I', b'I', 0x2A, 0x00];
    push_u32(&mut tiff, 8);

    push_u16(&mut tiff, 1);
    push_ifd_entry(&mut tiff, 0x8825, 4, 1, GPS_IFD_OFFSET);
    push_u32(&mut tiff, 0);

    push_u16(&mut tiff, 4);
    push_ifd_entry(&mut tiff, 0x0001, 2, 2, u32::from(lat_ref));
    push_ifd_entry(&mut tiff, 0x0002, 5, 3, LATITUDE_OFFSET);
    push_ifd_entry(&mut tiff, 0x0003, 2, 2, u32::from(lon_ref));
    push_ifd_entry(&mut tiff, 0x0004, 5, 3, LONGITUDE_OFFSET);
    push_u32(&mut tiff, 0);

    for (numerator, denominator) in latitude.into_iter().chain(longitude) {
        push_u32(&mut tiff, numerator);
        push_u32(&mut tiff, denominator);
    }
    tiff
}

fn jpeg_with_tiff(tiff: &[u8]) -> Vec<u8> {
    let mut jpeg = vec![0xFF, 0xD8];
    let segment_len = u16::try_from(2 + 6 + tiff.len()).expect("fixture APP1 fits in u16");
    jpeg.extend_from_slice(&[0xFF, 0xE1]);
    jpeg.extend_from_slice(&segment_len.to_be_bytes());
    jpeg.extend_from_slice(b"Exif\0\0");
    jpeg.extend_from_slice(tiff);
    jpeg.extend_from_slice(&[0xFF, 0xD9]);
    jpeg
}

fn read_fixture(bytes: &[u8], extension: &str) -> keeppix_domain::ExifData {
    let dir = tempfile::tempdir().expect("temporary fixture directory");
    let path = dir.path().join(format!("fixture.{extension}"));
    std::fs::write(&path, bytes).expect("write EXIF fixture");
    let mtime = Utc.with_ymd_and_hms(2024, 1, 2, 3, 4, 5).unwrap();
    read_exif(&path, mtime).expect("read EXIF fixture")
}

fn assert_point_close(actual: GeoPoint, expected: GeoPoint) {
    assert!((actual.lat - expected.lat).abs() < 1e-9);
    assert!((actual.lon - expected.lon).abs() < 1e-9);
}

#[test]
fn jpeg_without_gps_tags_has_no_gps() {
    let data = read_fixture(&jpeg_with_tiff(&tiff_without_gps_ifd()), "jpg");

    assert_eq!(data.gps, None);
}

#[test]
fn south_and_west_references_make_coordinates_negative() {
    let tiff = tiff_with_gps(
        b'S',
        [(34, 1), (30, 1), (0, 1)],
        b'W',
        [(58, 1), (22, 1), (30, 1)],
    );
    let data = read_fixture(&jpeg_with_tiff(&tiff), "jpg");

    assert_point_close(
        data.gps.expect("standard GPS IFD is parsed"),
        GeoPoint {
            lat: -34.5,
            lon: -58.375,
        },
    );
}

#[test]
fn zero_zero_without_a_fix_has_no_gps() {
    let tiff = tiff_with_gps(
        b'N',
        [(0, 1), (0, 1), (0, 1)],
        b'E',
        [(0, 1), (0, 1), (0, 1)],
    );
    let data = read_fixture(&jpeg_with_tiff(&tiff), "jpg");

    assert_eq!(data.gps, None);
}

#[test]
fn a_zero_on_only_one_axis_is_a_valid_coordinate() {
    let tiff = tiff_with_gps(
        b'N',
        [(0, 1), (0, 1), (0, 1)],
        b'E',
        [(12, 1), (30, 1), (0, 1)],
    );
    let data = read_fixture(&jpeg_with_tiff(&tiff), "jpg");

    assert_point_close(
        data.gps.expect("equator with non-zero longitude is valid"),
        GeoPoint {
            lat: 0.0,
            lon: 12.5,
        },
    );
}

#[test]
fn maker_note_only_gps_is_ignored_without_error() {
    let tiff = tiff_with_maker_note_gps();
    let parsed = exif::Reader::new()
        .read_raw(tiff.clone())
        .expect("fixture is valid TIFF");
    let maker_note = parsed
        .fields()
        .find(|field| field.tag == exif::Tag::MakerNote)
        .expect("fixture contains a real MakerNote tag");
    let exif::Value::Undefined(payload, _) = &maker_note.value else {
        panic!("MakerNote payload must be binary");
    };
    assert_eq!(payload.len(), 102);
    assert!(
        parsed.fields().all(|field| !matches!(
            field.tag,
            exif::Tag::GPSLatitude
                | exif::Tag::GPSLatitudeRef
                | exif::Tag::GPSLongitude
                | exif::Tag::GPSLongitudeRef
        )),
        "fixture must not expose standard GPS fields"
    );

    let data = read_fixture(&tiff, "arw");

    assert_eq!(data.gps, None);
}
