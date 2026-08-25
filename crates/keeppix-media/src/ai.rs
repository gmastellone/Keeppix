//! Inferenza AI locale (Fase 7 Task 2): runtime e misura ms/foto.
//!
//! I pesi restano su disco (`models/…`); zero rete a runtime. Il probe chiama
//! [`measure_image_inference`] una volta all'avvio.

use std::path::{Path, PathBuf};
use std::time::Instant;

/// Esito della prova di inferenza sul modello visuale.
#[derive(Debug, Clone, PartialEq)]
pub struct InferenceProbe {
    pub inference_ms: Option<f64>,
    pub inference_status: String,
    pub runtime: Option<String>,
}

/// Percorsi candidati per `visual.onnx` (`OpenCLIP` XLM-R IT/EN), in ordine.
///
/// Override: `KEEPPIX_AI_VISUAL_ONNX` (file) o `KEEPPIX_MODELS_DIR` (directory
/// che contiene `openclip-xlmr-it-en/visual.onnx`).
#[must_use]
pub fn visual_model_candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(p) = std::env::var("KEEPPIX_AI_VISUAL_ONNX") {
        out.push(PathBuf::from(p));
    }
    if let Ok(dir) = std::env::var("KEEPPIX_MODELS_DIR") {
        out.push(PathBuf::from(dir).join("openclip-xlmr-it-en/visual.onnx"));
    }
    out.push(PathBuf::from("models/openclip-xlmr-it-en/visual.onnx"));
    if let Ok(manifest) = std::env::var("CARGO_MANIFEST_DIR") {
        let crate_dir = PathBuf::from(manifest);
        if let Some(workspace) = crate_dir.parent().and_then(Path::parent) {
            out.push(workspace.join("models/openclip-xlmr-it-en/visual.onnx"));
        }
    }
    out
}

fn first_existing_model() -> Option<PathBuf> {
    // Se l'override è impostato, è l'unico percorso ammesso — anche se assente
    // (serve ai test `model_missing` e al bake Docker con path esplicito).
    if let Ok(override_path) = std::env::var("KEEPPIX_AI_VISUAL_ONNX") {
        let path = PathBuf::from(override_path);
        return path.is_file().then_some(path);
    }
    visual_model_candidates().into_iter().find(|p| p.is_file())
}

/// Prova a caricare il modello e a misurare un'inferenza immagine.
///
/// Runtime scelto (Task 2): **ort**. Tract è stato provato per primo e gira
/// `MobileCLIP` in isolamento (~370 ms), ma non entra nel workspace (conflitto
/// `libm` con lo stack russh). Vedi ledger.
#[must_use]
pub fn measure_image_inference() -> InferenceProbe {
    let Some(path) = first_existing_model() else {
        return InferenceProbe {
            inference_ms: None,
            inference_status: "model_missing".to_owned(),
            runtime: None,
        };
    };
    match run_ort_probe(&path) {
        Ok(ms) => InferenceProbe {
            inference_ms: Some(ms),
            inference_status: "ok".to_owned(),
            runtime: Some("ort".to_owned()),
        },
        Err(_) => InferenceProbe {
            inference_ms: None,
            inference_status: "failed".to_owned(),
            runtime: Some("ort".to_owned()),
        },
    }
}

fn run_ort_probe(model_path: &Path) -> Result<f64, String> {
    use ort::session::Session;
    use ort::value::Tensor;

    let mut session = Session::builder()
        .map_err(|e| format!("session builder: {e}"))?
        .commit_from_file(model_path)
        .map_err(|e| format!("load: {e}"))?;

    let zeros = vec![0f32; 3 * 224 * 224];
    let warmup = Tensor::from_array(([1usize, 3, 224, 224], zeros.clone()))
        .map_err(|e| format!("tensor: {e}"))?;
    let _ = session
        .run(ort::inputs![warmup])
        .map_err(|e| format!("warmup: {e}"))?;

    let input =
        Tensor::from_array(([1usize, 3, 224, 224], zeros)).map_err(|e| format!("tensor: {e}"))?;
    let started = Instant::now();
    let outputs = session
        .run(ort::inputs![input])
        .map_err(|e| format!("infer: {e}"))?;
    let ms = started.elapsed().as_secs_f64() * 1000.0;
    let (_shape, data) = outputs[0]
        .try_extract_tensor::<f32>()
        .map_err(|e| format!("output: {e}"))?;
    if data.len() != 512 {
        return Err(format!("expected 512-d embedding, got {}", data.len()));
    }
    Ok(ms)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use serial_test::serial;

    #[test]
    #[serial]
    fn measure_reports_model_missing_when_path_points_nowhere() {
        let previous = std::env::var("KEEPPIX_AI_VISUAL_ONNX").ok();
        // SAFETY: serialised test; only this suite touches the override.
        unsafe {
            std::env::set_var(
                "KEEPPIX_AI_VISUAL_ONNX",
                "/no/such/keeppix/openclip/visual.onnx",
            );
        }
        let probe = measure_image_inference();
        match previous {
            Some(v) => unsafe { std::env::set_var("KEEPPIX_AI_VISUAL_ONNX", v) },
            None => unsafe { std::env::remove_var("KEEPPIX_AI_VISUAL_ONNX") },
        }
        assert_eq!(probe.inference_status, "model_missing");
        assert!(probe.inference_ms.is_none());
    }

    #[test]
    #[serial]
    fn measure_reports_inference_ms_when_model_is_present() {
        let model = visual_model_candidates()
            .into_iter()
            .find(|p| p.is_file())
            .or_else(|| {
                let p = PathBuf::from("/workspace/models/openclip-xlmr-it-en/visual.onnx");
                p.is_file().then_some(p)
            });
        let Some(model) = model else {
            eprintln!(
                "skipping: openclip-xlmr-it-en visual.onnx not on disk (girare .github/workflows/export-openclip-xlmr.yml)"
            );
            return;
        };
        let previous = std::env::var("KEEPPIX_AI_VISUAL_ONNX").ok();
        unsafe {
            std::env::set_var("KEEPPIX_AI_VISUAL_ONNX", &model);
        }
        let probe = measure_image_inference();
        match previous {
            Some(v) => unsafe { std::env::set_var("KEEPPIX_AI_VISUAL_ONNX", v) },
            None => unsafe { std::env::remove_var("KEEPPIX_AI_VISUAL_ONNX") },
        }
        assert_eq!(
            probe.inference_status, "ok",
            "model present must yield ok, got {probe:?}"
        );
        assert!(
            probe
                .inference_ms
                .is_some_and(|ms| ms.is_finite() && ms > 0.0),
            "inference_ms must be a positive measurement: {probe:?}"
        );
        assert_eq!(probe.runtime.as_deref(), Some("ort"));
    }
}
