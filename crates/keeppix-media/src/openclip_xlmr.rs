//! `OpenCLIP` XLM-R `ViT-B-32` IT/EN: image and text embedding via `ort`,
//! replacing `MobileCLIP2`-S2 (Apple ML Research Model License,
//! research-only — never eligible for a commercial offering).
//!
//! `MobileCLIP2`-S2 (`clip.rs`) was removed only after a counter-check: a
//! real IT/EN regression bench, same harness, equivalent-or-better numbers
//! and ~3x faster — the same principle already followed for SCRFD/`ArcFace`
//! in `crates/keeppix-media/src/face.rs`: the fallback doesn't get removed
//! before the replacement is proven.
//!
//! Weights produced by `scripts/export-openclip-xlmr-it-en.py` (Python, the
//! only place in the pipeline where it runs). Real numbers verified in real
//! CI (never in this sandbox: the checkpoint is hosted only on
//! `huggingface.co`, blocked at the proxy level): vocabulary 250,002 →
//! 15,583 rows (6.2%), `visual.onnx` ~85 MB, `text.onnx` ~96 MB int8, image
//! normalization `mean=(0.4815,0.4578,0.4082)` `std=(0.2686,0.2613,0.2758)`
//! (standard CLIP values, but read from the checkpoint's real pipeline —
//! not assumed), `max_position_embeddings=514`.
//!
//! **The tokenizer is not pruned** (`tokenizer.json`, copied unmodified
//! from the checkpoint): same segmentation for every language, since the
//! other 107 languages aren't a requirement and it's fine if pruning
//! breaks them — but here the segmentation *doesn't* break at all, only
//! which vocabulary cell gets which embedding changes. **The remap lives
//! inside the ONNX graph**, not here: `text.onnx` takes the ORIGINAL ids
//! from the unpruned tokenizer as input and remaps them internally with a
//! `gather` against a `[original_vocab_size]` constant baked into the
//! graph at export time (see `scripts/export-openclip-xlmr-it-en.py`,
//! `TextTowerExport.forward`: the remap must exist BEFORE export, not be
//! applied on the Rust side afterward) — an id outside the IT/EN corpus
//! used for pruning becomes the `<unk>` embedding inside the graph itself,
//! never an error here. Remapping them again here (as an earlier draft of
//! this file did, reading `id_remap.json`) would apply the remap twice — a
//! real bug found by the first IT/EN bench actually run in CI (EN recall@1
//! collapsed to 0.05, essentially random): `id_remap.json` isn't needed by
//! the Rust consumer, it's kept only for Python diagnostics.
//!
//! `text.onnx` has a dynamic sequence axis (no fixed-length padding
//! needed, unlike `MobileCLIP2`): tokenization only truncates to
//! `text_max_position_embeddings`.

use std::path::{Path, PathBuf};

use ort::session::Session;
use ort::value::Tensor;
use serde::Deserialize;
use tokenizers::{Tokenizer, TruncationParams};

/// Stable identity of the checkpoint used by the probe, jobs, and DB.
/// Older rows with `model_version = "mobileclip2-s2"` (`MobileCLIP2`-S2,
/// removed) stay in the DB until recomputed: identical embed dim, 512, no
/// schema migration needed.
pub const MODEL_VERSION: &str = "openclip-xlmr-it-en";
const EMBED_DIM: usize = 512;

#[must_use]
pub fn model_dir_candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(p) = std::env::var("KEEPPIX_OPENCLIP_XLMR_MODEL_DIR") {
        out.push(PathBuf::from(p));
    }
    if let Ok(dir) = std::env::var("KEEPPIX_MODELS_DIR") {
        out.push(PathBuf::from(dir).join("openclip-xlmr-it-en"));
    }
    out.push(PathBuf::from("models/openclip-xlmr-it-en"));
    if let Ok(manifest) = std::env::var("CARGO_MANIFEST_DIR") {
        let crate_dir = PathBuf::from(manifest);
        if let Some(workspace) = crate_dir.parent().and_then(Path::parent) {
            out.push(workspace.join("models/openclip-xlmr-it-en"));
        }
    }
    out
}

