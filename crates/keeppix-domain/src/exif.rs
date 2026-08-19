use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::GeoPoint;

/// Metadati letti dalla testa del file. `raw` è immutabile una volta scritto.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExifData {
    pub raw: serde_json::Value,
    pub taken_at_utc: DateTime<Utc>,
    pub tz_offset_minutes: i32,
    pub tz_assumed: bool,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub lens: Option<String>,
    pub iso: Option<i32>,
    pub f_number: Option<f32>,
    pub exposure: Option<String>,
    pub focal_length: Option<f32>,
    pub gps: Option<GeoPoint>,
}
