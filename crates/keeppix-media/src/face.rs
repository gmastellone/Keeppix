//! `YuNet` (detection) + `SFace` (embedding) via `ort`. SCRFD/`ArcFace`
//! (`InsightFace`) were never used in production: research-only weights,
//! never downloaded, never run — replaced before any release. `YuNet` is
//! MIT, `SFace` is Apache 2.0: both free even for commercial use.
//!
//! Same stack as `clip.rs`: local ONNX weights under `models/`, zero
//! network at runtime, session alive only for the analysis window. **No
//! `face_id` crate**: it would pin a second `ort` version (0.4.4 wants
//! `2.0.0-rc.13`) instead of reusing the one already in this crate — the
//! exact opposite of the single-inference-stack goal.
//!
//! **Declared limitation of this implementation**: without network access
//! to `cdn.pyke.io` (to compile `ort-sys`) from this development sandbox,
//! the decode below was never run against real ONNX weights inside `cargo
//! test` — only against synthetic tensors in the tests. **Unlike the
//! earlier SCRFD implementation**, though, the two real `.onnx` files were
//! downloaded and inspected directly (`onnx.load`, Python graph) while
//! writing this module: the input/output shapes below (names, dimensions,
//! `YuNet`'s fixed 640×640 input, `SFace`'s 128-d `fc1` output, the only
//! input actually required, `data` — the other ~144 names in the `SFace`
//! graph all carry an initializer, frozen `BatchNorm`/`PReLU` parameters
//! from the export, and must not be supplied to `Session::run`) are
//! **verified against the real file**, not inferred. The decoding formulas
//! (stride, `score = sqrt(cls·obj)`, YOLO-style boxes), however, come from
//! reading `modules/objdetect/src/face_detect.cpp` in `OpenCV` directly —
//! output names read by **name** (not by position), so a different order
//! fails explicitly instead of silently producing wrong boxes. Still to be
//! verified in real CI: that inference converges on a real photo, and that
//! the similarity-threshold calibration holds up.

use std::path::{Path, PathBuf};

use keeppix_domain::FaceBBox;
use ort::session::Session;
use ort::value::Tensor;

use crate::align::{self, SFACE_REFERENCE_112};

/// Stable identity of the checkpoint used by the probe, jobs, and DB.
pub const MODEL_VERSION: &str = "yunet+sface";

const STRIDES: [u32; 3] = [8, 16, 32];
/// `YuNet` detector input side: **not** an implementation choice the way it
/// was for SCRFD — it's the fixed dimension declared by the ONNX graph of
/// the pinned `2023mar_int8` checkpoint (`input: [1, 3, 640, 640]`,
/// verified by loading the real file). `ort` rejects a tensor of a
/// different size: not a tunable parameter.
const DETECTOR_INPUT_SIZE: u32 = 640;
/// Official thresholds from `OpenCV` Zoo's Python wrapper for `YuNet`
/// (`models/face_detection_yunet/yunet.py`: `confThreshold=0.6`,
/// `nmsThreshold=0.3`) — not the SCRFD values from the previous checkpoint.
const SCORE_THRESHOLD: f32 = 0.6;
const NMS_IOU_THRESHOLD: f32 = 0.3;
/// `SFace` embedding dimension: 128, **not** 512 like `ArcFace` (verified
/// both against the `fc1` output of the real ONNX graph and against
/// `OpenCV`'s official comment in `samples/dnn/js_face_recognition.html`,
/// "Get 128 floating points feature vector"). The schema migration that
/// follows from this number is
/// `crates/keeppix-db/migrations/0050_faces_embedding_dim_128.sql`.
const SFACE_EMBED_DIM: usize = 128;

