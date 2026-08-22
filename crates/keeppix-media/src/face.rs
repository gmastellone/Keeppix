//! SCRFD (rilevamento) + `ArcFace` (impronta) via `ort` (Fase 8 Task 2/4).
//!
//! Stesso stack di `clip.rs`: pesi ONNX locali sotto `models/`, zero rete a
//! runtime, sessione viva solo per la finestra di analisi. **Niente crate
//! `face_id`**: pinnerebbe una seconda versione di `ort` (0.4.4 vuole
//! `2.0.0-rc.13`) invece di riusare quella già in questo crate — esattamente
//! l'opposto di "un solo stack di inferenza" che la spec chiede. Ruling nel
//! ledger di fase.
//!
//! **Limite dichiarato di questa implementazione**: senza rete verso
//! `HuggingFace` da questa sandbox di sviluppo (stesso limite già noto per
//! MobileCLIP2-S2, Fase 7), il decode SCRFD qui sotto non è mai stato
//! eseguito contro pesi ONNX reali — solo contro tensori sintetici nei test.
//! La convenzione di output (`score_{8,16,32}`, `bbox_{8,16,32}`,
//! `kps_{8,16,32}`, distanze l/t/r/b scalate per lo stride) è quella
//! documentata dall'export ufficiale insightface/SCRFD, letta per **nome**
//! di output (non per posizione) così un ordine diverso fallisce in modo
//! esplicito invece di produrre riquadri sbagliati in silenzio.

use std::path::{Path, PathBuf};

use keeppix_domain::FaceBBox;
use ort::session::Session;
use ort::value::Tensor;

use crate::align::{self, ARCFACE_REFERENCE_112};

/// Identità stabile del checkpoint usato da probe, job e DB.
pub const MODEL_VERSION: &str = "scrfd-500mf+arcface";

const STRIDES: [u32; 3] = [8, 16, 32];
const NUM_ANCHORS: usize = 2;
/// Lato dell'input del rilevatore: multiplo di 32 (i tre stride della FPN
/// devono dividere esattamente la mappa di feature), ≥ 240px (la miniatura
/// su cui gira il rilevamento, spec §2.3 emendamento).
const DETECTOR_INPUT_SIZE: u32 = 256;
const SCORE_THRESHOLD: f32 = 0.5;
const NMS_IOU_THRESHOLD: f32 = 0.4;
const ARCFACE_EMBED_DIM: usize = 512;

#[must_use]
pub fn model_dir_candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(p) = std::env::var("KEEPPIX_FACE_MODEL_DIR") {
        out.push(PathBuf::from(p));
    }
    if let Ok(dir) = std::env::var("KEEPPIX_MODELS_DIR") {
        out.push(PathBuf::from(dir).join("scrfd-arcface"));
    }
    out.push(PathBuf::from("models/scrfd-arcface"));
    if let Ok(manifest) = std::env::var("CARGO_MANIFEST_DIR") {
        let crate_dir = PathBuf::from(manifest);
        if let Some(workspace) = crate_dir.parent().and_then(Path::parent) {
            out.push(workspace.join("models/scrfd-arcface"));
        }
    }
    out
}

fn is_complete_model_dir(dir: &Path) -> bool {
    dir.join("detect.onnx").is_file() && dir.join("embed.onnx").is_file()
}

/// Prova di inferenza sul rilevatore, stesso contratto di
/// [`crate::ai::measure_image_inference`] per CLIP (Task 1/2 della Fase 7):
/// una passata di warm-up più una misurata, su un input nero. Se i pesi
/// mancano torna `inference_status = "model_missing"` invece di inventare
/// un numero — quello che Task 2 di questa fase deve mettere nel ledger.
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

/// Un volto rilevato, in coordinate **relative** (0..1) all'immagine passata
/// a [`FaceModels::detect`] — sopravvive a derivati di dimensione diversa.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DetectedFace {
    pub bbox: FaceBBox,
    /// Occhio sinistro, occhio destro, naso, angolo bocca sinistro, angolo
    /// bocca destro — stesso ordine di [`align::ARCFACE_REFERENCE_112`].
    pub landmarks: [(f32, f32); 5],
    pub score: f32,
}

/// Sessioni SCRFD + `ArcFace` caricate in memoria. Dropparle libera la RAM
/// (stesso pattern di `MobileClip`, che a sua volta ha lo stesso limite:
/// onnxruntime non restituisce tutte le pagine al SO subito dopo `Drop`).
#[derive(Debug)]
pub struct FaceModels {
    detector: Session,
    embedder: Session,
}