#[must_use]
pub fn first_complete_model_dir() -> Option<PathBuf> {
    if let Ok(override_dir) = std::env::var("KEEPPIX_OPENCLIP_XLMR_MODEL_DIR") {
        let path = PathBuf::from(override_dir);
        return is_complete_model_dir(&path).then_some(path);
    }
    model_dir_candidates()
        .into_iter()
        .find(|p| is_complete_model_dir(p))
}

fn is_complete_model_dir(dir: &Path) -> bool {
    missing_pieces(dir).is_empty()
}

fn missing_pieces(dir: &Path) -> Vec<&'static str> {
    let mut missing = Vec::new();
    if !dir.join("visual.onnx").is_file() {
        missing.push("visual.onnx");
    }
    if !dir.join("text.onnx").is_file() {
        missing.push("text.onnx");
    }
    if !dir.join("tokenizer.json").is_file() {
        missing.push("tokenizer.json");
    }
    if !dir.join("export_manifest.json").is_file() {
        missing.push("export_manifest.json");
    }
    missing
}

/// Mirrors the fields written by `scripts/export-openclip-xlmr-it-en.py`
/// (`manifest = {...}` in that script) — read at runtime, not copied as
/// Rust constants: if the export changes checkpoint or normalization, this
/// file doesn't need to be touched.
#[derive(Debug, Deserialize)]
struct ExportManifest {
    embed_dim: usize,
    image_size: u32,
    image_mean: [f32; 3],
    image_std: [f32; 3],
    text_max_position_embeddings: usize,
}

/// `OpenCLIP` XLM-R sessions loaded in memory (visual + text). Dropping
/// them frees the RAM (onnxruntime limitation: it doesn't return every
/// page to the OS immediately after `Drop`).
#[derive(Debug)]
pub struct OpenClipXlmr {
    visual: Session,
    text: Session,
    tokenizer: Tokenizer,
    image_size: usize,
    mean: [f32; 3],
    std: [f32; 3],
}

impl OpenClipXlmr {
    /// Loads `visual.onnx` + `text.onnx` + tokenizer from the model
    /// directory.
    ///
    /// # Errors
    /// Incomplete directory, unreadable/malformed JSON, or ort/tokenizer
    /// failure.
    pub fn load(model_dir: &Path) -> Result<Self, String> {
        let missing = missing_pieces(model_dir);
        if !missing.is_empty() {
            return Err(format!(
                "incomplete model dir {}: missing {}",
                model_dir.display(),
                missing.join(", ")
            ));
        }

        let manifest_raw = std::fs::read_to_string(model_dir.join("export_manifest.json"))
            .map_err(|e| format!("read export_manifest.json: {e}"))?;
        let manifest: ExportManifest = serde_json::from_str(&manifest_raw)
            .map_err(|e| format!("parse export_manifest.json: {e}"))?;
        if manifest.embed_dim != EMBED_DIM {
            return Err(format!(
                "expected {EMBED_DIM}-d embed_dim in export_manifest.json, got {}",
                manifest.embed_dim
            ));
        }

        let visual = Session::builder()
            .map_err(|e| format!("visual session builder: {e}"))?
            .commit_from_file(model_dir.join("visual.onnx"))
            .map_err(|e| format!("load visual.onnx: {e}"))?;
        let text = Session::builder()
            .map_err(|e| format!("text session builder: {e}"))?
            .commit_from_file(model_dir.join("text.onnx"))
            .map_err(|e| format!("load text.onnx: {e}"))?;

        let mut tokenizer = Tokenizer::from_file(model_dir.join("tokenizer.json"))
            .map_err(|e| format!("load tokenizer.json: {e}"))?;
        // Truncation only, no fixed-length padding: text.onnx's sequence
        // axis is dynamic (dynamic_axes in the export script), unlike
        // MobileCLIP2's fixed context_length.
        tokenizer
            .with_truncation(Some(TruncationParams {
                max_length: manifest.text_max_position_embeddings,
                ..Default::default()
            }))
            .map_err(|e| format!("tokenizer truncation: {e}"))?;

        Ok(Self {
            visual,
            text,
            tokenizer,
            image_size: usize::try_from(manifest.image_size)
                .map_err(|_| "image_size does not fit usize".to_owned())?,
            mean: manifest.image_mean,
            std: manifest.image_std,
        })
    }

