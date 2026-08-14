//! Documento `OpenAPI` generato dalle annotazioni sugli handler e sui tipi:
//! nasce dal codice, quindi non può divergere dalla *forma dei dati*. Percorso
//! e metodo di ogni operazione restano però stringhe scritte a mano
//! nell'attributo `#[utoipa::path]`: a legarli alle rotte davvero montate ci
//! pensa `documented_operations_are_all_mounted` in `tests/openapi.rs`.

use utoipa::OpenApi;
use utoipa::openapi::security::{ApiKey, ApiKeyValue, SecurityScheme};

use crate::extract::SESSION_COOKIE;
use crate::routes::{auth, folders, media, setup, timeline, viewport};

/// Nome dello schema di sicurezza nel documento. Gli attributi
/// `#[utoipa::path(security(("session_cookie" = [])))]` devono ripeterlo come
/// letterale — le macro non accettano una costante — quindi
/// `security_requirements_name_a_declared_scheme` in `tests/openapi.rs`
/// verifica che le due scritture non divergano.
pub const SESSION_SCHEME: &str = "session_cookie";

/// Descrive l'autenticazione a cookie. Il nome del cookie non è riscritto a
/// mano: viene da `SESSION_COOKIE`, la stessa costante che l'extractor legge.
struct SecurityAddon;

impl utoipa::Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.get_or_insert_default();
        components.add_security_scheme(
            SESSION_SCHEME,
            SecurityScheme::ApiKey(ApiKey::Cookie(ApiKeyValue::with_description(
                SESSION_COOKIE,
                "Cookie di sessione emesso da POST /api/v1/setup e POST /api/v1/auth/login.",
            ))),
        );
    }
}

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Keeppix API",
        version = env!("CARGO_PKG_VERSION"),
        description = "API di Keeppix. Contratto congelato: solo aggiunte entro /api/v1."
    ),
    modifiers(&SecurityAddon),
    paths(
        setup::status,
        setup::create,
        auth::login,
        auth::refresh,
        auth::logout,
        auth::me,
        timeline::buckets,
        timeline::page,
        folders::tree,
        folders::children,
        media::thumb,
        media::preview,
        media::original,
        viewport::promote,
    ),
    // Elenco ridondante: utoipa raccoglie da sé gli schemi referenziati dalle
    // operazioni (verificato — togliendo una voce il documento non cambia di un
    // byte). Vale come indice leggibile dei tipi pubblici, non come
    // configurazione: aggiungere qui un tipo che nessuna operazione referenzia
    // non lo fa comparire nel documento.
    components(schemas(
        auth::UserView,
        auth::LoginRequest,
        auth::LoginResponse,
        auth::MeResponse,
        setup::SetupStatus,
        setup::SetupRequest,
        setup::SetupResponse,
        timeline::MonthBucketView,
        timeline::TimelinePage,
        timeline::AssetView,
        folders::FolderView,
        folders::FolderChildren,
        viewport::ViewportRequest,
        crate::problem::Problem,
    )),
    tags(
        (name = "setup", description = "Configurazione iniziale dell'istanza"),
        (name = "auth", description = "Autenticazione e sessioni"),
        (name = "timeline", description = "Bucket mensili e pagine keyset"),
        (name = "folders", description = "Albero delle cartelle"),
        (name = "media", description = "Miniature, preview e originali")
    )
)]
pub struct ApiDoc;

/// Serve il documento su `GET /api/openapi.json`.
pub async fn serve() -> axum::Json<utoipa::openapi::OpenApi> {
    axum::Json(ApiDoc::openapi())
}
