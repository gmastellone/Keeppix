use chrono::{TimeZone, Utc};
use keeppix_media::gpx::{DEFAULT_TOLERANCE, interpolate, parse};

const GPX: &str = r#"<?xml version="1.0"?>
<gpx version="1.1" creator="keeppix-test">
  <trk><trkseg>
    <trkpt lat="40.0" lon="8.0"><time>2026-08-18T10:00:00Z</time></trkpt>
    <trkpt lat="50.0" lon="18.0"><time>2026-08-18T10:10:00Z</time></trkpt>
  </trkseg></trk>
</gpx>"#;

#[test]
#[allow(clippy::expect_used, clippy::unwrap_used)]
fn interpolation_is_linear_and_default_tolerance_uses_endpoints_without_extrapolating() {
    let track = parse(GPX.as_bytes()).expect("valid GPX");

    let before = interpolate(
        &track,
        Utc.with_ymd_and_hms(2026, 8, 18, 9, 58, 0).unwrap(),
        DEFAULT_TOLERANCE,
    )
    .expect("two minutes before is within tolerance");
    assert!((before.lat - 40.0).abs() < 1e-9);
    assert!((before.lon - 8.0).abs() < 1e-9);

    let middle = interpolate(
        &track,
        Utc.with_ymd_and_hms(2026, 8, 18, 10, 5, 0).unwrap(),
        DEFAULT_TOLERANCE,
    )
    .expect("time inside track");
    assert!((middle.lat - 45.0).abs() < 1e-9);
    assert!((middle.lon - 13.0).abs() < 1e-9);

    let after = interpolate(
        &track,
        Utc.with_ymd_and_hms(2026, 8, 18, 10, 12, 0).unwrap(),
        DEFAULT_TOLERANCE,
    )
    .expect("two minutes after is within tolerance");
    assert!((after.lat - 50.0).abs() < 1e-9);
    assert!((after.lon - 18.0).abs() < 1e-9);

    assert!(
        interpolate(
            &track,
            Utc.with_ymd_and_hms(2026, 8, 18, 10, 30, 0).unwrap(),
            DEFAULT_TOLERANCE,
        )
        .is_none(),
        "twenty minutes after the last point exceeds the five-minute tolerance"
    );
}

#[test]
#[allow(clippy::expect_used)]
fn malformed_or_coordinate_free_gpx_is_rejected() {
    assert!(parse(b"<gpx><trkpt>").is_err());
    assert!(parse(b"<gpx version=\"1.1\"></gpx>").is_err());
}
