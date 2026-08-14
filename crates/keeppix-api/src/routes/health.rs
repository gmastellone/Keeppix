use serde::Serialize;

use crate::json::Json;

#[derive(Serialize)]
pub struct Health {
    status: &'static str,
    version: &'static str,
}

pub async fn get() -> Json<Health> {
    Json(Health {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    })
}
