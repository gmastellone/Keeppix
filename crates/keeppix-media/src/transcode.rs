use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::probe::VideoBackend;
use crate::sandbox;
use crate::video;

/// Virtual address ceiling for sandboxed ffmpeg during HLS transcodes.
/// 1 GiB was enough on quiet hosts; GitHub runners under parallel `cargo test`
/// hit `Error while filtering: Resource temporarily unavailable` (EAGAIN on
/// thread create) — 2 GiB + single-thread encode below fixes that.
const TRANSCODE_MEM: u64 = 2 * 1024 * 1024 * 1024;
/// Longer budget than poster extract: a short clip can take tens of seconds
/// on software ARM.
const TRANSCODE_CPU: u64 = 600;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerKind {
    Mp4,
    Mov,
    Mkv,
    Webm,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoCodec {
    H264,
    Hevc,
    Av1,
    Vp9,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HdrKind {
    Hlg,
    Pq,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoStreamInfo {
    pub container: ContainerKind,
    pub codec: VideoCodec,
    pub width: u32,
    pub height: u32,
    pub duration: Duration,
    pub rotation: Option<i32>,
    pub hdr: Option<HdrKind>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TranscodeProfile {
    SaveBandwidth720p,
    Hdr720p,
}

impl TranscodeProfile {
    #[must_use]
    pub const fn dir_name(self) -> &'static str {
        match self {
            Self::SaveBandwidth720p => "720p",
            Self::Hdr720p => "720p-hdr",
        }
    }

    #[must_use]
    pub const fn max_height() -> u32 {
        720
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackMode {
    DirectPlay,
    Transcode(TranscodeProfile),
}

/// Decide whether to serve the original or transcode on demand.
#[must_use]
pub fn playback_mode(info: &VideoStreamInfo, save_bandwidth: bool) -> PlaybackMode {
    if info.hdr.is_some() {
        return PlaybackMode::Transcode(TranscodeProfile::Hdr720p);
    }
    if matches!(
        info.codec,
        VideoCodec::Hevc | VideoCodec::Av1 | VideoCodec::Vp9
    ) || matches!(info.container, ContainerKind::Mkv | ContainerKind::Webm)
    {
        return PlaybackMode::Transcode(TranscodeProfile::SaveBandwidth720p);
    }
    if info.codec == VideoCodec::H264
        && matches!(info.container, ContainerKind::Mp4 | ContainerKind::Mov)
    {
        if info.height > 1080 && save_bandwidth {
            return PlaybackMode::Transcode(TranscodeProfile::SaveBandwidth720p);
        }
        if info.height <= 1080 || !save_bandwidth {
            return PlaybackMode::DirectPlay;
        }
    }
    PlaybackMode::Transcode(TranscodeProfile::SaveBandwidth720p)
}

/// Whether the client's `Accept` header can play the original container.
#[must_use]
pub fn client_accepts_direct(info: &VideoStreamInfo, accept: Option<&str>) -> bool {
    let Some(accept) = accept else {
        return false;
    };
    let lower = accept.to_ascii_lowercase();
    if lower.contains("*/*") {
        return true;
    }
    for mime in direct_mimes(info) {
        if lower.contains(mime) {
            return true;
        }
    }
    false
}

fn direct_mimes(info: &VideoStreamInfo) -> &'static [&'static str] {
    match info.container {
        ContainerKind::Mp4 => &["video/mp4"],
        ContainerKind::Mov => &["video/quicktime", "video/mp4"],
        ContainerKind::Webm => &["video/webm"],
        ContainerKind::Mkv => &["video/x-matroska"],
        ContainerKind::Other => &[],
    }
}

#[must_use]
pub fn transcode_cache_dir(data_dir: &Path, hash: &[u8; 32], profile: TranscodeProfile) -> PathBuf {
    data_dir
        .join("video-cache")
        .join(hex32(hash))
        .join(profile.dir_name())
}

#[must_use]
pub fn playlist_path(cache_dir: &Path) -> PathBuf {
    cache_dir.join("index.m3u8")
}

#[must_use]
pub fn cache_is_ready(cache_dir: &Path) -> bool {
    playlist_path(cache_dir).is_file()
}

/// Updates the cache directory mtime for 90-day cleanup.
///
/// # Errors
/// Filesystem.
pub fn touch_cache(cache_dir: &Path) -> std::io::Result<()> {
    if cache_dir.is_dir() {
        let now = filetime::FileTime::now();
        filetime::set_file_mtime(cache_dir, now)?;
    }
    Ok(())
}

/// Extended ffprobe: container, codec, resolution, HDR transfer characteristics.
///
/// # Errors
/// ffprobe missing or JSON illegible.
pub fn probe_stream(path: &Path) -> Result<VideoStreamInfo, std::io::Error> {
    let path_s = path.to_string_lossy();
    let out = sandbox::run(
        "ffprobe",
        &[
            "-v",
            "error",
            "-show_entries",
            "format=format_name,duration:stream=codec_name,width,height,tags=rotate:stream_tags=rotate,color_transfer,color_primaries",
            "-select_streams",
            "v:0",
            "-of",
            "json",
            path_s.as_ref(),
        ],
        TRANSCODE_MEM,
        30,
    )?;
    if !out.status.success() {
        return Err(std::io::Error::other("ffprobe failed"));
    }
    parse_stream_probe(&out.stdout, path)
}

fn parse_stream_probe(bytes: &[u8], path: &Path) -> Result<VideoStreamInfo, std::io::Error> {
    let v: serde_json::Value = serde_json::from_slice(bytes)
        .map_err(|e| std::io::Error::other(format!("ffprobe json: {e}")))?;
    let stream = v
        .pointer("/streams/0")
        .ok_or_else(|| std::io::Error::other("ffprobe: missing video stream"))?;
    let format_name = v
        .pointer("/format/format_name")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let container = parse_container(format_name, path);
    let codec = stream
        .pointer("/codec_name")
        .and_then(serde_json::Value::as_str)
        .map_or(VideoCodec::Other, parse_codec);
    let width = stream
        .pointer("/width")
        .and_then(serde_json::Value::as_u64)
        .and_then(|n| u32::try_from(n).ok())
        .unwrap_or(0);
    let height = stream
        .pointer("/height")
        .and_then(serde_json::Value::as_u64)
        .and_then(|n| u32::try_from(n).ok())
        .unwrap_or(0);
    let dur = v
        .pointer("/format/duration")
        .ok_or_else(|| std::io::Error::other("ffprobe: missing duration"))?;
    let secs = match dur {
        serde_json::Value::String(s) => s
            .parse()
            .map_err(|e| std::io::Error::other(format!("duration: {e}")))?,
        serde_json::Value::Number(n) => n
            .as_f64()
            .ok_or_else(|| std::io::Error::other("duration not finite"))?,
        _ => return Err(std::io::Error::other("duration: unexpected type")),
    };
    if !secs.is_finite() || secs < 0.0 {
        return Err(std::io::Error::other("duration not finite"));
    }
    let rotation = stream
        .pointer("/tags/rotate")
        .and_then(serde_json::Value::as_str)
        .and_then(|s| s.parse().ok());
    let transfer = stream
        .pointer("/color_transfer")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let hdr = parse_hdr(transfer);
    Ok(VideoStreamInfo {
        container,
        codec,
        width,
        height,
        duration: Duration::from_secs_f64(secs),
        rotation,
        hdr,
    })
}

fn parse_container(format_name: &str, path: &Path) -> ContainerKind {
    let lower = format_name.to_ascii_lowercase();
    if lower.contains("mp4") || lower.contains("mov") {
        if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("mov"))
        {
            ContainerKind::Mov
        } else {
            ContainerKind::Mp4
        }
    } else if lower.contains("matroska") {
        ContainerKind::Mkv
    } else if lower.contains("webm") {
        ContainerKind::Webm
    } else {
        match path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("mp4") => ContainerKind::Mp4,
            Some("mov") => ContainerKind::Mov,
            Some("mkv") => ContainerKind::Mkv,
            Some("webm") => ContainerKind::Webm,
            _ => ContainerKind::Other,
        }
    }
}

fn parse_codec(name: &str) -> VideoCodec {
    match name {
        "h264" => VideoCodec::H264,
        "hevc" | "h265" => VideoCodec::Hevc,
        "av1" => VideoCodec::Av1,
        "vp9" => VideoCodec::Vp9,
        _ => VideoCodec::Other,
    }
}

fn parse_hdr(transfer: &str) -> Option<HdrKind> {
    match transfer.to_ascii_lowercase().as_str() {
        "smpte2084" => Some(HdrKind::Pq),
        "arib-std-b67" => Some(HdrKind::Hlg),
        _ => None,
    }
}

/// Writes an HLS playlist and segments into `cache_dir`.
///
/// # Errors
/// ffmpeg missing or encode failed.
pub fn transcode_to_hls(
    src: &Path,
    cache_dir: &Path,
    profile: TranscodeProfile,
    backend: VideoBackend,
) -> Result<(), std::io::Error> {
    std::fs::create_dir_all(cache_dir)?;
    let src_s = src.to_string_lossy();
    let playlist = playlist_path(cache_dir);
    let segment_pattern = cache_dir.join("seg%03d.ts");
    let segment_s = segment_pattern.to_string_lossy();
    let playlist_s = playlist.to_string_lossy();
    let mut args: Vec<String> = vec![
        "-hide_banner".into(),
        "-y".into(),
        // Cap threads: under parallel CI, default auto-threading races the
        // sandbox RLIMIT_AS and fails the scale filter with EAGAIN.
        "-threads".into(),
        "1".into(),
        "-filter_threads".into(),
        "1".into(),
        "-i".into(),
        src_s.into(),
    ];
    args.extend(decode_args(backend));
    let vf = video_filter(profile, backend);
    args.extend(["-vf".into(), vf]);
    args.extend(encode_codec_args(backend));
    args.extend([
        "-map".into(),
        "0:v:0".into(),
        // Optional audio — video-only fixtures must not force an AAC encoder.
        "-map".into(),
        "0:a?".into(),
        "-c:a".into(),
        "aac".into(),
        "-b:a".into(),
        "128k".into(),
        "-ac".into(),
        "2".into(),
        "-f".into(),
        "hls".into(),
        "-hls_time".into(),
        "4".into(),
        "-hls_list_size".into(),
        "0".into(),
        "-hls_segment_filename".into(),
        segment_s.into(),
        playlist_s.into(),
    ]);
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let mut out = sandbox::run("ffmpeg", &arg_refs, TRANSCODE_MEM, TRANSCODE_CPU)?;
    // One retry: EAGAIN during filter setup is transient on busy runners.
    if !out.status.success() {
        let detail = String::from_utf8_lossy(&out.stderr);
        if detail.contains("Resource temporarily unavailable") {
            out = sandbox::run("ffmpeg", &arg_refs, TRANSCODE_MEM, TRANSCODE_CPU)?;
        }
    }
    if !out.status.success() {
        let detail = String::from_utf8_lossy(&out.stderr);
        return Err(std::io::Error::other(format!(
            "ffmpeg hls failed: {detail}"
        )));
    }
    touch_cache(cache_dir)
}

fn decode_args(backend: VideoBackend) -> Vec<String> {
    match backend {
        VideoBackend::Nvenc => vec!["-hwaccel".into(), "cuda".into()],
        VideoBackend::Vaapi => vec![
            "-hwaccel".into(),
            "vaapi".into(),
            "-hwaccel_device".into(),
            vaapi_device(),
        ],
        VideoBackend::Qsv => vec!["-hwaccel".into(), "qsv".into()],
        VideoBackend::Videotoolbox => vec!["-hwaccel".into(), "videotoolbox".into()],
        _ => Vec::new(),
    }
}

fn video_filter(profile: TranscodeProfile, backend: VideoBackend) -> String {
    let scale = format!(
        "scale=-2:'min({},ih)':force_original_aspect_ratio=decrease",
        TranscodeProfile::max_height()
    );
    let base = match profile {
        TranscodeProfile::Hdr720p => format!(
            "zscale=transfer=linear:npl=100,tonemap=tonemap=hable:desat=0,zscale=transfer=bt709:matrix=bt709:primaries=bt709,format=yuv420p,{scale}"
        ),
        TranscodeProfile::SaveBandwidth720p => scale,
    };
    if backend == VideoBackend::Vaapi {
        format!("{base},format=nv12,hwupload")
    } else {
        base
    }
}

fn encode_codec_args(backend: VideoBackend) -> Vec<String> {
    match backend {
        VideoBackend::Rkmpp => vec!["-c:v".into(), "h264_rkmpp".into()],
        VideoBackend::Nvenc => vec!["-c:v".into(), "h264_nvenc".into()],
        VideoBackend::V4l2m2m => vec!["-c:v".into(), "h264_v4l2m2m".into()],
        VideoBackend::Videotoolbox => vec!["-c:v".into(), "h264_videotoolbox".into()],
        VideoBackend::Vaapi => vec!["-c:v".into(), "h264_vaapi".into()],
        VideoBackend::Qsv => vec!["-c:v".into(), "h264_qsv".into()],
        VideoBackend::Amf => vec!["-c:v".into(), "h264_amf".into()],
        VideoBackend::Software => vec![
            "-c:v".into(),
            "libx264".into(),
            "-preset".into(),
            "veryfast".into(),
            "-profile:v".into(),
            "main".into(),
        ],
    }
}

fn vaapi_device() -> String {
    if Path::new("/dev/dri/renderD128").exists() {
        "/dev/dri/renderD128".into()
    } else {
        "/dev/dri/renderD129".into()
    }
}

fn hex32(hash: &[u8; 32]) -> String {
    let mut hex = String::with_capacity(64);
    for b in hash {
        use std::fmt::Write as _;
        let _ = write!(hex, "{b:02x}");
    }
    hex
}

/// Poster and animated preview paths beside the other derivatives.
#[must_use]
pub fn video_poster_path(data_dir: &Path, hash: &[u8; 32]) -> PathBuf {
    let hex = hex32(hash);
    data_dir
        .join("derivatives")
        .join(&hex[0..2])
        .join(&hex[2..4])
        .join(format!("{hex}-video-poster.jpg"))
}

#[must_use]
pub fn video_preview_anim_path(data_dir: &Path, hash: &[u8; 32]) -> PathBuf {
    let hex = hex32(hash);
    data_dir
        .join("derivatives")
        .join(&hex[0..2])
        .join(&hex[2..4])
        .join(format!("{hex}-video-preview.webp"))
}

/// Ensures poster exists, delegating to [`video::extract_poster`].
///
/// # Errors
/// ffmpeg or I/O.
pub fn ensure_poster(src: &Path, dest: &Path) -> Result<(), std::io::Error> {
    if dest.is_file() {
        return Ok(());
    }
    video::extract_poster(src, dest)
}

/// Three-second animated WebP hover preview at 10% of duration.
///
/// # Errors
/// ffmpeg or I/O.
pub fn extract_animated_preview(src: &Path, dest: &Path) -> Result<(), std::io::Error> {
    if dest.is_file() {
        return Ok(());
    }
    let info = video::probe(src)?;
    let at = info.duration.mul_f32(0.1);
    let ss = format!("{:.3}", at.as_secs_f64());
    let src_s = src.to_string_lossy();
    let dest_s = dest.to_string_lossy();
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let out = sandbox::run(
        "ffmpeg",
        &[
            "-y",
            "-ss",
            &ss,
            "-i",
            src_s.as_ref(),
            "-t",
            "3",
            "-an",
            "-vf",
            "fps=10,scale=320:-2",
            "-c:v",
            "libwebp",
            "-lossless",
            "0",
            "-q:v",
            "75",
            "-loop",
            "0",
            dest_s.as_ref(),
        ],
        TRANSCODE_MEM,
        60,
    )?;
    if !out.status.success() {
        return Err(std::io::Error::other("ffmpeg animated preview failed"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn parse_hdr_detects_pq_and_hlg() {
        assert_eq!(parse_hdr("smpte2084"), Some(HdrKind::Pq));
        assert_eq!(parse_hdr("arib-std-b67"), Some(HdrKind::Hlg));
        assert!(parse_hdr("bt709").is_none());
    }
}
