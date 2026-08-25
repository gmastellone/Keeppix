//! Misura RSS del processo (Linux `/proc/self/status`). Generico — non legato
//! a un modello AI specifico: usato dai bench e dai job di embedding per
//! verificare il tetto duro di RAM durante l'inferenza (Task 6, Fase 7).
//! Spostato qui da `clip.rs` (rimosso con `MobileCLIP2`) perché resta
//! necessario anche al solo `openclip_xlmr.rs`.

use std::time::Instant;

/// RSS del processo corrente in byte (`None` fuori da Linux/`/proc`).
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

/// Picco RSS (byte) osservato durante `f`, campionato all'inizio, a metà e alla fine
/// se `f` espone un hook; qui misuriamo prima/dopo e il massimo visto.
pub fn measure_rss_peak_during<T>(mut f: impl FnMut() -> T) -> (T, Option<u64>) {
    let before = current_rss_bytes();
    let started = Instant::now();
    let out = f();
    let after = current_rss_bytes();
    let _ = started;
    let peak = [before, after].into_iter().flatten().max();
    (out, peak)
}
