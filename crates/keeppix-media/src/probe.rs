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
/// Il campo `extra` porta le estensioni: Fase 7 scrive `extra.ai` con i fatti
/// host (RAM, core) e, da Task 2, i ms di una inferenza reale sul modello
/// locale (o `model_missing` / `failed` se i pesi o il runtime non bastano).
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
///
/// Aggiunge sempre `extra.ai` (Fase 7): RAM libera, core, e una prova di
/// inferenza sul MobileCLIP2-S2 locale quando i pesi sono presenti.
#[must_use]
pub fn probe() -> Capabilities {
    let mut caps = if crate::video::ffprobe_available() {
        let mut found = None;
        for backend in candidate_backends() {
            if backend == VideoBackend::Software {
                break;
            }
            if let Some(fps) = try_encode(backend) {
                found = Some(Capabilities {
                    backend,
                    decode_fps: Some(fps),
                    extra: default_extra(),
                });
                break;
            }
        }
        found.unwrap_or_else(|| software_fallback(try_encode(VideoBackend::Software)))
    } else {
        software_fallback(None)
    };
    caps.extra = merge_ai_extra(caps.extra, measure_ai_host());
    caps
}

fn software_fallback(fps: Option<f32>) -> Capabilities {
    Capabilities {
        backend: VideoBackend::Software,
        decode_fps: fps,
        extra: default_extra(),
    }
}

/// Fatti host per l'analisi IA, più l'esito della prova di inferenza.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AiHostFacts {
    pub free_ram_bytes: u64,
    pub cpu_cores: u32,
    pub has_neon: bool,
    /// Millisecondi di una inferenza immagine sul modello locale, se riuscita.
    pub inference_ms: Option<f64>,
    pub inference_status: String,
    /// Runtime che ha prodotto la misura (`tract`), se la prova è partita.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inference_runtime: Option<String>,
}

fn measure_ai_host() -> AiHostFacts {
    let inference = crate::ai::measure_image_inference();
    AiHostFacts {
        free_ram_bytes: free_ram_bytes(),
        cpu_cores: std::thread::available_parallelism()
            .map(|n| u32::try_from(n.get()).unwrap_or(1))
            .unwrap_or(1),
        has_neon: cfg!(target_feature = "neon") || cfg!(target_arch = "aarch64"),
        inference_ms: inference.inference_ms,
        inference_status: inference.inference_status,
        inference_runtime: inference.runtime,
    }
}

fn merge_ai_extra(extra: serde_json::Value, ai: AiHostFacts) -> serde_json::Value {
    let mut object = match extra {
        serde_json::Value::Object(map) => map,
        _ => serde_json::Map::new(),
    };
    if let Ok(value) = serde_json::to_value(ai) {
        object.insert("ai".to_owned(), value);
    }
    serde_json::Value::Object(object)
}

/// RAM effettivamente disponibile per nuovi allocatori, non la sola `MemFree`
/// (che esclude la cache reclamabile). Su Linux legge `/proc/meminfo`; altrove
/// resta 0 e il chiamante tratta lo stato come sconosciuto.
fn free_ram_bytes() -> u64 {
    #[cfg(target_os = "linux")]
    {
        let Ok(text) = std::fs::read_to_string("/proc/meminfo") else {
            return 0;
        };
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix("MemAvailable:") {
                let kb: u64 = rest
                    .split_whitespace()
                    .next()
                    .and_then(|t| t.parse().ok())
                    .unwrap_or(0);
                return kb.saturating_mul(1024);
            }
        }
        0
    }
    #[cfg(not(target_os = "linux"))]
    {
        0
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

    #[test]
    fn measure_ai_host_reports_positive_ram_on_linux() {
        let facts = measure_ai_host();
        assert!(facts.cpu_cores >= 1);
        assert!(
            matches!(
                facts.inference_status.as_str(),
                "ok" | "model_missing" | "failed"
            ),
            "unexpected status {}",
            facts.inference_status
        );
        if facts.inference_status == "ok" {
            assert!(
                facts
                    .inference_ms
                    .is_some_and(|ms| ms.is_finite() && ms > 0.0)
            );
        } else {
            assert!(facts.inference_ms.is_none());
        }
        #[cfg(target_os = "linux")]
        assert!(facts.free_ram_bytes > 0);
    }
}
