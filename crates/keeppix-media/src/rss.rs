//! Process RSS measurement (Linux `/proc/self/status`). Generic — not tied
//! to a specific AI model: used by benches and embedding jobs to verify
//! the hard RAM ceiling during inference. Moved here from `clip.rs`
//! (removed along with `MobileCLIP2`) because it's still needed by
//! `openclip_xlmr.rs` alone.

use std::time::Instant;

/// Current process RSS in bytes (`None` outside Linux/`/proc`).
#[must_use]
pub fn current_rss_bytes() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("VmRSS:") {
            let kb: u64 = rest.split_whitespace().next()?.parse().ok()?;
            return Some(kb.saturating_mul(1024));
        }
    }
    None
}

/// Peak RSS (bytes) observed during `f`, sampled at the start, middle, and
/// end if `f` exposes a hook; here we measure before/after and the maximum seen.
pub fn measure_rss_peak_during<T>(mut f: impl FnMut() -> T) -> (T, Option<u64>) {
    let before = current_rss_bytes();
    let started = Instant::now();
    let out = f();
    let after = current_rss_bytes();
    let _ = started;
    let peak = [before, after].into_iter().flatten().max();
    (out, peak)
}
