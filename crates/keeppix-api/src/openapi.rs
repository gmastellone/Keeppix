//! Documento `OpenAPI` generato dalle annotazioni sugli handler e sui tipi:
//! nasce dal codice, quindi non può divergere dall'implementazione.

use utoipa::OpenApi;

use crate::routes::{auth, setup};

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Keeppix API",
        version = env!("CARGO_PKG_VERSION"),
        description = "API di Keeppix. Contratto congelato: solo aggiunte entro /api/v1."
    ),
    paths(
        setup::status,
        setup::create,
        auth::login,
        auth::refresh,
        auth::logout,
        auth::me,
    ),
    components(schemas(
        auth::UserView,
        auth::LoginRequest,
        auth::LoginResponse,
        auth::MeResponse,
        setup::SetupStatus,
        setup::SetupRequest,
        setup::SetupResponse,
    )),
    tags(
        (name = "setup", description = "Configurazione iniziale dell'istanza"),
        (name = "auth", description = "Autenticazione e sessioni")
    )
)]
pub struct ApiDoc;

/// Serve il documento su `GET /api/openapi.json`.
pub async fn serve() -> axum::Json<utoipa::openapi::OpenApi> {
    axum::Json(ApiDoc::openapi())
}
