use serde::{Deserialize, Serialize};

/// Backend di decode. In CI resta `software`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Capabilities {
    pub backend: String,
    pub decode_fps: Option<f32>,
}

#[must_use]
pub fn probe() -> Capabilities {
    Capabilities {
        backend: "software".to_owned(),
        decode_fps: None,
    }
}
