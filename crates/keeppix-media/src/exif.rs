use std::io::Read as _;
use std::path::Path;

use chrono::{DateTime, FixedOffset, Local, NaiveDateTime, TimeZone, Utc};
use keeppix_domain::{ExifData, GeoPoint};

const HEADER_BYTES: u64 = 128 * 1024;
const ZERO_COORDINATE_EPSILON: f64 = 1e-12;

/// Opens only the first 128 KB. Without EXIF, `taken_at` falls back to
/// `mtime`.
///
/// # Errors
/// I/O or unreadable EXIF (not a domain error: falls back to `mtime`).
pub fn read_exif(path: &Path, mtime: DateTime<Utc>) -> std::io::Result<ExifData> {
    let mut file = std::fs::File::open(path)?;
    let mut buf = Vec::new();
    file.by_ref().take(HEADER_BYTES).read_to_end(&mut buf)?;
    Ok(parse_header(&buf, mtime))
}

fn parse_header(buf: &[u8], mtime: DateTime<Utc>) -> ExifData {
    let mut data = ExifData {
        raw: serde_json::json!({}),
        taken_at_utc: mtime,
        tz_offset_minutes: local_offset_minutes(),
        tz_assumed: true,
        width: jpeg_sof_size(buf).map(|(w, _)| w),
        height: jpeg_sof_size(buf).map(|(_, h)| h),
        camera_make: None,
        camera_model: None,
        lens: None,
        iso: None,
        f_number: None,
        exposure: None,
        focal_length: None,
        gps: None,
    };

    let Some(exif) = parse_exif_bytes(buf) else {
        data.raw = serde_json::json!({ "tz_assumed": true });
        return data;
    };

    let mut raw = serde_json::Map::new();
    for field in exif.fields() {
        raw.insert(
            field.tag.to_string(),
            serde_json::Value::String(field.display_value().to_string()),
        );
    }

    data.camera_make = ascii(&exif, exif::Tag::Make);
    data.camera_model = ascii(&exif, exif::Tag::Model);
    data.lens = ascii(&exif, exif::Tag::LensModel);
    data.iso = int_tag(&exif, exif::Tag::PhotographicSensitivity);
    data.f_number = float_tag(&exif, exif::Tag::FNumber);
    data.focal_length = float_tag(&exif, exif::Tag::FocalLength);
    data.exposure = ascii(&exif, exif::Tag::ExposureTime);
    data.gps = gps_point(&exif);
    if let Some((w, h)) = pixel_size(&exif) {
        data.width = Some(w);
        data.height = Some(h);
    }

    let original = ascii(&exif, exif::Tag::DateTimeOriginal);
    let created = ascii(&exif, exif::Tag::DateTimeDigitized);
    let tiff_dt = ascii(&exif, exif::Tag::DateTime);
    let offset_str =
        ascii(&exif, exif::Tag::OffsetTimeOriginal).or_else(|| ascii(&exif, exif::Tag::OffsetTime));
    let dated = original.or(created).or(tiff_dt);

    if let Some(taken) = dated.and_then(|s| parse_exif_datetime(&s, offset_str.as_deref())) {
        data.taken_at_utc = taken.0;
        data.tz_offset_minutes = taken.1;
        data.tz_assumed = taken.2;
        raw.insert("tz_assumed".to_owned(), serde_json::Value::Bool(taken.2));
    } else {
        raw.insert("tz_assumed".to_owned(), serde_json::Value::Bool(true));
    }

    data.raw = serde_json::Value::Object(raw);
    data
}

fn parse_exif_bytes(buf: &[u8]) -> Option<exif::Exif> {
    if let Some(pos) = buf.windows(6).position(|w| w == b"Exif\0\0")
        && let Ok(exif) = exif::Reader::new().read_raw(buf[pos + 6..].to_vec())
    {
        return Some(exif);
    }
    exif::Reader::new()
        .read_from_container(&mut std::io::Cursor::new(buf))
        .ok()
}

fn local_offset_minutes() -> i32 {
    Local::now().offset().local_minus_utc() / 60
}

fn ascii(exif: &exif::Exif, tag: exif::Tag) -> Option<String> {
    let field = exif.fields().find(|f| f.tag == tag)?;
    Some(
        field
            .display_value()
            .to_string()
            .trim_matches('"')
            .to_owned(),
    )
}

fn int_tag(exif: &exif::Exif, tag: exif::Tag) -> Option<i32> {
    ascii(exif, tag)?.parse().ok()
}

fn float_tag(exif: &exif::Exif, tag: exif::Tag) -> Option<f32> {
    let s = ascii(exif, tag)?;
    if let Some((n, d)) = s.split_once('/') {
        let n: f32 = n.parse().ok()?;
        let d: f32 = d.parse().ok()?;
        if d == 0.0 {
            return None;
        }
        return Some(n / d);
    }
    s.parse().ok()
}