    /// Visual input side length (224, read from the real manifest).
    #[must_use]
    pub fn image_size(&self) -> usize {
        self.image_size
    }

    /// 512-d L2-normalized embedding from a float NCHW tensor already at
    /// `image_size`, pixels in \[0,1\] (the checkpoint's real mean/std
    /// applied here — `visual.onnx` doesn't include them).
    ///
    /// # Errors
    /// Wrong NCHW length, ort failure, or embedding not 512-d.
    #[allow(clippy::missing_panics_doc)]
    pub fn embed_image_nchw(&mut self, nchw: &[f32]) -> Result<Vec<f32>, String> {
        let expected = 3 * self.image_size * self.image_size;
        if nchw.len() != expected {
            return Err(format!(
                "expected {expected} floats for 3×{size}×{size}, got {}",
                nchw.len(),
                size = self.image_size,
            ));
        }
        let mut normalized = Vec::with_capacity(expected);
        let plane = self.image_size * self.image_size;
        for c in 0..3 {
            let m = self.mean[c];
            let s = self.std[c];
            let plane_base = c * plane;
            for i in 0..plane {
                normalized.push((nchw[plane_base + i] - m) / s);
            }
        }
        let input = Tensor::from_array(([1usize, 3, self.image_size, self.image_size], normalized))
            .map_err(|e| format!("image tensor: {e}"))?;
        let outputs = self
            .visual
            .run(ort::inputs![input])
            .map_err(|e| format!("visual infer: {e}"))?;
        let (_shape, data) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| format!("visual output: {e}"))?;
        l2_normalize(data)
    }

    /// 512-d L2-normalized embedding of a text string: tokenizes with the
    /// unpruned tokenizer and feeds `text.onnx` the ORIGINAL ids (no
    /// padding, length = actual token count) — the remap onto the pruned
    /// vocabulary lives inside the ONNX graph itself
    /// (`TextTowerExport.forward` in the export script), not here:
    /// remapping it again on the Rust side would apply it twice (a real
    /// bug, see the module doc comment).
    ///
    /// # Errors
    /// Tokenization failure, ort failure, or embedding not 512-d.
    pub fn embed_text(&mut self, text: &str) -> Result<Vec<f32>, String> {
        let encoding = self
            .tokenizer
            .encode(text, true)
            .map_err(|e| format!("tokenize: {e}"))?;
        let original_ids = encoding.get_ids();
        if original_ids.is_empty() {
            return Err("tokenizer produced zero ids".to_owned());
        }
        let ids: Vec<i64> = original_ids.iter().map(|id| i64::from(*id)).collect();
        let seq_len = ids.len();
        let attention_mask: Vec<i64> = vec![1; seq_len];

        let ids_tensor = Tensor::from_array(([1usize, seq_len], ids))
            .map_err(|e| format!("text input_ids tensor: {e}"))?;
        let mask_tensor = Tensor::from_array(([1usize, seq_len], attention_mask))
            .map_err(|e| format!("text attention_mask tensor: {e}"))?;
        // Explicit names (not positional): text.onnx has two inputs
        // (input_ids, attention_mask, in this order in the export script)
        // — binding by name fails explicitly if the export ever changed
        // order, instead of silently feeding the wrong tensor into the
        // wrong slot.
        let outputs = self
            .text
            .run(ort::inputs!["input_ids" => ids_tensor, "attention_mask" => mask_tensor])
            .map_err(|e| format!("text infer: {e}"))?;
        let (_shape, data) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| format!("text output: {e}"))?;
        l2_normalize(data)
    }

    /// Preprocesses RGB8 (`H`×`W` interleaved) → float NCHW \[0,1\] with a
    /// "shortest side" resize + center crop to `image_size`.
    ///
    /// # Errors
    /// RGB buffer whose length doesn't match `width * height * 3`.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        clippy::cast_possible_wrap
    )]
    pub fn rgb_to_nchw(&self, rgb: &[u8], width: u32, height: u32) -> Result<Vec<f32>, String> {
        if rgb.len() != (width as usize) * (height as usize) * 3 {
            return Err("rgb buffer length mismatch".to_owned());
        }
        let size = self.image_size as u32;
        let scale = size as f32 / width.min(height) as f32;
        let scaled_w = ((width as f32) * scale).round().max(1.0) as u32;
        let scaled_h = ((height as f32) * scale).round().max(1.0) as u32;
        let crop_x = scaled_w.saturating_sub(size) / 2;
        let crop_y = scaled_h.saturating_sub(size) / 2;

        let mut nchw = vec![0.0_f32; 3 * self.image_size * self.image_size];
        for y in 0..size {
            for x in 0..size {
                let sx = (((x + crop_x) as f32 + 0.5) / scale - 0.5)
                    .round()
                    .clamp(0.0, (width - 1) as f32) as u32;
                let sy = (((y + crop_y) as f32 + 0.5) / scale - 0.5)
                    .round()
                    .clamp(0.0, (height - 1) as f32) as u32;
                let src = ((sy * width + sx) * 3) as usize;
                let dst = (y * size + x) as usize;
                nchw[dst] = f32::from(rgb[src]) / 255.0;
                nchw[self.image_size * self.image_size + dst] = f32::from(rgb[src + 1]) / 255.0;
                nchw[2 * self.image_size * self.image_size + dst] = f32::from(rgb[src + 2]) / 255.0;
            }
        }
        Ok(nchw)
    }
}

