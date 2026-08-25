//! `YuNet` (rilevamento) + `SFace` (impronta) via `ort` (Fase 8 Task 2/4,
//! sostituiti dal Task A del piano modelli IA — vedi
//! `docs/superpowers/plans/2026-08-22-keeppix-modelli-ai.md`). SCRFD/`ArcFace`
//! (`InsightFace`) non sono mai stati usati in produzione: pesi
//! research-only, mai scaricati, mai eseguiti — sostituiti prima di
//! qualunque rilascio. `YuNet` è MIT, `SFace` è Apache 2.0: entrambi liberi
//! anche per uso commerciale.
//!
//! Stesso stack di `clip.rs`: pesi ONNX locali sotto `models/`, zero rete a
//! runtime, sessione viva solo per la finestra di analisi. **Niente crate
//! `face_id`**: pinnerebbe una seconda versione di `ort` (0.4.4 vuole
//! `2.0.0-rc.13`) invece di riusare quella già in questo crate — esattamente
//! l'opposto di "un solo stack di inferenza" che la spec chiede. Ruling nel
//! ledger di fase.
//!
//! **Limite dichiarato di questa implementazione**: senza rete verso
//! `cdn.pyke.io` (per compilare `ort-sys`) da questa sandbox di sviluppo, il
//! decode qui sotto non è mai stato eseguito contro pesi ONNX reali dentro
//! `cargo test` — solo contro tensori sintetici nei test. **A differenza
//! della precedente implementazione SCRFD**, però, i due file `.onnx` reali
//! sono stati scaricati e ispezionati direttamente (`onnx.load`, grafo
//! Python) durante la stesura di questo modulo: gli shape di input/output
//! sotto (nomi, dimensioni, l'input fisso 640×640 di `YuNet`, l'output
//! 128-d `fc1` di `SFace`, l'unico input realmente richiesto `data` — gli
//! altri ~144 nomi nel grafo `SFace` hanno tutti un initializer, sono
//! parametri di `BatchNorm`/`PReLU` congelati dall'export, non vanno forniti a
//! `Session::run`) sono **verificati sul file reale**, non dedotti. Le
//! formule di decodifica (stride, `score = sqrt(cls·obj)`, box in stile
//! YOLO) vengono invece dalla lettura diretta di
//! `modules/objdetect/src/face_detect.cpp` di `OpenCV` — nomi di output letti
//! per **nome** (non per posizione), così un ordine diverso fallisce in modo
//! esplicito invece di produrre riquadri sbagliati in silenzio. Resta da
//! verificare in CI reale: che l'inferenza converga su una foto vera (il
//! test end-to-end lo fa girare per la prima volta, Task A punto 3) e la
//! calibrazione delle soglie di similarità (Task A punto 4).

use std::path::{Path, PathBuf};

use keeppix_domain::FaceBBox;
use ort::session::Session;
use ort::value::Tensor;

use crate::align::{self, SFACE_REFERENCE_112};

/// Identità stabile del checkpoint usato da probe, job e DB.
pub const MODEL_VERSION: &str = "yunet+sface";

const STRIDES: [u32; 3] = [8, 16, 32];
/// Lato dell'input del rilevatore `YuNet`: **non** una scelta di
/// implementazione come lo era per SCRFD — è la dimensione fissa dichiarata
/// dal grafo ONNX del checkpoint `2023mar_int8` pinnato dal piano
/// (`input: [1, 3, 640, 640]`, verificato caricando il file reale). `ort`
/// rifiuta un tensore di dimensione diversa: non è un parametro tunabile.
const DETECTOR_INPUT_SIZE: u32 = 640;
/// Soglie ufficiali del wrapper Python di `OpenCV` Zoo per `YuNet`
/// (`models/face_detection_yunet/yunet.py`: `confThreshold=0.6`,
/// `nmsThreshold=0.3`) — non i valori SCRFD del checkpoint precedente.
const SCORE_THRESHOLD: f32 = 0.6;
const NMS_IOU_THRESHOLD: f32 = 0.3;
/// Dimensione dell'impronta `SFace`: 128, **non** 512 come `ArcFace`
/// (verificato sia sull'output `fc1` del grafo ONNX reale, sia sul
/// commento ufficiale `OpenCV` `samples/dnn/js_face_recognition.html`, "Get
/// 128 floating points feature vector"). La migrazione dello schema che
/// segue da questo numero è in
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
    /// Occhio destro, occhio sinistro, naso, angolo bocca destro, angolo
    /// bocca sinistro — ordine nativo di `YuNet` (`demo.py`,
    /// `landmark_color`: indici 0..4 etichettati esattamente così), passato
    /// **senza permutazioni** a [`align::SFACE_REFERENCE_112`]: verificato
    /// su `objdetect/src/face_recognize.cpp`, `FaceRecognizerSF::alignCrop`
    /// — la stessa funzione ufficiale che fa da ponte fra questi due
    /// modelli legge i 5 punti del rilevatore in ordine stretto
    /// (`src_point[row][col] = face_mat.at<float>(0, row*2+col+4)`, nessun
    /// riordino) dentro lo stesso identico array di riferimento.
    pub landmarks: [(f32, f32); 5],
    pub score: f32,
}

