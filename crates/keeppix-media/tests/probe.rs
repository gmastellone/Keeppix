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

/// Fase 7 Task 1+2: `extra.ai` porta i fatti host e, se i pesi sono presenti,
/// i ms di una inferenza reale (altrimenti `model_missing` esplicito).
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

    let status = ai
        .get("inference_status")
        .and_then(serde_json::Value::as_str)
        .expect("inference_status");
    assert!(
        matches!(status, "ok" | "model_missing" | "failed"),
        "Task 2 replaces pending_runtime with a concrete status, got {status}: {ai}"
    );
    assert_ne!(
        status, "pending_runtime",
        "Task 2 must leave pending_runtime behind"
    );

    if status == "ok" {
        let ms = ai
            .get("inference_ms")
            .and_then(serde_json::Value::as_f64)
            .expect("inference_ms when ok");
        assert!(ms.is_finite() && ms > 0.0, "inference_ms={ms}");
    } else {
        assert!(
            ai.get("inference_ms")
                .is_some_and(serde_json::Value::is_null),
            "without a successful run inference_ms stays null: {ai}"
        );
    }
}

/// Quando i pesi MobileCLIP2-S2 sono su disco, il probe deve misurare ms > 0.
#[test]
fn probe_records_inference_ms_when_visual_model_is_present() {
    let model = keeppix_media::ai::visual_model_candidates()
        .into_iter()
        .find(|p| p.is_file());
    let Some(_model) = model else {
        eprintln!(
            "skipping: models/mobileclip2-s2/visual.onnx missing — run scripts/download-mobileclip2-s2.sh"
        );
        return;
    };
    let caps = keeppix_media::probe();
    let ai = &caps.extra["ai"];
    assert_eq!(
        ai.get("inference_status")
            .and_then(serde_json::Value::as_str),
        Some("ok"),
        "model on disk must yield ok: {ai}"
    );
    let ms = ai
        .get("inference_ms")
        .and_then(serde_json::Value::as_f64)
        .expect("inference_ms");
    assert!(ms.is_finite() && ms > 0.0, "inference_ms={ms}");
}
