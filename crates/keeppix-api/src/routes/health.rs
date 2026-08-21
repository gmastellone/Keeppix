use serde::Serialize;

use crate::json::Json;

#[derive(Serialize, utoipa::ToSchema)]
pub struct Health {
    status: &'static str,
    version: &'static str,
}

#[utoipa::path(
    get,
    path = "/health",
    tag = "health",
    operation_id = "health_get",
    summary = "Liveness check",
    responses(
        (status = 200, description = "Il processo risponde", body = Health)
    )
)]
pub async fn get() -> Json<Health> {
    Json(Health {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    })
}
