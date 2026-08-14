#![allow(clippy::unwrap_used, clippy::cast_possible_truncation)]

use chrono::{TimeZone, Utc};
use keeppix_media::read_exif;

fn jpeg_with_datetime_original(dt: &str) -> Vec<u8> {
    assert_eq!(dt.len(), 19);
    let mut tiff = vec![0x49, 0x49, 0x2A, 0x00, 0x08, 0x00, 0x00, 0x00];
    tiff.extend_from_slice(&[0x01, 0x00]);
    // DateTimeOriginal in IFD0 so a tiny fixture does not need an Exif IFD.
    tiff.extend_from_slice(&[
        0x32, 0x01, 0x02, 0x00, 0x14, 0x00, 0x00, 0x00, 0x1A, 0x00, 0x00, 0x00,
    ]);
    tiff.extend_from_slice(&[0, 0, 0, 0]);
    tiff.extend_from_slice(dt.as_bytes());
    tiff.push(0);

    let mut jpeg = vec![0xFF, 0xD8];
    let app1 = 6 + tiff.len();
    let len = u16::try_from(app1 + 2).unwrap();
    jpeg.extend_from_slice(&[0xFF, 0xE1, (len >> 8) as u8, len as u8]);
    jpeg.extend_from_slice(b"Exif\0\0");
    jpeg.extend(tiff);
    jpeg.extend_from_slice(&[0xFF, 0xD9]);
    jpeg
}

#[test]
fn missing_exif_falls_back_to_mtime() {
    let dir = std::env::temp_dir().join(format!("kpx-exif-m-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("a.jpg");
    std::fs::write(&path, b"not jpeg").unwrap();
    let mtime = Utc.with_ymd_and_hms(2021, 5, 6, 7, 8, 9).unwrap();
    let data = read_exif(&path, mtime).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(data.taken_at_utc, mtime);
    assert!(data.tz_assumed);
}

#[test]
fn datetime_original_wins_over_mtime() {
    let dir = std::env::temp_dir().join(format!("kpx-exif-d-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("a.jpg");
    std::fs::write(&path, jpeg_with_datetime_original("2020:01:02 03:04:05")).unwrap();
    let mtime = Utc.with_ymd_and_hms(2021, 5, 6, 7, 8, 9).unwrap();
    let data = read_exif(&path, mtime).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(
        &data.taken_at_utc.format("%Y-%m-%d").to_string(),
        "2020-01-02"
    );
}
