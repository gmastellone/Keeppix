use serde::{Deserialize, Serialize};

/// Esito del probe hardware di decode/transcodifica.
///
/// Oggi **non si misura nulla**: la pipeline fotografica è CPU (JPEG/WebP) e
/// l'accelerazione serve al video (Fase 6). [`probe`] dichiara `"unprobed"`
/// così chi legge `settings` non scambia una costante per una misura.
/// `"software"` resterà il valore *misurato* quando il probe reale esisterà.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Capabilities {
    pub backend: String,
    pub decode_fps: Option<f32>,
}

/// Non rileva hardware. Restituisce `"unprobed"` di proposito: un backend
/// `"software"` affermerebbe una misura che non è mai stata fatta (spec
/// Fase 1b §4). Il rilevamento vero è lavoro della Fase 6.
#[must_use]
pub fn probe() -> Capabilities {
    Capabilities {
        backend: "unprobed".to_owned(),
        decode_fps: None,
    }
}
