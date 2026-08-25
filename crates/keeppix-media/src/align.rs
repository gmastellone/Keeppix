//! Allineamento del volto (Fase 8 §2.1): i cinque punti di riferimento
//! (occhi, naso, angoli bocca) verso una posa canonica 112×112, prima
//! dell'impronta `SFace`. Un volto storto produce un embedding peggiore, e
//! la qualità del raggruppamento dipende quasi tutta da qui.
//!
//! Niente SVD generica: una trasformazione di similarità 2D (rotazione +
//! scala uniforme, **mai** una riflessione — un volto specchiato non è lo
//! stesso volto) ha solo 4 gradi di libertà ed è esattamente il minimo
//! quadratico di una moltiplicazione per un numero complesso `w = a + bi`:
//! `q_i ≈ w·p_i + t`. La soluzione chiusa è la proiezione complessa
//! standard, non un caso speciale ad hoc.

/// I cinque punti di riferimento su un ritaglio 112×112, ordine nativo
/// YuNet (occhio destro, occhio sinistro, naso, angolo bocca destro,
/// angolo bocca sinistro — vedi il commento su
/// [`crate::face::DetectedFace::landmarks`]). Numericamente identico alla
/// costante `ArcFace`/insightface usata dalla precedente implementazione
/// SCRFD: verificato leggendo `FaceRecognizerSF::alignCrop`
/// (`objdetect/src/face_recognize.cpp` di OpenCV), che usa lo stesso
/// identico array per `SFace` — stesso template di riferimento, modello di
/// impronta diverso. Il nome è cambiato, i numeri no.
pub const SFACE_REFERENCE_112: [(f32, f32); 5] = [
    (38.2946, 51.6963),
    (73.5318, 51.5014),
    (56.0252, 71.7366),
    (41.5493, 92.3655),
    (70.7299, 92.2041),
];

pub const ALIGNED_FACE_SIZE: u32 = 112;

/// Matrice affine `[a, -b, tx; b, a, ty]` (rotazione+scala uniforme+
/// traslazione, mai riflessione).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SimilarityTransform {
    pub a: f32,
    pub b: f32,
    pub tx: f32,
    pub ty: f32,
}

impl SimilarityTransform {
    #[must_use]
    pub const fn identity() -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            tx: 0.0,
            ty: 0.0,
        }
    }

    /// Applica la trasformazione a un punto `(x, y)`.
    #[must_use]
    pub fn apply(&self, x: f32, y: f32) -> (f32, f32) {
        (
            self.a * x - self.b * y + self.tx,
            self.b * x + self.a * y + self.ty,
        )
    }

    /// Inversa: `SimilarityTransform` è invertibile finché `a²+b² > 0`
    /// (sempre vero per punti non tutti coincidenti).
    #[must_use]
    #[allow(clippy::similar_names)]
    pub fn inverse(&self) -> Self {
        let denom = self.a * self.a + self.b * self.b;
        if denom == 0.0 {
            return Self::identity();
        }
        let inv_a = self.a / denom;
        let inv_b = -self.b / denom;
        // t' = -R^-1 * t
        let inv_tx = -(inv_a * self.tx - inv_b * self.ty);
        let inv_ty = -(inv_b * self.tx + inv_a * self.ty);
        Self {
            a: inv_a,
            b: inv_b,
            tx: inv_tx,
            ty: inv_ty,
        }
    }
}

/// Stima ai minimi quadrati la trasformazione di similarità che porta
/// `src` più vicino possibile a `dst` (stesso ordine dei punti nei due
/// array). Se i punti sorgente coincidono tutti (varianza nulla), torna
/// l'identità invece di dividere per zero.
#[must_use]
pub fn similarity_transform_from_points(
    src: &[(f32, f32)],
    dst: &[(f32, f32)],
) -> SimilarityTransform {
    debug_assert_eq!(src.len(), dst.len());
    if src.is_empty() {
        return SimilarityTransform::identity();
    }

    let mean_src = mean(src);
    let mean_dst = mean(dst);

    let mut num_a = 0.0_f32; // Σ(px·qx + py·qy)
    let mut num_b = 0.0_f32; // Σ(px·qy - py·qx)
    let mut denom = 0.0_f32; // Σ(px² + py²)
    for (&(sx, sy), &(dx, dy)) in src.iter().zip(dst.iter()) {
        let px = sx - mean_src.0;
        let py = sy - mean_src.1;
        let qx = dx - mean_dst.0;
        let qy = dy - mean_dst.1;
        num_a += px * qx + py * qy;
        num_b += px * qy - py * qx;
        denom += px * px + py * py;
    }
    if denom <= f32::EPSILON {
        return SimilarityTransform {
            a: 1.0,
            b: 0.0,
            tx: mean_dst.0 - mean_src.0,
            ty: mean_dst.1 - mean_src.1,
        };
    }
    let a = num_a / denom;
    let b = num_b / denom;
    let tx = mean_dst.0 - (a * mean_src.0 - b * mean_src.1);
    let ty = mean_dst.1 - (b * mean_src.0 + a * mean_src.1);
    SimilarityTransform { a, b, tx, ty }
}