#[must_use]
pub fn model_dir_candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(p) = std::env::var("KEEPPIX_FACE_MODEL_DIR") {
        out.push(PathBuf::from(p));
    }
    if let Ok(dir) = std::env::var("KEEPPIX_MODELS_DIR") {
        out.push(PathBuf::from(dir).join("yunet-sface"));
    }
    out.push(PathBuf::from("models/yunet-sface"));
    if let Ok(manifest) = std::env::var("CARGO_MANIFEST_DIR") {
        let crate_dir = PathBuf::from(manifest);
        if let Some(workspace) = crate_dir.parent().and_then(Path::parent) {
            out.push(workspace.join("models/yunet-sface"));
        }
    }
    out
}

fn is_complete_model_dir(dir: &Path) -> bool {
    dir.join("detect.onnx").is_file() && dir.join("embed.onnx").is_file()
}

/// Inference probe for the detector, same contract as
/// [`crate::ai::measure_image_inference`] for CLIP: one warm-up pass plus a
/// measured one, on a black input. If the weights are missing, returns
/// `inference_status = "model_missing"` instead of making up a number.
#[must_use]
pub fn measure_face_detection_inference() -> crate::ai::InferenceProbe {
    use crate::ai::InferenceProbe;
    let Some(dir) = first_complete_model_dir() else {
        return InferenceProbe {
            inference_ms: None,
            inference_status: "model_missing".to_owned(),
            runtime: None,
        };
    };
    match run_detector_probe(&dir) {
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

fn run_detector_probe(model_dir: &Path) -> Result<f64, String> {
    let mut models = FaceModels::load(model_dir)?;
    let black = vec![0_u8; (DETECTOR_INPUT_SIZE * DETECTOR_INPUT_SIZE * 3) as usize];
    let _ = models.detect(&black, DETECTOR_INPUT_SIZE, DETECTOR_INPUT_SIZE)?; // warm-up
    let started = std::time::Instant::now();
    let _ = models.detect(&black, DETECTOR_INPUT_SIZE, DETECTOR_INPUT_SIZE)?;
    Ok(started.elapsed().as_secs_f64() * 1000.0)
}

#[must_use]
pub fn first_complete_model_dir() -> Option<PathBuf> {
    if let Ok(override_dir) = std::env::var("KEEPPIX_FACE_MODEL_DIR") {
        let path = PathBuf::from(override_dir);
        return is_complete_model_dir(&path).then_some(path);
    }
    model_dir_candidates()
        .into_iter()
        .find(|p| is_complete_model_dir(p))
}

/// A detected face, in coordinates **relative** (0..1) to the image passed
/// to [`FaceModels::detect`] — survives derivatives of a different size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DetectedFace {
    pub bbox: FaceBBox,
    /// Right eye, left eye, nose, right mouth corner, left mouth corner —
    /// `YuNet`'s native order (`demo.py`, `landmark_color`: indices 0..4
    /// labeled exactly this way), passed **without permutation** to
    /// [`align::SFACE_REFERENCE_112`]: verified against
    /// `objdetect/src/face_recognize.cpp`, `FaceRecognizerSF::alignCrop` —
    /// the same official function that bridges these two models reads the
    /// detector's 5 points in strict order
    /// (`src_point[row][col] = face_mat.at<float>(0, row*2+col+4)`, no
    /// reordering) into this exact same reference array.
    pub landmarks: [(f32, f32); 5],
    pub score: f32,
}

/// `YuNet` + `SFace` sessions loaded in memory. Dropping them frees the
/// RAM (same pattern as `MobileClip`, which has the same limitation:
/// onnxruntime doesn't return every page to the OS immediately after
/// `Drop`).
#[derive(Debug)]
pub struct FaceModels {
    detector: Session,
    embedder: Session,
}

impl FaceModels {
    /// Loads `detect.onnx` (`YuNet`) + `embed.onnx` (`SFace`) from the
    /// model directory.
    ///
    /// # Errors
    /// Incomplete directory, or ort failing to load either graph.
    pub fn load(model_dir: &Path) -> Result<Self, String> {
        if !is_complete_model_dir(model_dir) {
            return Err(format!(
                "incomplete face model dir {}: missing detect.onnx and/or embed.onnx",
                model_dir.display()
            ));
        }
        let detector = Session::builder()
            .map_err(|e| format!("detector session builder: {e}"))?
            .commit_from_file(model_dir.join("detect.onnx"))
            .map_err(|e| format!("load detect.onnx: {e}"))?;
        let embedder = Session::builder()
            .map_err(|e| format!("embedder session builder: {e}"))?
            .commit_from_file(model_dir.join("embed.onnx"))
            .map_err(|e| format!("load embed.onnx: {e}"))?;
        Ok(Self { detector, embedder })
    }

    /// Detects faces in an interleaved RGB8 image (`width`×`height`).
    /// Applies a letterbox (resize keeping aspect ratio, fills the rest
    /// with black) to [`DETECTOR_INPUT_SIZE`] — the only size the `YuNet`
    /// graph accepts — then decodes the three heads (stride 8/16/32, one
    /// detection per cell, no multiple anchors unlike SCRFD) and applies
    /// non-maximum suppression.
    ///
    /// # Errors
    /// RGB buffer inconsistent with `width`×`height`, or ort failure.
    #[allow(clippy::cast_precision_loss)]
    pub fn detect(
        &mut self,
        rgb: &[u8],
        width: u32,
        height: u32,
    ) -> Result<Vec<DetectedFace>, String> {
        if rgb.len() != (width as usize) * (height as usize) * 3 {
            return Err("rgb buffer length mismatch".to_owned());
        }
        let (nchw, scale, pad_x, pad_y) =
            letterbox_to_nchw_bgr(rgb, width, height, DETECTOR_INPUT_SIZE);
        let input = Tensor::from_array((
            [
                1usize,
                3,
                DETECTOR_INPUT_SIZE as usize,
                DETECTOR_INPUT_SIZE as usize,
            ],
            nchw,
        ))
        .map_err(|e| format!("detector input tensor: {e}"))?;
        let outputs = self
            .detector
            .run(ort::inputs![input])
            .map_err(|e| format!("detector infer: {e}"))?;

        let mut candidates = Vec::new();
        for &stride in &STRIDES {
            let cls = named_output_f32(&outputs, &format!("cls_{stride}"))?;
            let obj = named_output_f32(&outputs, &format!("obj_{stride}"))?;
            let bboxes = named_output_f32(&outputs, &format!("bbox_{stride}"))?;
            let kpss = named_output_f32(&outputs, &format!("kps_{stride}"))?;
            let feat = DETECTOR_INPUT_SIZE / stride;
            decode_stride(
                feat,
                feat,
                stride,
                &cls,
                &obj,
                &bboxes,
                &kpss,
                &mut candidates,
            )?;
        }

        let kept = nms(&candidates, NMS_IOU_THRESHOLD);
        let mut out = Vec::with_capacity(kept.len());
        for idx in kept {
            let c = &candidates[idx];
            // From letterbox space to original image space, then to
            // relative coordinates — order matters: first remove the
            // padding, then divide by the scale, finally normalize.
            let unletterbox = |x: f32, y: f32| -> (f32, f32) {
                (
                    ((x - pad_x as f32) / scale) / width as f32,
                    ((y - pad_y as f32) / scale) / height as f32,
                )
            };
            let (x1, y1) = unletterbox(c.x1, c.y1);
            let (x2, y2) = unletterbox(c.x2, c.y2);
            let mut landmarks = [(0.0_f32, 0.0_f32); 5];
            for (i, lm) in landmarks.iter_mut().enumerate() {
                *lm = unletterbox(c.kps[i].0, c.kps[i].1);
            }
            out.push(DetectedFace {
                bbox: FaceBBox {
                    x: x1.clamp(0.0, 1.0),
                    y: y1.clamp(0.0, 1.0),
                    w: (x2 - x1).clamp(0.0, 1.0),
                    h: (y2 - y1).clamp(0.0, 1.0),
                },
                landmarks,
                score: c.score,
            });
        }
        Ok(out)
    }

    /// Aligns (Umeyama/similarity) and crops the face from the preview to
    /// 112×112, then computes its 128-d `SFace` embedding, **not**
    /// L2-normalized by the model: normalization is the caller's
    /// responsibility, so it can also be applied to an averaged centroid
    /// without recomputing the embedding.
    ///
    /// `landmarks_rel` are the 5 relative coordinates (0..1) **relative to
    /// `preview_w`×`preview_h`**, not to the thumbnail used by
    /// [`Self::detect`] — detection runs on the thumbnail, embedding on the
    /// preview.
    ///
    /// # Errors
    /// Inconsistent RGB buffer, or ort failure.
    #[allow(clippy::cast_precision_loss)]
    pub fn embed_face(
        &mut self,
        preview_rgb: &[u8],
        preview_w: u32,
        preview_h: u32,
        landmarks_rel: [(f32, f32); 5],
    ) -> Result<Vec<f32>, String> {
        if preview_rgb.len() != (preview_w as usize) * (preview_h as usize) * 3 {
            return Err("rgb buffer length mismatch".to_owned());
        }
        let src_points: Vec<(f32, f32)> = landmarks_rel
            .iter()
            .map(|&(x, y)| (x * preview_w as f32, y * preview_h as f32))
            .collect();
        let transform = align::similarity_transform_from_points(&src_points, &SFACE_REFERENCE_112);
        let aligned = align::warp_aligned_face(preview_rgb, preview_w, preview_h, &transform)?;
        self.embed_aligned(&aligned)
    }

    /// `SFace` embedding from an already-aligned 112×112 RGB8 crop.
    ///
    /// # Errors
    /// Wrong size, or ort failure.
    pub fn embed_aligned(&mut self, aligned_rgb_112: &[u8]) -> Result<Vec<f32>, String> {
        let size = align::ALIGNED_FACE_SIZE as usize;
        if aligned_rgb_112.len() != size * size * 3 {
            return Err(format!(
                "expected {size}x{size}x3 aligned crop, got {} bytes",
                aligned_rgb_112.len()
            ));
        }
        // `SFace` (dnn::blobFromImage(_aligned_img, 1, Size(112,112),
        // Scalar(0,0,0), true, false), read from the face_recognize.cpp C++
        // source): scalefactor 1 and mean 0 → raw 0..255 pixels, NO
        // normalization (unlike ArcFace, which divided by
        // (pixel-127.5)/128). swapRB=true in the original source converts
        // OpenCV's BGR-native source to RGB for the network — our buffer
        // is already RGB, so it's fine as-is, no channel swap needed
        // (unlike the YuNet detector below, which does swap them — see the
        // comment on `letterbox_to_nchw_bgr`).
        let mut nchw = vec![0.0_f32; 3 * size * size];
        let plane = size * size;
        for i in 0..plane {
            for c in 0..3 {
                nchw[c * plane + i] = f32::from(aligned_rgb_112[i * 3 + c]);
            }
        }
        let input = Tensor::from_array(([1usize, 3, size, size], nchw))
            .map_err(|e| format!("embedder input tensor: {e}"))?;
        // The `SFace` graph declares ~144 additional inputs (BatchNorm/
        // PReLU parameters frozen at export) besides `data`: they all carry
        // an initializer in the ONNX graph (verified by loading the real
        // file), so onnxruntime uses them as defaults without needing them
        // supplied here — standard ONNX behavior for inputs with an
        // initializer, not an omission.
        let outputs = self
            .embedder
            .run(ort::inputs![input])
            .map_err(|e| format!("embedder infer: {e}"))?;
        let (_shape, data) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| format!("embedder output: {e}"))?;
        if data.len() != SFACE_EMBED_DIM {
            return Err(format!(
                "expected {SFACE_EMBED_DIM}-d embedding, got {}",
                data.len()
            ));
        }
        Ok(data.to_vec())
    }
}

