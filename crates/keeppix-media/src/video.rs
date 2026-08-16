use std::path::Path;
use std::time::Duration;

use crate::sandbox;

/// Virtual address ceiling for sandboxed ffprobe/ffmpeg.
///
/// Distro ffmpeg (Ubuntu 24.04+) maps dozens of shared codecs into the
/// address space before doing any work; `RLIMIT_AS` of 512 MiB is enough
/// for ffprobe but ffmpeg aborts while configuring the filter graph
/// (`Failed to inject frame … Resource temporarily unavailable`). Measured
/// floor on that host for a 64×64 poster extract is ~800 MiB; 1 GiB leaves
/// headroom for ASLR without opening the door to multi-GB frame buffers.
const MEM: u64 = 1024 * 1024 * 1024;
const CPU: u64 = 30;

#[derive(Debug, Clone, PartialEq)]
pub struct VideoInfo {
    pub duration: Duration,
    pub codec: Option<String>,
    pub rotation: Option<i32>,
}

/// # Errors
/// ffprobe assente o JSON illeggibile.
pub fn probe(path: &Path) -> Result<VideoInfo, std::io::Error> {
    let path_s = path.to_string_lossy();
    let out = sandbox::run(
        "ffprobe",
        &[
            "-v",
            "error",
            "-show_entries",
            "format=duration:stream=codec_name,tags=rotate:stream_tags=rotate",
            "-of",
            "json",
            path_s.as_ref(),
        ],
        MEM,
        CPU,
    )?;
    if !out.status.success() {
        return Err(std::io::Error::other("ffprobe failed"));
    }
    parse_ffprobe(&out.stdout)
}

fn parse_ffprobe(bytes: &[u8]) -> Result<VideoInfo, std::io::Error> {
    let v: serde_json::Value = serde_json::from_slice(bytes)
        .map_err(|e| std::io::Error::other(format!("ffprobe json: {e}")))?;
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
    let codec = v
        .pointer("/streams/0/codec_name")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned);
    let rotation = v
        .pointer("/streams/0/tags/rotate")
        .and_then(serde_json::Value::as_str)
        .and_then(|s| s.parse().ok());
    Ok(VideoInfo {
        duration: Duration::from_secs_f64(secs),
        codec,
        rotation,
    })
}

/// Poster al 10% della durata. Nessuna transcodifica dell'originale.
///
/// # Errors
/// ffmpeg assente o fallito.
pub fn extract_poster(src: &Path, dest: &Path) -> Result<(), std::io::Error> {
    let info = probe(src)?;
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
            "-frames:v",
            "1",
            dest_s.as_ref(),
        ],
        MEM,
        CPU,
    )?;
    if !out.status.success() {
        return Err(std::io::Error::other("ffmpeg poster failed"));
    }
    Ok(())
}

#[must_use]
pub fn ffprobe_available() -> bool {
    sandbox::run("ffprobe", &["-version"], MEM, CPU)
        .map(|o| o.status.success())
        .unwrap_or(false)
}