impl FaceModels {
    /// Carica `detect.onnx` (SCRFD) + `embed.onnx` (`ArcFace`) dalla directory
    /// modello.
    ///
    /// # Errors
    /// Directory incompleta, o fallimento di ort nel caricare uno dei due
    /// grafi.
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

    /// Rileva volti in un'immagine RGB8 interleaved (`width`×`height`).
    /// Applica un letterbox (ridimensiona mantenendo l'aspect ratio, riempie
    /// il resto di nero) verso [`DETECTOR_INPUT_SIZE`], poi decodifica le
    /// tre teste SCRFD (stride 8/16/32) e applica soppressione dei non
    /// massimi.
    ///
    /// # Errors
    /// Buffer RGB incoerente con `width`×`height`, o fallimento di ort.
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
            letterbox_to_nchw(rgb, width, height, DETECTOR_INPUT_SIZE);
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
            let scores = named_output_f32(&outputs, &format!("score_{stride}"))?;
            let bboxes = named_output_f32(&outputs, &format!("bbox_{stride}"))?;
            let kpss = named_output_f32(&outputs, &format!("kps_{stride}"))?;
            let feat = DETECTOR_INPUT_SIZE / stride;
            decode_stride(feat, feat, stride, &scores, &bboxes, &kpss, &mut candidates)?;
        }

        let kept = nms(&candidates, NMS_IOU_THRESHOLD);
        let mut out = Vec::with_capacity(kept.len());
        for idx in kept {
            let c = &candidates[idx];
            // Da spazio letterbox a spazio immagine originale, poi a
            // coordinate relative — l'ordine conta: prima si toglie il
            // padding, poi si divide per la scala, infine si normalizza.
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

    /// Allinea (Umeyama/similarità) e incornicia il volto dalla preview a
    /// 112×112, poi ne calcola l'impronta `ArcFace` a 512 dimensioni, **non**
    /// L2-normalizzata dal modello (a differenza di `MobileClip`): la
    /// normalizzazione è responsabilità del chiamante, per poterla applicare
    /// anche a un centroide medio senza ricalcolare l'impronta.
    ///
    /// `landmarks_rel` sono le 5 coordinate relative (0..1) **rispetto a
    /// `preview_w`×`preview_h`**, non alla miniatura di [`Self::detect`] —
    /// spec §2.1 emendamento: rilevamento su miniatura, impronta su preview.
    ///
    /// # Errors
    /// Buffer RGB incoerente, o fallimento di ort.
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
        let transform =
            align::similarity_transform_from_points(&src_points, &ARCFACE_REFERENCE_112);
        let aligned = align::warp_aligned_face(preview_rgb, preview_w, preview_h, &transform)?;
        self.embed_aligned(&aligned)
    }

    /// Impronta `ArcFace` da un ritaglio già allineato 112×112 RGB8.
    ///
    /// # Errors
    /// Dimensione errata, o fallimento di ort.
    pub fn embed_aligned(&mut self, aligned_rgb_112: &[u8]) -> Result<Vec<f32>, String> {
        let size = align::ALIGNED_FACE_SIZE as usize;
        if aligned_rgb_112.len() != size * size * 3 {
            return Err(format!(
                "expected {size}x{size}x3 aligned crop, got {} bytes",
                aligned_rgb_112.len()
            ));
        }
        // ArcFace: (pixel - 127.5) / 128.0, NCHW.
        let mut nchw = vec![0.0_f32; 3 * size * size];
        let plane = size * size;
        for i in 0..plane {
            for c in 0..3 {
                let v = f32::from(aligned_rgb_112[i * 3 + c]);
                nchw[c * plane + i] = (v - 127.5) / 128.0;
            }
        }
        let input = Tensor::from_array(([1usize, 3, size, size], nchw))
            .map_err(|e| format!("embedder input tensor: {e}"))?;
        let outputs = self
            .embedder
            .run(ort::inputs![input])
            .map_err(|e| format!("embedder infer: {e}"))?;
        let (_shape, data) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| format!("embedder output: {e}"))?;
        if data.len() != ARCFACE_EMBED_DIM {
            return Err(format!(
                "expected {ARCFACE_EMBED_DIM}-d embedding, got {}",
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

/// Letterbox: scala mantenendo l'aspect ratio così il lato più lungo diventi
/// `target`, poi centra il risultato su un canvas `target`×`target`
/// riempito di nero. Torna il tensore NCHW normalizzato
/// `(pixel - 127.5) / 128.0` (convenzione SCRFD standard) e i parametri per
/// invertire la trasformazione sulle coordinate rilevate.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
fn letterbox_to_nchw(
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
    let mut nchw = vec![-127.5_f32 / 128.0; 3 * plane]; // nero dopo normalizzazione

    for oy in 0..scaled_h.min(target) {
        for ox in 0..scaled_w.min(target) {
            let sx = ((ox as f32 + 0.5) / scale).clamp(0.0, (width - 1) as f32) as u32;
            let sy = ((oy as f32 + 0.5) / scale).clamp(0.0, (height - 1) as f32) as u32;
            let src = ((sy * width + sx) * 3) as usize;
            let dst = ((oy + pad_y) * target + (ox + pad_x)) as usize;
            for c in 0..3 {
                let v = (f32::from(rgb[src + c]) - 127.5) / 128.0;
                nchw[c * plane + dst] = v;
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

/// Centri delle ancore per una mappa di feature `feat_w`×`feat_h` a questo
/// `stride`, ripetuti `NUM_ANCHORS` volte per cella — stessa convenzione
/// SCRFD ufficiale (`anchor_centers`).
#[allow(clippy::cast_precision_loss)]
fn anchor_centers(feat_w: u32, feat_h: u32, stride: u32) -> Vec<(f32, f32)> {
    let mut out = Vec::with_capacity((feat_w * feat_h) as usize * NUM_ANCHORS);
    for iy in 0..feat_h {
        for ix in 0..feat_w {
            let cx = (ix as f32 + 0.5) * stride as f32;
            let cy = (iy as f32 + 0.5) * stride as f32;
            for _ in 0..NUM_ANCHORS {
                out.push((cx, cy));
            }
        }
    }
    out
}

/// `distance2bbox`: `bbox_pred` sono le quattro distanze (sinistra, alto,
/// destra, basso) dal centro dell'ancora, scalate per lo stride — stessa
/// decodifica FCOS-style usata dall'export ufficiale SCRFD.
const fn distance2bbox(center: (f32, f32), d: [f32; 4], stride: f32) -> (f32, f32, f32, f32) {
    (
        center.0 - d[0] * stride,
        center.1 - d[1] * stride,
        center.0 + d[2] * stride,
        center.1 + d[3] * stride,
    )
}

fn distance2kps(center: (f32, f32), d: [f32; 10], stride: f32) -> [(f32, f32); 5] {
    let mut out = [(0.0_f32, 0.0_f32); 5];
    for (k, slot) in out.iter_mut().enumerate() {
        *slot = (
            center.0 + d[2 * k] * stride,
            center.1 + d[2 * k + 1] * stride,
        );
    }
    out
}

#[allow(clippy::cast_precision_loss)]
fn decode_stride(
    feat_w: u32,
    feat_h: u32,
    stride: u32,
    scores: &[f32],
    bboxes: &[f32],
    kpss: &[f32],
    out: &mut Vec<Candidate>,
) -> Result<(), String> {
    let centers = anchor_centers(feat_w, feat_h, stride);
    let n = centers.len();
    if scores.len() != n || bboxes.len() != n * 4 || kpss.len() != n * 10 {
        return Err(format!(
            "stride {stride}: shape mismatch — {n} anchors, scores={}, bboxes={}, kps={}",
            scores.len(),
            bboxes.len(),
            kpss.len()
        ));
    }
    for i in 0..n {
        let score = scores[i];
        if score < SCORE_THRESHOLD {
            continue;
        }
        let bd: [f32; 4] = [
            bboxes[i * 4],
            bboxes[i * 4 + 1],
            bboxes[i * 4 + 2],
            bboxes[i * 4 + 3],
        ];
        let (x1, y1, x2, y2) = distance2bbox(centers[i], bd, stride as f32);
        let mut kd = [0.0_f32; 10];
        kd.copy_from_slice(&kpss[i * 10..i * 10 + 10]);
        let kps = distance2kps(centers[i], kd, stride as f32);
        out.push(Candidate {
            x1,
            y1,
            x2,
            y2,
            kps,
            score,
        });
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

/// Soppressione dei non massimi: ordina per punteggio decrescente, tiene un
/// riquadro e scarta ogni altro riquadro non ancora scartato con `IoU` oltre
/// soglia rispetto a quello tenuto. Torna gli indici tenuti in `candidates`.
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
        assert_eq!(MODEL_VERSION, "scrfd-500mf+arcface");
    }

    #[test]
    fn anchor_centers_cover_the_whole_feature_map_with_two_anchors_each() {
        let centers = anchor_centers(4, 3, 8);
        assert_eq!(centers.len(), 4 * 3 * NUM_ANCHORS);
        // Prima cella: centro a (4,4) per stride 8, ripetuto due volte.
        assert_eq!(centers[0], (4.0, 4.0));
        assert_eq!(centers[1], (4.0, 4.0));
        // Seconda cella (ix=1): centro a (12,4).
        assert_eq!(centers[2], (12.0, 4.0));
    }

    #[test]
    fn distance2bbox_recovers_a_known_box() {
        // Centro (100,100), distanze (10,20,30,40), stride 1 → box
        // (90,80,130,140).
        let (x1, y1, x2, y2) = distance2bbox((100.0, 100.0), [10.0, 20.0, 30.0, 40.0], 1.0);
        assert_eq!((x1, y1, x2, y2), (90.0, 80.0, 130.0, 140.0));
    }

    #[test]
    fn distance2bbox_scales_by_stride() {
        let (x1, y1, x2, y2) = distance2bbox((100.0, 100.0), [1.0, 1.0, 1.0, 1.0], 8.0);
        assert_eq!((x1, y1, x2, y2), (92.0, 92.0, 108.0, 108.0));
    }

    #[test]
    fn distance2kps_places_five_points_relative_to_center() {
        let d = [1.0, 0.0, 0.0, 1.0, -1.0, 0.0, 0.0, -1.0, 2.0, 2.0];
        let kps = distance2kps((50.0, 50.0), d, 2.0);
        assert_eq!(kps[0], (52.0, 50.0));
        assert_eq!(kps[1], (50.0, 52.0));
        assert_eq!(kps[2], (48.0, 50.0));
        assert_eq!(kps[3], (50.0, 48.0));
        assert_eq!(kps[4], (54.0, 54.0));
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
            candidate(1.0, 1.0, 11.0, 11.0, 0.9), // quasi identico, score più alto
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
    fn decode_stride_rejects_a_shape_mismatch() {
        let mut out = Vec::new();
        let err = decode_stride(2, 2, 8, &[0.9; 3], &[0.0; 16], &[0.0; 40], &mut out).unwrap_err();
        assert!(err.contains("shape mismatch"));
    }

    #[test]
    fn decode_stride_filters_below_threshold() {
        let n = 2 * 2 * NUM_ANCHORS;
        let scores = vec![0.1_f32; n]; // tutti sotto soglia
        let bboxes = vec![0.0_f32; n * 4];
        let kpss = vec![0.0_f32; n * 10];
        let mut out = Vec::new();
        decode_stride(2, 2, 8, &scores, &bboxes, &kpss, &mut out).unwrap();
        assert!(out.is_empty());
    }

    #[test]
    fn decode_stride_keeps_detections_above_threshold() {
        let n = NUM_ANCHORS;
        let mut scores = vec![0.1_f32; n];
        scores[0] = 0.99;
        let bboxes = vec![0.0_f32; n * 4];
        let kpss = vec![0.0_f32; n * 10];
        let mut out = Vec::new();
        decode_stride(1, 1, 8, &scores, &bboxes, &kpss, &mut out).unwrap();
        assert_eq!(out.len(), 1);
        assert!((out[0].score - 0.99).abs() < 1e-6);
    }

    #[test]
    fn letterbox_centers_a_landscape_image_vertically() {
        let w = 240_u32;
        let h = 120_u32;
        let rgb = vec![255_u8; (w * h * 3) as usize];
        let (nchw, scale, pad_x, pad_y) = letterbox_to_nchw(&rgb, w, h, 256);
        assert!((scale - 256.0 / 240.0).abs() < 1e-4);
        assert_eq!(pad_x, 0);
        assert!(pad_y > 0);
        assert_eq!(nchw.len(), 3 * 256 * 256);
    }

    #[test]
    fn model_dir_candidates_include_the_models_directory() {
        let candidates = model_dir_candidates();
        assert!(
            candidates
                .iter()
                .any(|p| p.ends_with("models/scrfd-arcface"))
        );
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
        // In questa sandbox di sviluppo i pesi SCRFD/ArcFace non ci sono
        // (nessuna rete verso HuggingFace) — stesso limite dichiarato di
        // MobileCLIP2-S2 in Fase 7. Se un giorno girasse con i pesi veri
        // presenti, il probe riporterebbe "ok"; qui verifichiamo solo che il
        // percorso degradato non inventi un numero.
        if first_complete_model_dir().is_some() {
            eprintln!("skipping: real SCRFD/ArcFace weights are present, probe would report ok");
            return;
        }
        let probe = measure_face_detection_inference();
        assert_eq!(probe.inference_status, "model_missing");
        assert!(probe.inference_ms.is_none());
    }
}