fn named_output_f32(
    outputs: &ort::session::SessionOutputs<'_>,
    name: &str,
) -> Result<Vec<f32>, String> {
    let value = outputs
        .get(name)
        .ok_or_else(|| format!("detector output `{name}` not found"))?;
    let (_shape, data) = value
        .try_extract_tensor::<f32>()
        .map_err(|e| format!("detector output `{name}`: {e}"))?;
    Ok(data.to_vec())
}

/// Letterbox: scale keeping the aspect ratio so the longer side becomes
/// `target`, then center the result on a `target`×`target` canvas filled
/// with black. Returns the NCHW tensor with **raw** 0..255 pixels
/// (`YuNet`'s `dnn::blobFromImage` doesn't normalize: scalefactor 1, mean
/// 0, read from `face_detect.cpp`) and the parameters to invert the
/// transform on the detected coordinates.
///
/// **R↔B channels swapped** (the function name: it produces BGR, not RGB):
/// the `OpenCV` source calls `blobFromImage` with `swapRB=false` on a
/// BGR-native image (from `cv::imread`) — i.e. the network receives BGR
/// with no swap at all. Our source buffer is RGB (this crate's
/// convention): to feed the network the same channel order it was trained
/// on, we swap here, rather than leaving the RGB order unchanged as the
/// earlier SCRFD letterbox did (that one fed raw RGB — a reasonable choice
/// for the `InsightFace` family, whose official demos explicitly convert
/// BGR→RGB before inference; for `YuNet` the source evidence says the
/// opposite). Not verified empirically in this sandbox (no real inference
/// can be run here) — to be confirmed the first time CI runs the
/// end-to-end test with real weights.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
fn letterbox_to_nchw_bgr(
    rgb: &[u8],
    width: u32,
    height: u32,
    target: u32,
) -> (Vec<f32>, f32, u32, u32) {
    let scale = (target as f32 / width.max(1) as f32).min(target as f32 / height.max(1) as f32);
    let scaled_w = ((width as f32) * scale).round().max(1.0) as u32;
    let scaled_h = ((height as f32) * scale).round().max(1.0) as u32;
    let pad_x = (target - scaled_w.min(target)) / 2;
    let pad_y = (target - scaled_h.min(target)) / 2;

    let plane = (target * target) as usize;
    let mut nchw = vec![0.0_f32; 3 * plane]; // black: raw pixel 0

    for oy in 0..scaled_h.min(target) {
        for ox in 0..scaled_w.min(target) {
            let sx = ((ox as f32 + 0.5) / scale).clamp(0.0, (width - 1) as f32) as u32;
            let sy = ((oy as f32 + 0.5) / scale).clamp(0.0, (height - 1) as f32) as u32;
            let src = ((sy * width + sx) * 3) as usize;
            let dst = ((oy + pad_y) * target + (ox + pad_x)) as usize;
            // src+0=R, src+1=G, src+2=B (RGB buffer) → network channel
            // c=0 (B), c=1 (G), c=2 (R): R↔B swap, see doc comment above.
            let bgr = [rgb[src + 2], rgb[src + 1], rgb[src]];
            for (c, &v) in bgr.iter().enumerate() {
                nchw[c * plane + dst] = f32::from(v);
            }
        }
    }
    (nchw, scale, pad_x, pad_y)
}

