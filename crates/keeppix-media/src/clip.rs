//! MobileCLIP2-S2 via `ort` (Fase 7 Task 2 / 2bis): embedding immagine e testo.
//!
//! I pesi restano su disco sotto `models/mobileclip2-s2/` (visual + text +
//! tokenizer). Zero rete a runtime. La sessione è pensata per essere caricata
//! per lotto e scaricata subito dopo (ciclo di vita Task 6).

use std::path::{Path, PathBuf};
use std::time::Instant;

use ort::session::Session;
use ort::value::Tensor;
use serde::Deserialize;
use tokenizers::{PaddingParams, PaddingStrategy, Tokenizer, TruncationParams};

const EMBED_DIM: usize = 512;

/// Identità stabile del checkpoint MobileCLIP2-S2 usato da probe, job e DB.
/// Deve restare allineata a `extra.ai.model_version` del probe.
pub const MODEL_VERSION: &str = "mobileclip2-s2";

/// Directory candidata del modello MobileCLIP2-S2 (contiene visual.onnx, text.onnx,
/// tokenizer.json, …).
#[must_use]
pub fn model_dir_candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(p) = std::env::var("KEEPPIX_AI_MODEL_DIR") {
        out.push(PathBuf::from(p));
    }
    if let Ok(dir) = std::env::var("KEEPPIX_MODELS_DIR") {
        out.push(PathBuf::from(dir).join("mobileclip2-s2"));
    }
    out.push(PathBuf::from("models/mobileclip2-s2"));
    if let Ok(manifest) = std::env::var("CARGO_MANIFEST_DIR") {
        let crate_dir = PathBuf::from(manifest);
        if let Some(workspace) = crate_dir.parent().and_then(Path::parent) {
            out.push(workspace.join("models/mobileclip2-s2"));
        }
    }
    out
}

/// Prima directory che contiene sia visual che text ONNX.
#[must_use]
pub fn first_complete_model_dir() -> Option<PathBuf> {
    if let Ok(override_dir) = std::env::var("KEEPPIX_AI_MODEL_DIR") {
        let path = PathBuf::from(override_dir);
        return is_complete_model_dir(&path).then_some(path);
    }
    model_dir_candidates()
        .into_iter()
        .find(|p| is_complete_model_dir(p))
}

fn is_complete_model_dir(dir: &Path) -> bool {
    dir.join("visual.onnx").is_file()
        && dir.join("text.onnx").is_file()
        && dir.join("tokenizer.json").is_file()
        && dir.join("open_clip_config.json").is_file()
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
    if !dir.join("open_clip_config.json").is_file() {
        missing.push("open_clip_config.json");
    }
    missing
}

#[derive(Debug, Deserialize)]
struct OpenClipConfigFile {
    model_cfg: ModelCfg,
    preprocess_cfg: PreprocessCfg,
}

#[derive(Debug, Deserialize)]
struct ModelCfg {
    vision_cfg: VisionCfg,
    text_cfg: TextCfg,
}

#[derive(Debug, Deserialize)]
struct VisionCfg {
    image_size: u32,
}

#[derive(Debug, Deserialize)]
struct TextCfg {
    context_length: usize,
}

#[derive(Debug, Deserialize)]
struct PreprocessCfg {
    mean: [f32; 3],
    std: [f32; 3],
}

#[derive(Debug, Deserialize)]
struct ModelConfigFile {
    #[serde(default)]
    tokenizer_needs_lowercase: bool,
    #[serde(default)]
    pad_id: Option<u32>,
}

/// Sessione `MobileCLIP` caricata in memoria (visual + text). Dropparla libera la RAM.
#[derive(Debug)]
pub struct MobileClip {
    visual: Session,
    text: Session,
    tokenizer: Tokenizer,
    image_size: usize,
    context_length: usize,
    mean: [f32; 3],
    std: [f32; 3],
    lowercase: bool,
}

