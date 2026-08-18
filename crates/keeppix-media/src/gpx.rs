//! Parsing e matching temporale di tracce GPX. Nessun database e nessun I/O:
//! il chiamante fornisce i byte del documento e i timestamp da abbinare.

use chrono::{DateTime, Duration, Utc};
use keeppix_domain::GeoPoint;
use quick_xml::events::{BytesStart, Event};
use quick_xml::reader::Reader;
use thiserror::Error;

/// Tolleranza predefinita richiesta dal flusso di import: cinque minuti.
pub const DEFAULT_TOLERANCE: Duration = Duration::minutes(5);

#[derive(Debug, Error)]
pub enum GpxError {
    #[error("malformed GPX: {0}")]
    Malformed(String),
    #[error("GPX track contains no timestamped points")]
    NoTrackPoints,
}

#[derive(Debug, Clone, Copy)]
struct TrackPoint {
    at: DateTime<Utc>,
    point: GeoPoint,
}

/// Traccia normalizzata e ordinata per timestamp.
#[derive(Debug, Clone)]
pub struct Track {
    points: Vec<TrackPoint>,
}

/// Legge i `trkpt` con coordinate e timestamp RFC 3339.
///
/// # Errors
/// `Malformed` per XML non valido, coordinate fuori WGS84 o timestamp non
/// RFC 3339; `NoTrackPoints` se il documento non contiene punti utilizzabili.
pub fn parse(bytes: &[u8]) -> Result<Track, GpxError> {
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut depth = 0_usize;
    let mut current_point = None;
    let mut reading_time = false;
    let mut time_text = String::new();
    let mut points = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(start)) => {
                depth = depth.saturating_add(1);
                match start.local_name().as_ref() {
                    b"trkpt" => current_point = Some(point_from(&start)?),
                    b"time" if current_point.is_some() => {
                        reading_time = true;
                        time_text.clear();
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(text)) if reading_time => {
                let decoded = text
                    .decode()
                    .map_err(|e| GpxError::Malformed(format!("invalid time text: {e}")))?;
                time_text.push_str(&decoded);
            }
            Ok(Event::End(end)) => {
                match end.local_name().as_ref() {
                    b"time" if reading_time => {
                        let at = DateTime::parse_from_rfc3339(time_text.trim())
                            .map_err(|e| GpxError::Malformed(format!("invalid track time: {e}")))?
                            .with_timezone(&Utc);
                        if let Some(point) = current_point {
                            points.push(TrackPoint { at, point });
                        }
                        reading_time = false;
                    }
                    b"trkpt" => {
                        current_point = None;
                        reading_time = false;
                    }
                    _ => {}
                }
                depth = depth.saturating_sub(1);
            }
            Ok(Event::Eof) => {
                if depth != 0 {
                    return Err(GpxError::Malformed(
                        "document ended before all elements were closed".to_owned(),
                    ));
                }
                break;
            }
            Ok(_) => {}
            Err(e) => return Err(GpxError::Malformed(e.to_string())),
        }
        buf.clear();
    }

    if points.is_empty() {
        return Err(GpxError::NoTrackPoints);
    }
    points.sort_unstable_by_key(|point| point.at);
    points.dedup_by_key(|point| point.at);
    Ok(Track { points })
}

/// Abbina `at` alla traccia. Fra due punti interpola linearmente; entro la
/// tolleranza fuori dagli estremi usa la coordinata dell'estremo, senza
/// estrapolare una velocità inventata.
#[must_use]
pub fn interpolate(track: &Track, at: DateTime<Utc>, tolerance: Duration) -> Option<GeoPoint> {
    let first = track.points.first()?;
    let last = track.points.last()?;

    if at < first.at {
        return (first.at - at <= tolerance).then_some(first.point);
    }
    if at > last.at {
        return (at - last.at <= tolerance).then_some(last.point);
    }
    if at == first.at {
        return Some(first.point);
    }
    if at == last.at {
        return Some(last.point);
    }

    let pair = track
        .points
        .windows(2)
        .find(|pair| pair[0].at <= at && at <= pair[1].at)?;
    let total_ms = (pair[1].at - pair[0].at).num_milliseconds();
    if total_ms == 0 {
        return Some(pair[0].point);
    }
    let elapsed_ms = (at - pair[0].at).num_milliseconds();
    #[allow(clippy::cast_precision_loss)]
    let ratio = elapsed_ms as f64 / total_ms as f64;
    Some(GeoPoint {
        lat: pair[0].point.lat + (pair[1].point.lat - pair[0].point.lat) * ratio,
        lon: pair[0].point.lon + (pair[1].point.lon - pair[0].point.lon) * ratio,
    })
}

fn point_from(start: &BytesStart<'_>) -> Result<GeoPoint, GpxError> {
    let mut lat = None;
    let mut lon = None;
    for attr in start.attributes() {
        let attr =
            attr.map_err(|e| GpxError::Malformed(format!("invalid trkpt attribute: {e}")))?;
        let value = std::str::from_utf8(attr.value.as_ref())
            .map_err(|e| GpxError::Malformed(format!("non-UTF-8 trkpt attribute: {e}")))?;
        match attr.key.local_name().as_ref() {
            b"lat" => lat = Some(parse_coordinate(value, -90.0, 90.0, "latitude")?),
            b"lon" => lon = Some(parse_coordinate(value, -180.0, 180.0, "longitude")?),
            _ => {}
        }
    }
    let lat = lat.ok_or_else(|| GpxError::Malformed("trkpt is missing latitude".to_owned()))?;
    let lon = lon.ok_or_else(|| GpxError::Malformed("trkpt is missing longitude".to_owned()))?;
    Ok(GeoPoint { lat, lon })
}

fn parse_coordinate(raw: &str, min: f64, max: f64, name: &str) -> Result<f64, GpxError> {
    let value: f64 = raw
        .parse()
        .map_err(|e| GpxError::Malformed(format!("invalid {name}: {e}")))?;
    if !value.is_finite() || !(min..=max).contains(&value) {
        return Err(GpxError::Malformed(format!("{name} is outside WGS84")));
    }
    Ok(value)
}