/// Sessioni `YuNet` + `SFace` caricate in memoria. Dropparle libera la RAM
/// (stesso pattern di `MobileClip`, che a sua volta ha lo stesso limite:
/// onnxruntime non restituisce tutte le pagine al SO subito dopo `Drop`).
#[derive(Debug)]
pub struct FaceModels {
    detector: Session,
    embedder: Session,
}

impl FaceModels {
    /// Carica `detect.onnx` (`YuNet`) + `embed.onnx` (`SFace`) dalla directory
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
    /// il resto di nero) verso [`DETECTOR_INPUT_SIZE`] — l'unica dimensione
    /// che il grafo `YuNet` accetta — poi decodifica le tre teste (stride
    /// 8/16/32, un rilevamento per cella, niente ancore multiple a
    /// differenza di SCRFD) e applica soppressione dei non massimi.
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
    /// 112×112, poi ne calcola l'impronta `SFace` a 128 dimensioni, **non**
    /// L2-normalizzata dal modello: la normalizzazione è responsabilità del
    /// chiamante, per poterla applicare anche a un centroide medio senza
    /// ricalcolare l'impronta.
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
        let transform = align::similarity_transform_from_points(&src_points, &SFACE_REFERENCE_112);
        let aligned = align::warp_aligned_face(preview_rgb, preview_w, preview_h, &transform)?;
        self.embed_aligned(&aligned)
    }

    /// Impronta `SFace` da un ritaglio già allineato 112×112 RGB8.
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
        // `SFace` (dnn::blobFromImage(_aligned_img, 1, Size(112,112),
        // Scalar(0,0,0), true, false), letto dal sorgente C++ di
        // face_recognize.cpp): scalefactor 1 e mean 0 → pixel grezzi 0..255,
        // NESSUNA normalizzazione (a differenza di ArcFace, che divideva per
        // (pixel-127.5)/128). swapRB=true nel sorgente originale converte il
        // sorgente BGR-nativo di OpenCV in RGB per la rete — il nostro
        // buffer è già RGB, quindi va bene così com'è, senza scambiare i
        // canali (a differenza del rilevatore YuNet sotto, che invece li
        // scambia — vedi commento su `letterbox_to_nchw_bgr`).
        let mut nchw = vec![0.0_f32; 3 * size * size];
        let plane = size * size;
        for i in 0..plane {
            for c in 0..3 {
                nchw[c * plane + i] = f32::from(aligned_rgb_112[i * 3 + c]);
            }
        }
        let input = Tensor::from_array(([1usize, 3, size, size], nchw))
            .map_err(|e| format!("embedder input tensor: {e}"))?;
        // Il grafo `SFace` dichiara ~144 input aggiuntivi (parametri
        // BatchNorm/PReLU congelati dall'export) oltre a `data`: hanno
        // tutti un initializer nel grafo ONNX (verificato caricando il file
        // reale), quindi onnxruntime li usa come default senza che vadano
        // forniti qui — comportamento ONNX standard per input con
        // initializer, non un'omissione.
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