#[derive(Debug, Clone, Copy)]
struct Candidate {
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
    kps: [(f32, f32); 5],
    score: f32,
}

/// Decodes a `YuNet` head at one stride: one detection per cell
/// (anchor-free, unlike SCRFD's 2 anchors per cell), formulas read from
/// `OpenCV`'s `face_detect.cpp`:
/// `cx=(c+bbox[0])·stride`, `cy=(r+bbox[1])·stride`,
/// `w=exp(bbox[2])·stride`, `h=exp(bbox[3])·stride`,
/// `score=√(cls·obj)`, `kp_n=((kps[2n]+c)·stride, (kps[2n+1]+r)·stride)`.
/// `c`/`r` are the (column, row) indices of the cell in the
/// `feat_w`×`feat_h` map, in row-major order like the graph's outputs.
#[allow(clippy::cast_precision_loss, clippy::too_many_arguments)]
fn decode_stride(
    feat_w: u32,
    feat_h: u32,
    stride: u32,
    cls: &[f32],
    obj: &[f32],
    bboxes: &[f32],
    kpss: &[f32],
    out: &mut Vec<Candidate>,
) -> Result<(), String> {
    let n = (feat_w * feat_h) as usize;
    if cls.len() != n || obj.len() != n || bboxes.len() != n * 4 || kpss.len() != n * 10 {
        return Err(format!(
            "stride {stride}: shape mismatch — {n} cells, cls={}, obj={}, bboxes={}, kps={}",
            cls.len(),
            obj.len(),
            bboxes.len(),
            kpss.len()
        ));
    }
    let stride_f = stride as f32;
    for r in 0..feat_h {
        for c in 0..feat_w {
            let idx = (r * feat_w + c) as usize;
            let score = (cls[idx].max(0.0) * obj[idx].max(0.0)).sqrt();
            if score < SCORE_THRESHOLD {
                continue;
            }
            let bd = &bboxes[idx * 4..idx * 4 + 4];
            let cx = (c as f32 + bd[0]) * stride_f;
            let cy = (r as f32 + bd[1]) * stride_f;
            let w = bd[2].exp() * stride_f;
            let h = bd[3].exp() * stride_f;
            let x1 = cx - w / 2.0;
            let y1 = cy - h / 2.0;
            let kd = &kpss[idx * 10..idx * 10 + 10];
            let mut kps = [(0.0_f32, 0.0_f32); 5];
            for (k, slot) in kps.iter_mut().enumerate() {
                *slot = (
                    (kd[2 * k] + c as f32) * stride_f,
                    (kd[2 * k + 1] + r as f32) * stride_f,
                );
            }
            out.push(Candidate {
                x1,
                y1,
                x2: x1 + w,
                y2: y1 + h,
                kps,
                score,
            });
        }
    }
    Ok(())
}

