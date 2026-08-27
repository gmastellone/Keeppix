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

/// Traccia normalizzata per timestamp, con i confini fra segmenti preservati.
#[derive(Debug, Clone)]
pub struct Track {
    segments: Vec<Vec<TrackPoint>>,
}

/// Legge i `trkpt` con coordinate e timestamp RFC 3339, mantenendo ogni
/// `trkseg` separato dagli altri segmenti e dalle altre tracce.
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
    let mut current_segment = None;
    let mut reading_time = false;
    let mut time_text = String::new();
    let mut segments = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(start)) => {
                depth = depth.saturating_add(1);
                match start.local_name().as_ref() {
                    b"trkseg" => {
                        if current_segment.is_some() {
                            return Err(GpxError::Malformed("nested trkseg elements".to_owned()));
                        }
                        current_segment = Some(Vec::new());
                    }
                    b"trkpt" if current_segment.is_some() => {
                        current_point = Some(point_from(&start)?);
                    }
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
                        if let (Some(point), Some(segment)) =
                            (current_point, current_segment.as_mut())
                        {
                            segment.push(TrackPoint { at, point });
                        }
                        reading_time = false;
                    }
                    b"trkpt" => {
                        current_point = None;
                        reading_time = false;
                    }
                    b"trkseg" => {
                        if let Some(mut segment) = current_segment.take()
                            && !segment.is_empty()
                        {
                            segment.sort_unstable_by_key(|point| point.at);
                            segment.dedup_by_key(|point| point.at);
                            segments.push(segment);
                        }
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

    if segments.is_empty() {
        return Err(GpxError::NoTrackPoints);
    }
    Ok(Track { segments })
}

/// Matches `at` to the track. Between two points of the same segment it
/// interpolates linearly; in gaps between segments and outside the track it
/// uses the temporally nearest endpoint, only within `tolerance`.
#[must_use]
pub fn interpolate(track: &Track, at: DateTime<Utc>, tolerance: Duration) -> Option<GeoPoint> {
    if let Some(point) = track
        .segments
        .iter()
        .find_map(|segment| interpolate_segment(segment, at))
    {
        return Some(point);
    }

    let nearest = track
        .segments
        .iter()
        .flat_map(|segment| [segment.first(), segment.last()])
        .flatten()
        .map(|endpoint| {
            let delta = if at >= endpoint.at {
                at - endpoint.at
            } else {
                endpoint.at - at
            };
            (delta, endpoint.point)
        })
        .min_by_key(|(delta, _)| *delta)?;
    (nearest.0 <= tolerance).then_some(nearest.1)
}

fn interpolate_segment(segment: &[TrackPoint], at: DateTime<Utc>) -> Option<GeoPoint> {
    let first = segment.first()?;
    let last = segment.last()?;
    if at < first.at || at > last.at {
        return None;
    }
    if at == first.at {
        return Some(first.point);
    }
    if at == last.at {
        return Some(last.point);
    }

    let pair = segment
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