fn pixel_size(exif: &exif::Exif) -> Option<(i32, i32)> {
    let w = int_tag(exif, exif::Tag::PixelXDimension)?;
    let h = int_tag(exif, exif::Tag::PixelYDimension)?;
    Some((w, h))
}

fn gps_point(exif: &exif::Exif) -> Option<GeoPoint> {
    let lat = signed_dms(
        exif,
        exif::Tag::GPSLatitude,
        exif::Tag::GPSLatitudeRef,
        b'N',
        b'S',
    )?;
    let lon = signed_dms(
        exif,
        exif::Tag::GPSLongitude,
        exif::Tag::GPSLongitudeRef,
        b'E',
        b'W',
    )?;
    if lat.abs() <= ZERO_COORDINATE_EPSILON && lon.abs() <= ZERO_COORDINATE_EPSILON {
        return None;
    }
    if lat.abs() > 90.0 || lon.abs() > 180.0 {
        return None;
    }
    Some(GeoPoint { lat, lon })
}

fn signed_dms(
    exif: &exif::Exif,
    value_tag: exif::Tag,
    reference_tag: exif::Tag,
    positive: u8,
    negative: u8,
) -> Option<f64> {
    let field = exif.fields().find(|field| field.tag == value_tag)?;
    let exif::Value::Rational(parts) = &field.value else {
        return None;
    };
    let [degrees, minutes, seconds, ..] = parts.as_slice() else {
        return None;
    };
    if degrees.denom == 0 || minutes.denom == 0 || seconds.denom == 0 {
        return None;
    }
    let minutes_value = minutes.to_f64();
    let seconds_value = seconds.to_f64();
    if minutes_value >= 60.0 || seconds_value >= 60.0 {
        return None;
    }
    let value = degrees.to_f64() + minutes_value / 60.0 + seconds_value / 3600.0;
    let reference = gps_reference(exif, reference_tag)?;
    match reference.to_ascii_uppercase() {
        reference if reference == positive => Some(value),
        reference if reference == negative => Some(-value),
        _ => None,
    }
}

fn gps_reference(exif: &exif::Exif, tag: exif::Tag) -> Option<u8> {
    let field = exif.fields().find(|field| field.tag == tag)?;
    let exif::Value::Ascii(values) = &field.value else {
        return None;
    };
    values.first()?.first().copied()
}

fn parse_exif_datetime(raw: &str, offset: Option<&str>) -> Option<(DateTime<Utc>, i32, bool)> {
    let naive = NaiveDateTime::parse_from_str(raw.trim(), "%Y:%m:%d %H:%M:%S")
        .or_else(|_| NaiveDateTime::parse_from_str(raw.trim(), "%Y-%m-%d %H:%M:%S"))
        .ok()?;
    if let Some(off) = offset.and_then(parse_offset) {
        let dt = off.from_local_datetime(&naive).single()?;
        return Some((dt.with_timezone(&Utc), off.local_minus_utc() / 60, false));
    }
    let minutes = local_offset_minutes();
    let off = FixedOffset::east_opt(minutes * 60)?;
    let dt = off.from_local_datetime(&naive).single()?;
    Some((dt.with_timezone(&Utc), minutes, true))
}

fn parse_offset(raw: &str) -> Option<FixedOffset> {
    let trimmed = raw.trim().trim_matches('"');
    let (sign, rest) = trimmed.split_at(1);
    let (h, m) = rest.split_once(':')?;
    let hours: i32 = h.parse().ok()?;
    let mins: i32 = m.parse().ok()?;
    let secs = hours * 3600 + mins * 60;
    let secs = if sign == "-" { -secs } else { secs };
    FixedOffset::east_opt(secs)
}

fn jpeg_sof_size(buf: &[u8]) -> Option<(i32, i32)> {
    let mut i = 0;
    while i + 9 < buf.len() {
        if buf[i] != 0xFF {
            i += 1;
            continue;
        }
        let marker = buf[i + 1];
        if marker == 0xD8 || marker == 0xD9 {
            i += 2;
            continue;
        }
        if i + 3 >= buf.len() {
            break;
        }
        let len = u16::from_be_bytes([buf[i + 2], buf[i + 3]]) as usize;
        if matches!(marker, 0xC0..=0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF)
            && i + 8 < buf.len()
        {
            let h = i32::from(u16::from_be_bytes([buf[i + 5], buf[i + 6]]));
            let w = i32::from(u16::from_be_bytes([buf[i + 7], buf[i + 8]]));
            return Some((w, h));
        }
        i += 2 + len;
    }
    None
}