#[allow(clippy::similar_names)]
fn iou(a: &Candidate, b: &Candidate) -> f32 {
    let overlap_x1 = a.x1.max(b.x1);
    let overlap_y1 = a.y1.max(b.y1);
    let overlap_x2 = a.x2.min(b.x2);
    let overlap_y2 = a.y2.min(b.y2);
    let iw = (overlap_x2 - overlap_x1).max(0.0);
    let ih = (overlap_y2 - overlap_y1).max(0.0);
    let inter = iw * ih;
    let area_a = (a.x2 - a.x1).max(0.0) * (a.y2 - a.y1).max(0.0);
    let area_b = (b.x2 - b.x1).max(0.0) * (b.y2 - b.y1).max(0.0);
    let union = area_a + area_b - inter;
    if union <= 0.0 { 0.0 } else { inter / union }
}

/// Non-maximum suppression: sorts by descending score, keeps one box and
/// discards every other not-yet-discarded box whose `IoU` against the kept
/// one is above threshold. Returns the indices kept in `candidates`.
fn nms(candidates: &[Candidate], iou_threshold: f32) -> Vec<usize> {
    let mut order: Vec<usize> = (0..candidates.len()).collect();
    order.sort_by(|&a, &b| {
        candidates[b]
            .score
            .partial_cmp(&candidates[a].score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut kept = Vec::new();
    let mut suppressed = vec![false; candidates.len()];
    for &i in &order {
        if suppressed[i] {
            continue;
        }
        kept.push(i);
        for &j in &order {
            if j == i || suppressed[j] {
                continue;
            }
            if iou(&candidates[i], &candidates[j]) > iou_threshold {
                suppressed[j] = true;
            }
        }
    }
    kept
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;

    #[test]
    fn model_version_constant_is_stable() {
        assert_eq!(MODEL_VERSION, "yunet+sface");
    }

    #[test]
    fn decode_stride_recovers_a_known_box_and_score() {
        // Cell (c=2, r=1), stride 8: cx=(2+0)*8=16, cy=(1+0)*8=8,
        // w=exp(0)*8=8, h=exp(0)*8=8 → box (12,4,20,12). score=√(0.81)=0.9.
        let feat_w = 4;
        let feat_h = 4;
        let n = (feat_w * feat_h) as usize;
        let mut cls = vec![0.0_f32; n];
        let mut obj = vec![0.0_f32; n];
        let mut bboxes = vec![0.0_f32; n * 4];
        let kpss = vec![0.0_f32; n * 10];
        let idx = (feat_w + 2) as usize; // row 1, column 2
        cls[idx] = 0.9;
        obj[idx] = 0.9;
        bboxes[idx * 4] = 0.0;
        bboxes[idx * 4 + 1] = 0.0;
        bboxes[idx * 4 + 2] = 0.0;
        bboxes[idx * 4 + 3] = 0.0;
        let mut out = Vec::new();
        decode_stride(feat_w, feat_h, 8, &cls, &obj, &bboxes, &kpss, &mut out).unwrap();
        assert_eq!(out.len(), 1);
        let c = out[0];
        assert!((c.x1 - 12.0).abs() < 1e-3);
        assert!((c.y1 - 4.0).abs() < 1e-3);
        assert!((c.x2 - 20.0).abs() < 1e-3);
        assert!((c.y2 - 12.0).abs() < 1e-3);
        assert!((c.score - 0.9).abs() < 1e-4);
    }

    #[test]
    fn decode_stride_rejects_a_shape_mismatch() {
        let mut out = Vec::new();
        let err = decode_stride(
            2, 2, 8, &[0.9; 3], &[0.9; 4], &[0.0; 16], &[0.0; 40], &mut out,
        )
        .unwrap_err();
        assert!(err.contains("shape mismatch"));
    }

    #[test]
    fn decode_stride_filters_below_threshold() {
        let n = 2 * 2;
        let cls = vec![0.1_f32; n]; // sqrt(0.1*0.1)=0.1, below the 0.6 threshold
        let obj = vec![0.1_f32; n];
        let bboxes = vec![0.0_f32; n * 4];
        let kpss = vec![0.0_f32; n * 10];
        let mut out = Vec::new();
        decode_stride(2, 2, 8, &cls, &obj, &bboxes, &kpss, &mut out).unwrap();
        assert!(out.is_empty());
    }

    #[test]
    fn decode_stride_keeps_detections_above_threshold() {
        let cls = vec![0.99_f32];
        let obj = vec![0.99_f32];
        let bboxes = vec![0.0_f32; 4];
        let kpss = vec![0.0_f32; 10];
        let mut out = Vec::new();
        decode_stride(1, 1, 8, &cls, &obj, &bboxes, &kpss, &mut out).unwrap();
        assert_eq!(out.len(), 1);
        assert!((out[0].score - 0.99).abs() < 1e-6);
    }

    fn candidate(x1: f32, y1: f32, x2: f32, y2: f32, score: f32) -> Candidate {
        Candidate {
            x1,
            y1,
            x2,
            y2,
            kps: [(0.0, 0.0); 5],
            score,
        }
    }

    #[test]
    fn iou_of_identical_boxes_is_one() {
        let a = candidate(0.0, 0.0, 10.0, 10.0, 0.9);
        assert!((iou(&a, &a) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn iou_of_disjoint_boxes_is_zero() {
        let a = candidate(0.0, 0.0, 10.0, 10.0, 0.9);
        let b = candidate(20.0, 20.0, 30.0, 30.0, 0.9);
        assert!(iou(&a, &b).abs() < f32::EPSILON);
    }

    #[test]
    fn nms_keeps_the_higher_score_of_two_overlapping_boxes() {
        let candidates = vec![
            candidate(0.0, 0.0, 10.0, 10.0, 0.6),
            candidate(1.0, 1.0, 11.0, 11.0, 0.9), // nearly identical, higher score
        ];
        let kept = nms(&candidates, 0.4);
        assert_eq!(kept, vec![1]);
    }

    #[test]
    fn nms_keeps_both_disjoint_boxes() {
        let candidates = vec![
            candidate(0.0, 0.0, 10.0, 10.0, 0.6),
            candidate(100.0, 100.0, 110.0, 110.0, 0.9),
        ];
        let mut kept = nms(&candidates, 0.4);
        kept.sort_unstable();
        assert_eq!(kept, vec![0, 1]);
    }

    #[test]
    fn letterbox_centers_a_landscape_image_vertically() {
        let w = 240_u32;
        let h = 120_u32;
        let rgb = vec![255_u8; (w * h * 3) as usize];
        let (nchw, scale, pad_x, pad_y) = letterbox_to_nchw_bgr(&rgb, w, h, 640);
        assert!((scale - 640.0 / 240.0).abs() < 1e-4);
        assert_eq!(pad_x, 0);
        assert!(pad_y > 0);
        assert_eq!(nchw.len(), 3 * 640 * 640);
    }

    #[test]
    fn letterbox_swaps_red_and_blue_channels() {
        // A pure red pixel (255,0,0) in RGB must arrive in plane 0 (which
        // the function's doc comment declares to be B) as 0, and in plane
        // 2 (R) as 255 — the R↔B swap toward BGR.
        let w = 2_u32;
        let h = 2_u32;
        let mut rgb = vec![0_u8; (w * h * 3) as usize];
        rgb[0] = 255; // R del pixel (0,0)
        let (nchw, _scale, pad_x, pad_y) = letterbox_to_nchw_bgr(&rgb, w, h, 4);
        let plane = 4 * 4;
        let dst = (pad_y * 4 + pad_x) as usize; // source pixel (0,0), shifted by padding
        assert!((nchw[dst] - 0.0).abs() < 1e-6, "plane B");
        assert!((nchw[2 * plane + dst] - 255.0).abs() < 1e-6, "plane R");
    }

    #[test]
    fn model_dir_candidates_include_the_models_directory() {
        let candidates = model_dir_candidates();
        assert!(candidates.iter().any(|p| p.ends_with("models/yunet-sface")));
    }

    #[test]
    fn load_rejects_an_incomplete_model_directory() {
        let dir =
            std::env::temp_dir().join(format!("keeppix-face-incomplete-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let err = FaceModels::load(&dir).unwrap_err();
        assert!(
            err.contains("incomplete"),
            "expected incomplete-dir error, got: {err}"
        );
    }

    #[test]
    fn measure_reports_model_missing_without_weights_on_disk() {
        // In this development sandbox the YuNet/SFace weights aren't
        // present (no network access to `cdn.pyke.io` to compile
        // `ort-sys`) — the same declared limitation as MobileCLIP2-S2. If
        // this ever runs with the real weights present, the probe would
        // report "ok"; here we only verify that the degraded path doesn't
        // make up a number.
        if first_complete_model_dir().is_some() {
            eprintln!("skipping: real YuNet/SFace weights are present, probe would report ok");
            return;
        }
        let probe = measure_face_detection_inference();
        assert_eq!(probe.inference_status, "model_missing");
        assert!(probe.inference_ms.is_none());
    }
}
