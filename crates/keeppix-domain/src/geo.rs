use serde::{Deserialize, Serialize};

use crate::GeoPoint;

/// Località `GeoNames` indipendente da utenti e librerie.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Place {
    pub id: i64,
    pub name: String,
    pub ascii_name: String,
    pub country_code: Option<String>,
    pub admin1: Option<String>,
    pub admin2: Option<String>,
    pub location: GeoPoint,
    pub population: i32,
}
