#![allow(clippy::unwrap_used, clippy::expect_used)]

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

/// Fase 7 Task 1: il probe non misura solo il video — `extra.ai` porta i
/// fatti host che decidono se l'analisi può girare (RAM libera, core).
#[test]
fn probe_extra_includes_measured_ai_host_facts() {
    let caps = keeppix_media::probe();
    let ai = caps
        .extra
        .get("ai")
        .expect("extra.ai must exist after Fase 7 Task 1");
    let free_ram = ai
        .get("free_ram_bytes")
        .and_then(serde_json::Value::as_u64)
        .expect("free_ram_bytes");
    let cores = ai
        .get("cpu_cores")
        .and_then(serde_json::Value::as_u64)
        .expect("cpu_cores");
    assert!(
        free_ram > 0,
        "free_ram_bytes must be a positive measurement"
    );
    assert!(cores >= 1, "cpu_cores must be at least 1");
    assert!(
        ai.get("inference_status")
            .and_then(serde_json::Value::as_str)
            == Some("pending_runtime"),
        "without Task 2's runtime/model, inference_ms stays pending_runtime: {ai}"
    );
    assert!(
        ai.get("inference_ms")
            .is_some_and(serde_json::Value::is_null)
    );
}
