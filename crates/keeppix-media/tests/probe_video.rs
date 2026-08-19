#![allow(clippy::unwrap_used, clippy::expect_used)]

use keeppix_media::{Capabilities, VideoBackend, probe};

#[test]
fn probe_never_returns_unprobed() {
    let caps = probe();
    assert_ne!(
        serde_json::to_value(caps.backend).expect("serialize"),
        serde_json::json!("unprobed"),
        "the hardware probe must pick a concrete backend"
    );
}

#[test]
fn probe_returns_software_when_ffmpeg_is_missing() {
    let path = std::env::var("PATH").unwrap_or_default();
    let trimmed = path
        .split(':')
        .filter(|entry| !entry.contains("ffmpeg") && *entry != "/usr/bin" && *entry != "/bin")
        .collect::<Vec<_>>()
        .join(":");
    // If we cannot hide ffmpeg, skip — CI always has it in /usr/bin.
    if trimmed == path {
        return;
    }
    unsafe {
        std::env::set_var("PATH", trimmed);
    }
    let caps = probe();
    unsafe {
        std::env::set_var("PATH", path);
    }
    assert_eq!(caps.backend, VideoBackend::Software);
    assert!(caps.decode_fps.is_none());
}

#[test]
fn probe_result_is_json_serializable_with_extra_object() {
    let caps = probe();
    let value = serde_json::to_value(&caps).expect("serialize capabilities");
    assert!(value.get("backend").is_some());
    assert!(value.get("decode_fps").is_some());
    let extra = value.get("extra").expect("extra field");
    assert!(extra.is_object());
}

#[test]
fn probe_completes_within_a_few_seconds() {
    let started = std::time::Instant::now();
    let _caps: Capabilities = probe();
    assert!(
        started.elapsed().as_secs() <= 8,
        "probe took {:?}, expected under ~4s plus CI slack",
        started.elapsed()
    );
}

#[test]
fn probe_measures_fps_when_ffmpeg_works() {
    if !keeppix_media::video::ffprobe_available() {
        return;
    }
    let caps = probe();
    assert!(
        caps.decode_fps
            .is_some_and(|fps| fps.is_finite() && fps > 0.0),
        "a working ffmpeg install should yield a measured fps for at least software encoding"
    );
}