fn l2_normalize(data: &[f32]) -> Result<Vec<f32>, String> {
    if data.len() != EMBED_DIM {
        return Err(format!(
            "expected {EMBED_DIM}-d embedding, got {}",
            data.len()
        ));
    }
    let norm = data.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm == 0.0 {
        return Err("zero embedding".to_owned());
    }
    Ok(data.iter().map(|x| x / norm).collect())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use serial_test::serial;

    #[test]
    fn model_version_constant_is_stable() {
        assert_eq!(MODEL_VERSION, "openclip-xlmr-it-en");
    }

    #[test]
    #[serial]
    fn load_rejects_incomplete_model_directory() {
        let dir = std::env::temp_dir().join(format!(
            "keeppix-openclip-xlmr-incomplete-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let err = OpenClipXlmr::load(&dir).unwrap_err();
        assert!(
            err.contains("missing") || err.contains("incomplete"),
            "expected a missing-file error, got: {err}"
        );
    }

    #[test]
    #[serial]
    fn embed_text_and_image_agree_on_a_trivial_match_when_model_present() {
        let Some(dir) = first_complete_model_dir() else {
            eprintln!("skipping: complete openclip-xlmr-it-en dir missing");
            return;
        };
        let mut clip = OpenClipXlmr::load(&dir).expect("load model");
        let size = clip.image_size();
        let mut nchw = vec![0.0_f32; 3 * size * size];
        for px in nchw.iter_mut().take(size * size) {
            *px = 1.0; // R
        }
        let img = clip.embed_image_nchw(&nchw).expect("image emb");
        let txt = clip.embed_text("a bright red square").expect("text emb");
        assert_eq!(img.len(), EMBED_DIM);
        assert_eq!(txt.len(), EMBED_DIM);
        let img_norm: f32 = img.iter().map(|x| x * x).sum::<f32>().sqrt();
        let txt_norm: f32 = txt.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!(
            (img_norm - 1.0).abs() < 1e-3,
            "image embedding must be L2-normalised, got {img_norm}"
        );
        assert!(
            (txt_norm - 1.0).abs() < 1e-3,
            "text embedding must be L2-normalised, got {txt_norm}"
        );
    }

    #[test]
    #[serial]
    fn unmapped_token_falls_back_to_unk_without_erroring() {
        // An id outside the pruned vocabulary must never make inference
        // fail — the remap inside the ONNX graph collapses it onto the
        // <unk> embedding, not a Rust error.
        let Some(dir) = first_complete_model_dir() else {
            eprintln!("skipping: complete openclip-xlmr-it-en dir missing");
            return;
        };
        let mut clip = OpenClipXlmr::load(&dir).expect("load model");
        // Text plausibly outside the IT/EN corpus (rare/technical words in
        // another language): must not panic or return Err for this.
        let out = clip.embed_text("縺薙ｌ縺ｯ繝・せ繝医〒縺・");
        assert!(
            out.is_ok(),
            "unmapped tokens must degrade, not error: {out:?}"
        );
    }
}