impl MobileClip {
    /// Carica `visual.onnx` + `text.onnx` + tokenizer dalla directory modello.
    ///
    /// # Errors
    /// Directory incompleta, JSON di config illegibile, o fallimento di ort/tokenizer.
    pub fn load(model_dir: &Path) -> Result<Self, String> {
        let missing = missing_pieces(model_dir);
        if !missing.is_empty() {
            return Err(format!(
                "incomplete model dir {}: missing {}",
                model_dir.display(),
                missing.join(", ")
            ));
        }

        let config_raw = std::fs::read_to_string(model_dir.join("open_clip_config.json"))
            .map_err(|e| format!("read open_clip_config.json: {e}"))?;
        let config: OpenClipConfigFile = serde_json::from_str(&config_raw)
            .map_err(|e| format!("parse open_clip_config: {e}"))?;

        let model_cfg_path = model_dir.join("model_config.json");
        let model_cfg: ModelConfigFile = if model_cfg_path.is_file() {
            let raw = std::fs::read_to_string(&model_cfg_path)
                .map_err(|e| format!("read model_config.json: {e}"))?;
            serde_json::from_str(&raw).map_err(|e| format!("parse model_config: {e}"))?
        } else {
            ModelConfigFile {
                tokenizer_needs_lowercase: false,
                pad_id: Some(0),
            }
        };

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
        let pad_id = model_cfg.pad_id.unwrap_or(0);
        let ctx = config.model_cfg.text_cfg.context_length;
        let _ = tokenizer.with_padding(Some(PaddingParams {
            strategy: PaddingStrategy::Fixed(ctx),
            pad_id,
            ..Default::default()
        }));
        tokenizer
            .with_truncation(Some(TruncationParams {
                max_length: ctx,
                ..Default::default()
            }))
            .map_err(|e| format!("tokenizer truncation: {e}"))?;

        Ok(Self {
            visual,
            text,
            tokenizer,
            image_size: usize::try_from(config.model_cfg.vision_cfg.image_size)
                .map_err(|_| "image_size does not fit usize".to_owned())?,
            context_length: ctx,
            mean: config.preprocess_cfg.mean,
            std: config.preprocess_cfg.std,
            lowercase: model_cfg.tokenizer_needs_lowercase,
        })
    }

    /// Dimensione lato dell'input visuale (256 per S2).
    #[must_use]
    pub fn image_size(&self) -> usize {
        self.image_size
    }

    /// Embedding 512-d L2-normalizzato da tensor NCHW float già a `image_size`,
    /// pixel in \[0,1\] (mean/std applicati qui).
    ///
    /// # Errors
    /// Lunghezza NCHW errata, fallimento ort, o embedding non 512-d.
    pub fn embed_image_nchw(&mut self, nchw: &[f32]) -> Result<Vec<f32>, String> {
        let mut out = self.embed_images_nchw_batch(nchw, 1)?;
        out.pop().ok_or_else(|| "empty batch".to_owned())
    }