#[allow(clippy::cast_precision_loss)]
fn mean(points: &[(f32, f32)]) -> (f32, f32) {
    let n = points.len() as f32;
    let sx: f32 = points.iter().map(|p| p.0).sum();
    let sy: f32 = points.iter().map(|p| p.1).sum();
    (sx / n, sy / n)
}

/// Ritaglia e allinea un volto da un'immagine RGB8 interleaved verso un
/// buffer `ALIGNED_FACE_SIZE`×`ALIGNED_FACE_SIZE` RGB8, usando la
/// trasformazione **inversa** (per ogni pixel di destinazione, campiona la
/// sorgente — l'unico modo che non lascia buchi nell'immagine di uscita).
/// Campionamento bilineare; fuori dai bordi dell'immagine sorgente il pixel
/// è nero (0,0,0), come per un volto vicino al bordo della foto.
///
/// # Errors
/// `src_rgb.len()` non coerente con `src_w * src_h * 3`.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
pub fn warp_aligned_face(
    src_rgb: &[u8],
    src_w: u32,
    src_h: u32,
    transform: &SimilarityTransform,
) -> Result<Vec<u8>, String> {
    if src_rgb.len() != (src_w as usize) * (src_h as usize) * 3 {
        return Err("rgb buffer length mismatch".to_owned());
    }
    let inverse = transform.inverse();
    let size = ALIGNED_FACE_SIZE as usize;
    let mut out = vec![0_u8; size * size * 3];
    for oy in 0..ALIGNED_FACE_SIZE {
        for ox in 0..ALIGNED_FACE_SIZE {
            let (sx, sy) = inverse.apply(ox as f32, oy as f32);
            let pixel = sample_bilinear(src_rgb, src_w, src_h, sx, sy);
            let dst = (oy as usize * size + ox as usize) * 3;
            out[dst..dst + 3].copy_from_slice(&pixel);
        }
    }
    Ok(out)
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
fn sample_bilinear(rgb: &[u8], width: u32, height: u32, x: f32, y: f32) -> [u8; 3] {
    if x < 0.0 || y < 0.0 || x > (width as f32 - 1.0) || y > (height as f32 - 1.0) {
        return [0, 0, 0];
    }
    let x0 = x.floor() as u32;
    let y0 = y.floor() as u32;
    let x1 = (x0 + 1).min(width - 1);
    let y1 = (y0 + 1).min(height - 1);
    let fx = x - x0 as f32;
    let fy = y - y0 as f32;

    let at = |px: u32, py: u32, c: usize| -> f32 {
        f32::from(rgb[((py * width + px) * 3) as usize + c])
    };

    let mut out = [0_u8; 3];
    for (c, slot) in out.iter_mut().enumerate() {
        let top = at(x0, y0, c) * (1.0 - fx) + at(x1, y0, c) * fx;
        let bottom = at(x0, y1, c) * (1.0 - fx) + at(x1, y1, c) * fx;
        let v = top * (1.0 - fy) + bottom * fy;
        *slot = v.round().clamp(0.0, 255.0) as u8;
    }
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;

    fn approx(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() < eps
    }

    #[test]
    fn identical_points_yield_the_identity() {
        let pts = SFACE_REFERENCE_112;
        let t = similarity_transform_from_points(&pts, &pts);
        assert!(approx(t.a, 1.0, 1e-4));
        assert!(approx(t.b, 0.0, 1e-4));
        assert!(approx(t.tx, 0.0, 1e-3));
        assert!(approx(t.ty, 0.0, 1e-3));
    }

    #[test]
    fn recovers_a_known_translation() {
        let src = [
            (0.0, 0.0),
            (10.0, 0.0),
            (10.0, 10.0),
            (0.0, 10.0),
            (5.0, 5.0),
        ];
        let dst: Vec<(f32, f32)> = src.iter().map(|&(x, y)| (x + 3.0, y - 7.0)).collect();
        let t = similarity_transform_from_points(&src, &dst);
        for &(x, y) in &src {
            let (px, py) = t.apply(x, y);
            assert!(approx(px, x + 3.0, 1e-3));
            assert!(approx(py, y - 7.0, 1e-3));
        }
    }

    #[test]
    fn recovers_a_known_scale_and_rotation() {
        // Rotazione di 90° (a=0,b=1) e scala 2×, poi traslazione.
        let src = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0), (0.5, 0.5)];
        let scale = 2.0_f32;
        let dst: Vec<(f32, f32)> = src
            .iter()
            .map(|&(x, y)| {
                // rotazione 90°: (x,y) -> (-y, x)
                (scale * -y + 4.0, scale * x + 1.0)
            })
            .collect();
        let t = similarity_transform_from_points(&src, &dst);
        assert!(approx(t.a, 0.0, 1e-3), "a={}", t.a);
        assert!(approx(t.b, scale, 1e-3), "b={}", t.b);
        for (&(x, y), &(ex, ey)) in src.iter().zip(dst.iter()) {
            let (px, py) = t.apply(x, y);
            assert!(approx(px, ex, 1e-2));
            assert!(approx(py, ey, 1e-2));
        }
    }

    #[test]
    fn never_produces_a_reflection() {
        // Punti sorgente e destinazione a chiralità opposta: il fit ai
        // minimi quadrati resta comunque una similarità pura (a,b), mai
        // una matrice con determinante negativo — per costruzione, non è
        // nemmeno rappresentabile con questo modello.
        let src = [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0)];
        let mirrored = [(0.0, 0.0), (-1.0, 0.0), (0.0, 1.0)]; // riflessione sull'asse Y
        let t = similarity_transform_from_points(&src, &mirrored);
        // determinante della parte lineare [[a,-b],[b,a]] = a²+b² ≥ 0 sempre.
        let determinant = t.a * t.a + t.b * t.b;
        assert!(determinant >= 0.0);
    }

    #[test]
    fn identity_transform_round_trips_through_inverse() {
        let t = SimilarityTransform::identity();
        let inv = t.inverse();
        assert!(approx(inv.a, 1.0, 1e-6));
        assert!(approx(inv.b, 0.0, 1e-6));
    }

    #[test]
    fn inverse_undoes_the_forward_transform() {
        let t = SimilarityTransform {
            a: 1.5,
            b: 0.5,
            tx: 10.0,
            ty: -4.0,
        };
        let inv = t.inverse();
        let (x, y) = t.apply(3.0, 7.0);
        let (bx, by) = inv.apply(x, y);
        assert!(approx(bx, 3.0, 1e-3));
        assert!(approx(by, 7.0, 1e-3));
    }

    #[test]
    fn warp_rejects_a_mismatched_buffer() {
        let rgb = vec![0_u8; 10];
        let t = SimilarityTransform::identity();
        assert!(warp_aligned_face(&rgb, 100, 100, &t).is_err());
    }

    #[test]
    fn warp_produces_the_declared_output_size() {
        let w = 20_u32;
        let h = 20_u32;
        let rgb = vec![128_u8; (w * h * 3) as usize];
        let t = SimilarityTransform::identity();
        let out = warp_aligned_face(&rgb, w, h, &t).unwrap();
        assert_eq!(
            out.len(),
            (ALIGNED_FACE_SIZE * ALIGNED_FACE_SIZE * 3) as usize
        );
    }

    #[test]
    fn warp_samples_a_uniform_color_unchanged() {
        let w = 200_u32;
        let h = 200_u32;
        let mut rgb = vec![0_u8; (w * h * 3) as usize];
        for px in rgb.chunks_mut(3) {
            px.copy_from_slice(&[200, 50, 10]);
        }
        // Identità: il centro del ritaglio 112×112 campiona un pixel ben
        // dentro l'immagine sorgente 200×200, colore uniforme.
        let t = SimilarityTransform::identity();
        let out = warp_aligned_face(&rgb, w, h, &t).unwrap();
        let center = ((56 * 112 + 56) * 3) as usize;
        assert_eq!(&out[center..center + 3], &[200, 50, 10]);
    }
}
