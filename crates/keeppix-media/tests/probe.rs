#![allow(clippy::unwrap_used)]

#[test]
fn probe_reports_a_concrete_backend() {
    let caps = keeppix_media::probe();
    assert!(
        caps.decode_fps.is_none()
            || caps
                .decode_fps
                .is_some_and(|fps| fps.is_finite() && fps > 0.0),
        "decode_fps must be absent or a positive finite measurement"
    );
    assert!(caps.extra.is_object());
}