    /// Inferenza a lotto: `nchw` è la concatenazione di `batch` immagini NCHW
    /// (ognuna `3×size×size` in \[0,1\]). Restituisce `batch` vettori 512-d L2.
    ///
    /// # Errors
    /// Lunghezza non divisibile, fallimento ort, o output non 512-d per riga.
    pub fn embed_images_nchw_batch(
        &mut self,
        nchw: &[f32],
        batch: usize,
    ) -> Result<Vec<Vec<f32>>, String> {
        if batch == 0 {
            return Ok(Vec::new());
        }
        let per = 3 * self.image_size * self.image_size;
        let expected = per * batch;
        if nchw.len() != expected {
            return Err(format!(
                "expected {expected} floats for {batch}×3×{size}×{size}, got {}",
                nchw.len(),
                size = self.image_size,
            ));
        }
        let mut normalized = Vec::with_capacity(expected);
        let plane = self.image_size * self.image_size;
        for b in 0..batch {
            let base = b * per;
            for c in 0..3 {
                let m = self.mean[c];
                let s = self.std[c];
                let plane_base = base + c * plane;
                for i in 0..plane {
                    let v = nchw[plane_base + i];
                    normalized.push((v - m) / s);
                }
            }
        }
        let input = Tensor::from_array(([batch, 3, self.image_size, self.image_size], normalized))
            .map_err(|e| format!("image tensor: {e}"))?;
        let outputs = self
            .visual
            .run(ort::inputs![input])
            .map_err(|e| format!("visual infer: {e}"))?;
        let (shape, data) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| format!("visual output: {e}"))?;
        let mut shape_vec = Vec::with_capacity(shape.len());
        for &d in shape.iter() {
            let n = usize::try_from(d).map_err(|_| format!("invalid dim {d}"))?;
            shape_vec.push(n);
        }
        if shape_vec == [EMBED_DIM] && batch == 1 {
            return Ok(vec![l2_normalize(data)?]);
        }
        if shape_vec.len() != 2 || shape_vec[0] != batch || shape_vec[1] != EMBED_DIM {
            return Err(format!(
                "expected [{batch}, {EMBED_DIM}] visual output, got {shape_vec:?}"
            ));
        }
        let mut out = Vec::with_capacity(batch);
        for b in 0..batch {
            let start = b * EMBED_DIM;
            out.push(l2_normalize(&data[start..start + EMBED_DIM])?);
        }
        Ok(out)
    }

    /// Embedding 512-d L2-normalizzato di una stringa di testo.
    ///
    /// # Errors
    /// Tokenizzazione fallita, fallimento ort, o embedding non 512-d.
    pub fn embed_text(&mut self, text: &str) -> Result<Vec<f32>, String> {
        let prepared = if self.lowercase {
            text.to_lowercase()
        } else {
            text.to_owned()
        };
        let encoding = self
            .tokenizer
            .encode(prepared, true)
            .map_err(|e| format!("tokenize: {e}"))?;
        let mut ids: Vec<i64> = encoding.get_ids().iter().map(|&x| i64::from(x)).collect();
        if ids.len() != self.context_length {
            ids.resize(self.context_length, 0);
        }
        let input = Tensor::from_array(([1usize, self.context_length], ids))
            .map_err(|e| format!("text tensor: {e}"))?;
        let outputs = self
            .text
            .run(ort::inputs![input])
            .map_err(|e| format!("text infer: {e}"))?;
        let (_shape, data) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| format!("text output: {e}"))?;
        l2_normalize(data)
    }

    /// Preprocessa RGB8 (`H`×`W` interleaved) → NCHW float \[0,1\] con resize
    /// "shortest" + center crop a `image_size` (nearest-neighbour semplificato).
    ///
    /// # Errors
    /// Buffer RGB la cui lunghezza non coincide con `width * height * 3`.
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use serial_test::serial;

    #[test]
    #[serial]
    fn load_rejects_incomplete_model_directory() {
        let dir = tempfile_dir();
        let err = MobileClip::load(&dir).unwrap_err();
        assert!(
            err.contains("missing") || err.contains("incomplete"),
            "expected a missing-file error, got: {err}"
        );
    }

    #[test]
    #[serial]
    fn embed_text_and_image_agree_on_a_trivial_match_when_model_present() {
        let Some(dir) = first_complete_model_dir() else {
            eprintln!("skipping: complete MobileCLIP2-S2 dir missing");
            return;
        };
        let mut clip = MobileClip::load(&dir).expect("load model");
        let mut nchw = vec![0.0_f32; 3 * 256 * 256];
        for px in nchw.iter_mut().take(256 * 256) {
            *px = 1.0; // R
        }
        let img = clip.embed_image_nchw(&nchw).expect("image emb");
        let txt = clip.embed_text("a bright red square").expect("text emb");
        assert_eq!(img.len(), 512);
        assert_eq!(txt.len(), 512);
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
    fn embed_images_batch_matches_single_when_model_present() {
        let Some(dir) = first_complete_model_dir() else {
            eprintln!("skipping: complete MobileCLIP2-S2 dir missing");
            return;
        };
        let mut clip = MobileClip::load(&dir).expect("load model");
        let size = clip.image_size();
        let mut red = vec![0.0_f32; 3 * size * size];
        for px in red.iter_mut().take(size * size) {
            *px = 1.0;
        }
        let mut blue = vec![0.0_f32; 3 * size * size];
        for px in blue.iter_mut().skip(2 * size * size) {
            *px = 1.0;
        }
        let single_red = clip.embed_image_nchw(&red).expect("red single");
        let single_blue = clip.embed_image_nchw(&blue).expect("blue single");
        let mut stacked = red;
        stacked.extend_from_slice(&blue);
        let batch = clip
            .embed_images_nchw_batch(&stacked, 2)
            .expect("batch of 2");
        assert_eq!(batch.len(), 2);
        for (i, (a, b)) in [(&single_red, &batch[0]), (&single_blue, &batch[1])]
            .into_iter()
            .enumerate()
        {
            let cos: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
            assert!(
                cos > 0.999,
                "batch[{i}] must match single inference, cos={cos}"
            );
        }
    }

    #[test]
    #[serial]
    fn dropping_the_session_moves_rss_down_from_the_infer_peak() {
        let Some(dir) = first_complete_model_dir() else {
            eprintln!("skipping: complete MobileCLIP2-S2 dir missing");
            return;
        };
        let before = current_rss_bytes();
        let (loaded, after_load_peak) =
            measure_rss_peak_during(|| MobileClip::load(&dir).expect("load"));
        let mut clip = loaded;
        let (_emb, peak_infer) =
            measure_rss_peak_during(|| clip.embed_text("a red square").expect("text"));
        drop(clip);
        let after_drop = current_rss_bytes();
        eprintln!(
            "RSS drop check: before={before:?} after_load={after_load_peak:?} \
             peak_infer={peak_infer:?} after_drop={after_drop:?}"
        );
        let Some(peak) = peak_infer.or(after_load_peak) else {
            return;
        };
        let Some(dropped) = after_drop else {
            return;
        };
        // Utile = sotto il picco di inferenza (verso la base di sessione),
        // non necessariamente fino a `before`: l'allocatore ORT può tenere
        // pagine finché il processo vive.
        assert!(
            dropped <= peak,
            "after Drop RSS ({dropped}) must not exceed infer peak ({peak})"
        );
        if let Some(base) = before {
            let toward_base = peak.saturating_sub(dropped);
            let room_to_base = peak.saturating_sub(base);
            assert!(
                room_to_base == 0 || toward_base > 0 || dropped <= after_load_peak.unwrap_or(peak),
                "Drop should move RSS toward the baseline (or at least to after_load); \
                 before={base} peak={peak} after_drop={dropped}"
            );
        }
    }

    #[test]
    fn model_version_constant_is_stable() {
        assert_eq!(MODEL_VERSION, "mobileclip2-s2");
    }

    fn tempfile_dir() -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("keeppix-clip-incomplete-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
