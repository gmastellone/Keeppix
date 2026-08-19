#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;
use std::process::Command;
use std::time::Duration;

use keeppix_media::transcode::{
    PlaybackMode, TranscodeProfile, VideoStreamInfo, client_accepts_direct, playback_mode,
    probe_stream, transcode_cache_dir, transcode_to_hls,
};
use keeppix_media::{VideoBackend, video};

fn sample_h264_720p() -> VideoStreamInfo {
    VideoStreamInfo {
        container: keeppix_media::transcode::ContainerKind::Mp4,
        codec: keeppix_media::transcode::VideoCodec::H264,
        width: 1280,
        height: 720,
        duration: Duration::from_secs(10),
        rotation: None,
        hdr: None,
    }
}

#[test]
fn h264_mp4_1080p_direct_play_without_save_bandwidth() {
    let info = VideoStreamInfo {
        height: 1080,
        ..sample_h264_720p()
    };
    assert_eq!(playback_mode(&info, false), PlaybackMode::DirectPlay);
}

#[test]
fn h264_4k_transcodes_when_save_bandwidth_is_set() {
    let info = VideoStreamInfo {
        height: 2160,
        ..sample_h264_720p()
    };
    assert_eq!(
        playback_mode(&info, true),
        PlaybackMode::Transcode(TranscodeProfile::SaveBandwidth720p)
    );
}

#[test]
fn h264_4k_direct_play_on_wifi() {
    let info = VideoStreamInfo {
        height: 2160,
        ..sample_h264_720p()
    };
    assert_eq!(playback_mode(&info, false), PlaybackMode::DirectPlay);
}

#[test]
fn hevc_always_transcodes() {
    let info = VideoStreamInfo {
        codec: keeppix_media::transcode::VideoCodec::Hevc,
        ..sample_h264_720p()
    };
    assert_eq!(
        playback_mode(&info, false),
        PlaybackMode::Transcode(TranscodeProfile::SaveBandwidth720p)
    );
}

#[test]
fn hdr_always_transcodes_with_tone_mapping_profile() {
    let info = VideoStreamInfo {
        hdr: Some(keeppix_media::transcode::HdrKind::Pq),
        ..sample_h264_720p()
    };
    assert_eq!(
        playback_mode(&info, false),
        PlaybackMode::Transcode(TranscodeProfile::Hdr720p)
    );
}

#[test]
fn client_accepts_mp4_when_accept_includes_video_mp4() {
    assert!(client_accepts_direct(
        &sample_h264_720p(),
        Some("video/mp4,*/*")
    ));
    assert!(!client_accepts_direct(
        &sample_h264_720p(),
        Some("video/webm")
    ));
}

#[test]
fn transcode_cache_dir_is_stable_under_data_dir() {
    let hash = [0xab_u8; 32];
    let dir = transcode_cache_dir(
        Path::new("/data"),
        &hash,
        TranscodeProfile::SaveBandwidth720p,
    );
    assert!(dir.starts_with("/data/video-cache"));
    assert!(dir.to_string_lossy().contains("720p"));
}

#[test]
fn hls_playlist_is_written_for_a_tiny_clip() {
    if !video::ffprobe_available() {
        eprintln!("skipping: ffprobe not in PATH");
        return;
    }
    let dir = std::env::temp_dir().join(format!("kpx-hls-{}", uuid_like()));
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("clip.mp4");
    let status = Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=c=red:s=64x64:d=2",
            "-t",
            "2",
            "-pix_fmt",
            "yuv420p",
        ])
        .arg(&src)
        .status()
        .expect("spawn ffmpeg");
    if !status.success() {
        eprintln!("skipping: ffmpeg could not write a clip");
        let _ = std::fs::remove_dir_all(&dir);
        return;
    }
    let cache = dir.join("cache");
    transcode_to_hls(
        &src,
        &cache,
        TranscodeProfile::SaveBandwidth720p,
        VideoBackend::Software,
    )
    .expect("transcode");
    let playlist = cache.join("index.m3u8");
    assert!(playlist.is_file(), "master playlist missing");
    let body = std::fs::read_to_string(&playlist).expect("read playlist");
    assert!(body.contains("#EXTM3U"));
    assert!(body.contains(".ts") || body.contains(".m4s"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn probe_stream_reads_codec_and_resolution() {
    if !video::ffprobe_available() {
        eprintln!("skipping: ffprobe not in PATH");
        return;
    }
    let dir = std::env::temp_dir().join(format!("kpx-probe-{}", uuid_like()));
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("clip.mp4");
    let status = Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=c=green:s=320x240:d=1",
            "-t",
            "1",
            "-pix_fmt",
            "yuv420p",
        ])
        .arg(&src)
        .status()
        .expect("spawn ffmpeg");
    if !status.success() {
        let _ = std::fs::remove_dir_all(&dir);
        return;
    }
    let info = probe_stream(&src).expect("probe");
    assert_eq!(info.codec, keeppix_media::transcode::VideoCodec::H264);
    assert_eq!(info.width, 320);
    assert_eq!(info.height, 240);
    let _ = std::fs::remove_dir_all(&dir);
}

fn uuid_like() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}
