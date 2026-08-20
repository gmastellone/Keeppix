use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::sandbox;

/// Accelerazione video misurata dal probe hardware (Fase 6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VideoBackend {
    Rkmpp,
    Nvenc,
    V4l2m2m,
    Videotoolbox,
    Vaapi,
    Qsv,
    Amf,
    Software,
}

/// Esito del probe hardware di decode/transcodifica.
///
/// Il campo `extra` resta disponibile per estensioni future (es. inferenza AI in
/// Fase 7) senza cambiare la forma di `backend`/`decode_fps`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Capabilities {
    pub backend: VideoBackend,
    pub decode_fps: Option<f32>,
    #[serde(default = "default_extra")]
    pub extra: serde_json::Value,
}

fn default_extra() -> serde_json::Value {
    serde_json::json!({})
}

/// Virtual address ceiling for sandboxed ffmpeg during the hardware probe.
const PROBE_MEM: u64 = 1024 * 1024 * 1024;
/// Per-backend CPU budget — the whole probe must finish in ~4 seconds.
const PROBE_CPU_SECS: u64 = 3;

/// Misura l'accelerazione video disponibile provando un encode di 2 secondi con
/// ogni backend candidato, in ordine di preferenza per il `SoC` rilevato.
#[must_use]
pub fn probe() -> Capabilities {
    if !crate::video::ffprobe_available() {
        return software_fallback(None);
    }

    for backend in candidate_backends() {
        if backend == VideoBackend::Software {
            break;
        }
        if let Some(fps) = try_encode(backend) {
            return Capabilities {
                backend,
                decode_fps: Some(fps),
                extra: default_extra(),
            };
        }
    }

    software_fallback(try_encode(VideoBackend::Software))
}

fn software_fallback(fps: Option<f32>) -> Capabilities {
    Capabilities {
        backend: VideoBackend::Software,
        decode_fps: fps,
        extra: default_extra(),
    }
}

fn candidate_backends() -> Vec<VideoBackend> {
    let mut order = Vec::new();
    let mut push = |b: VideoBackend| {
        if !order.contains(&b) {
            order.push(b);
        }
    };

    if is_rockchip() {
        push(VideoBackend::Rkmpp);
    }
    if has_nvidia() {
        push(VideoBackend::Nvenc);
    }
    if has_v4l2_m2m() {
        push(VideoBackend::V4l2m2m);
    }
    #[cfg(target_os = "macos")]
    push(VideoBackend::Videotoolbox);
    if has_vaapi() {
        push(VideoBackend::Vaapi);
    }
    if has_qsv() {
        push(VideoBackend::Qsv);
    }
    if has_amf() {
        push(VideoBackend::Amf);
    }

    for backend in [
        VideoBackend::Rkmpp,
        VideoBackend::Nvenc,
        VideoBackend::V4l2m2m,
        VideoBackend::Videotoolbox,
        VideoBackend::Vaapi,
        VideoBackend::Qsv,
        VideoBackend::Amf,
    ] {
        push(backend);
    }
    order.push(VideoBackend::Software);
    order
}

fn is_rockchip() -> bool {
    read_first_line("/proc/device-tree/compatible")
        .is_some_and(|s| s.to_ascii_lowercase().contains("rockchip"))
        || read_first_line("/proc/cpuinfo")
            .is_some_and(|s| s.to_ascii_lowercase().contains("rockchip"))
}

fn has_nvidia() -> bool {
    Path::new("/dev/nvidia0").exists()
        || sandbox::run("nvidia-smi", &["-L"], PROBE_MEM, 1)
            .map(|o| o.status.success())
            .unwrap_or(false)
}

fn has_v4l2_m2m() -> bool {
    Path::new("/dev/video0").exists() || Path::new("/dev/video1").exists()
}

fn has_vaapi() -> bool {
    Path::new("/dev/dri/renderD128").exists() || Path::new("/dev/dri/renderD129").exists()
}

fn has_qsv() -> bool {
    read_first_line("/proc/cpuinfo").is_some_and(|s| s.contains("GenuineIntel"))
}

fn has_amf() -> bool {
    read_first_line("/proc/cpuinfo").is_some_and(|s| s.contains("AuthenticAMD"))
}

fn read_first_line(path: &str) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| s.lines().next().map(str::to_owned))
}

fn try_encode(backend: VideoBackend) -> Option<f32> {
    let mut args: Vec<String> = vec![
        "-hide_banner".into(),
        "-loglevel".into(),
        "info".into(),
        "-f".into(),
        "lavfi".into(),
        "-i".into(),
        "testsrc=duration=2:size=320x240:rate=30".into(),
        "-frames:v".into(),
        "60".into(),
    ];
    args.extend(encoder_args(backend));
    args.extend(["-f".into(), "null".into(), "-".into()]);

    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let out = sandbox::run("ffmpeg", &arg_refs, PROBE_MEM, PROBE_CPU_SECS).ok()?;
    if !out.status.success() {
        return None;
    }
    parse_encode_fps(&out.stderr)
}

fn encoder_args(backend: VideoBackend) -> Vec<String> {
    match backend {
        VideoBackend::Rkmpp => vec!["-c:v".into(), "h264_rkmpp".into()],
        VideoBackend::Nvenc => vec!["-c:v".into(), "h264_nvenc".into()],
        VideoBackend::V4l2m2m => vec!["-c:v".into(), "h264_v4l2m2m".into()],
        VideoBackend::Videotoolbox => vec!["-c:v".into(), "h264_videotoolbox".into()],
        VideoBackend::Vaapi => vec![
            "-vaapi_device".into(),
            vaapi_device(),
            "-vf".into(),
            "format=nv12,hwupload".into(),
            "-c:v".into(),
            "h264_vaapi".into(),
        ],
        VideoBackend::Qsv => vec!["-c:v".into(), "h264_qsv".into()],
        VideoBackend::Amf => vec!["-c:v".into(), "h264_amf".into()],
        VideoBackend::Software => vec![
            "-c:v".into(),
            "libx264".into(),
            "-preset".into(),
            "ultrafast".into(),
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

fn parse_encode_fps(stderr: &[u8]) -> Option<f32> {
    let text = String::from_utf8_lossy(stderr);
    for line in text.lines().rev() {
        if let Some(speed) = line.split("speed=").nth(1) {
            let token = speed.split_whitespace().next()?;
            let numeric = token.trim_end_matches('x');
            if let Ok(fps) = numeric.parse::<f32>() {
                if fps.is_finite() && fps > 0.0 {
                    return Some(fps * 30.0);
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn parse_encode_fps_reads_ffmpeg_speed_line() {
        let stderr =
            b"frame=   60 fps= 25 q=-0.0 size=N/A time=00:00:02.00 bitrate=N/A speed= 2.5x\n";
        let fps = parse_encode_fps(stderr).expect("speed line");
        assert!((fps - 75.0).abs() < 0.1);
    }

    #[test]
    fn candidate_backends_always_ends_with_software() {
        let backends = candidate_backends();
        assert_eq!(*backends.last().expect("non-empty"), VideoBackend::Software);
    }
}