/// Letterbox: scala mantenendo l'aspect ratio così il lato più lungo diventi
/// `target`, poi centra il risultato su un canvas `target`×`target`
/// riempito di nero. Torna il tensore NCHW con pixel **grezzi** 0..255
/// (`dnn::blobFromImage` di `YuNet` non normalizza: scalefactor 1, mean 0,
/// letto da `face_detect.cpp`) e i parametri per invertire la
/// trasformazione sulle coordinate rilevate.
///
/// **Canali scambiati R↔B** (nome della funzione: produce BGR, non RGB): il
/// sorgente `OpenCV` chiama `blobFromImage` con `swapRB=false` su
/// un'immagine BGR-nativa (da `cv::imread`) — cioè la rete riceve BGR senza
/// alcuno scambio. Il nostro buffer sorgente è RGB (convenzione di questo
/// crate): per dare alla rete lo stesso ordine di canali con cui è stata
/// addestrata, scambiamo qui, non lasciamo l'ordine RGB invariato come
/// faceva il letterbox SCRFD precedente (quello alimentava RGB grezzo —
/// scelta ragionevole per la famiglia `InsightFace`, che nelle demo ufficiali
/// inverte esplicitamente BGR→RGB prima dell'inferenza; per `YuNet` l'evidenza
/// del sorgente dice il contrario). Non verificato empiricamente in questa
/// sandbox (nessuna inferenza reale eseguibile) — da confermare quando CI
/// esegue per la prima volta il test end-to-end con i pesi veri.
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
    let mut nchw = vec![0.0_f32; 3 * plane]; // nero: pixel grezzo 0

    for oy in 0..scaled_h.min(target) {
        for ox in 0..scaled_w.min(target) {
            let sx = ((ox as f32 + 0.5) / scale).clamp(0.0, (width - 1) as f32) as u32;
            let sy = ((oy as f32 + 0.5) / scale).clamp(0.0, (height - 1) as f32) as u32;
            let src = ((sy * width + sx) * 3) as usize;
            let dst = ((oy + pad_y) * target + (ox + pad_x)) as usize;
            // src+0=R, src+1=G, src+2=B (buffer RGB) → canale di rete
            // c=0 (B), c=1 (G), c=2 (R): scambio R↔B, vedi doc sopra.
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

/// Decodifica una testa `YuNet` a uno stride: un rilevamento per cella
/// (anchor-free, a differenza delle 2 ancore per cella di SCRFD), formule
/// lette da `face_detect.cpp` di `OpenCV`:
/// `cx=(c+bbox[0])·stride`, `cy=(r+bbox[1])·stride`,
/// `w=exp(bbox[2])·stride`, `h=exp(bbox[3])·stride`,
/// `score=√(cls·obj)`, `kp_n=((kps[2n]+c)·stride, (kps[2n+1]+r)·stride)`.
/// `c`/`r` sono gli indici (colonna, riga) della cella nella mappa
/// `feat_w`×`feat_h`, in ordine riga-per-riga come gli output del grafo.
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
        assert_eq!(MODEL_VERSION, "yunet+sface");
    }

    #[test]
    fn decode_stride_recovers_a_known_box_and_score() {
        // Cella (c=2, r=1), stride 8: cx=(2+0)*8=16, cy=(1+0)*8=8,
        // w=exp(0)*8=8, h=exp(0)*8=8 → box (12,4,20,12). score=√(0.81)=0.9.
        let feat_w = 4;
        let feat_h = 4;
        let n = (feat_w * feat_h) as usize;
        let mut cls = vec![0.0_f32; n];
        let mut obj = vec![0.0_f32; n];
        let mut bboxes = vec![0.0_f32; n * 4];
        let kpss = vec![0.0_f32; n * 10];
        let idx = (1 * feat_w + 2) as usize;
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
        let cls = vec![0.1_f32; n]; // sqrt(0.1*0.1)=0.1, sotto soglia 0.6
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
        // Un pixel rosso puro (255,0,0) in RGB deve arrivare nel piano 0
        // (che il commento della funzione dichiara essere B) come 0, e nel
        // piano 2 (R) come 255 — lo scambio R↔B verso BGR.
        let w = 2_u32;
        let h = 2_u32;
        let mut rgb = vec![0_u8; (w * h * 3) as usize];
        rgb[0] = 255; // R del pixel (0,0)
        let (nchw, _scale, pad_x, pad_y) = letterbox_to_nchw_bgr(&rgb, w, h, 4);
        let plane = 4 * 4;
        let dst = ((0 + pad_y) * 4 + (0 + pad_x)) as usize;
        assert!((nchw[0 * plane + dst] - 0.0).abs() < 1e-6, "piano B");
        assert!((nchw[2 * plane + dst] - 255.0).abs() < 1e-6, "piano R");
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
        // In questa sandbox di sviluppo i pesi YuNet/SFace non ci sono
        // (nessuna rete verso `cdn.pyke.io` per compilare `ort-sys`) — stesso
        // limite dichiarato di MobileCLIP2-S2 in Fase 7. Se un giorno girasse
        // con i pesi veri presenti, il probe riporterebbe "ok"; qui
        // verifichiamo solo che il percorso degradato non inventi un numero.
        if first_complete_model_dir().is_some() {
            eprintln!("skipping: real YuNet/SFace weights are present, probe would report ok");
            return;
        }
        let probe = measure_face_detection_inference();
        assert_eq!(probe.inference_status, "model_missing");
        assert!(probe.inference_ms.is_none());
    }
}
