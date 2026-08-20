//! Documento `OpenAPI` generato dalle annotazioni sugli handler e sui tipi:
//! nasce dal codice, quindi non può divergere dalla *forma dei dati*. Percorso
//! e metodo di ogni operazione restano però stringhe scritte a mano
//! nell'attributo `#[utoipa::path]`: a legarli alle rotte davvero montate ci
//! pensa `documented_operations_are_all_mounted` in `tests/openapi.rs`.

use utoipa::OpenApi;
use utoipa::openapi::security::{ApiKey, ApiKeyValue, SecurityScheme};

use crate::extract::SESSION_COOKIE;
use crate::routes::{
    auth, credentials, duplicates, flags, folders, geotag, libraries, map, media, metadata, places,
    preferences, problems, regions, search, sessions, setup, stacks, sync, timeline, totp, trash,
    users, video, viewport, ws,
};

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
        version = "1.0.0",
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
        totp::status,
        totp::setup,
        totp::confirm,
        totp::regenerate_recovery,
        totp::disable,
        timeline::buckets,
        timeline::geometry,
        timeline::page,
        timeline::asset,
        folders::tree,
        folders::children,
        folders::relocate,
        media::thumb,
        media::preview,
        media::full,
        media::original,
        video::playback,
        video::hls,
        video::poster,
        viewport::promote,
        search::run,
        search::suggest,
        places::reverse,
        places::suggest,
        map::clusters,
        map::tiles,
        regions::list,
        regions::download,
        regions::cancel,
        regions::delete,
        search::list_saved,
        search::create_saved,
        ws::ticket,
        ws::connect,
        sync::delta,
        problems::list,
        duplicates::list,
        duplicates::members,
        duplicates::resolve,
        trash::delete,
        trash::batch_delete,
        trash::restore,
        trash::list,
        trash::empty,
        stacks::get_members,
        stacks::set_primary,
        metadata::effective,
        metadata::apply,
        metadata::apply_batch,
        metadata::shift_taken_at,
        metadata::preview_timezones,
        metadata::apply_timezones,
        metadata::undo_batch,
        geotag::copy_location,
        geotag::import_gpx,
        flags::get,
        flags::set,
        flags::batch_set,
        libraries::list,
        libraries::create,
        libraries::get,
        libraries::patch,
        libraries::delete,
        libraries::preview,
        libraries::start_scan,
        libraries::scan_status,
        libraries::storage,
        users::list,
        users::create,
        users::patch,
        users::disable,
        users::enable,
        users::change_password,
        users::set_home,
        users::delete_home,
        preferences::get,
        preferences::patch,
        credentials::create,
        credentials::list,
        credentials::revoke,
        sessions::list,
        sessions::revoke,
        sessions::revoke_others,
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
        totp::TotpStatusView,
        totp::TotpSetupView,
        totp::TotpCodeRequest,
        totp::TotpRecoveryCodesView,
        setup::SetupStatus,
        setup::SetupRequest,
        setup::SetupResponse,
        timeline::MonthBucketView,
        timeline::TimelinePage,
        timeline::AssetView,
        folders::FolderView,
        folders::FolderChildren,
        viewport::ViewportRequest,
        search::SearchRequest,
        search::SearchPage,
        search::SuggestResponse,
        places::PlaceView,
        map::MapClusterView,
        regions::RegionView,
        regions::DownloadRegionRequest,
        search::SavedSearchRequest,
        search::SavedSearchView,
        ws::TicketResponse,
        sync::DeltaView,
        problems::ProblemsView,
        duplicates::DuplicateGroupView,
        duplicates::ResolveDuplicateRequest,
        duplicates::ResolveDuplicateResponse,
        video::PlaybackResponse,
        trash::DeleteAssetRequest,
        trash::BatchDeleteRequest,
        trash::TrashListPage,
        trash::TrashItemView,
        trash::EmptyTrashResponse,
        stacks::StackView,
        stacks::StackMemberView,
        metadata::GeoPointView,
        metadata::EffectiveMetadataView,
        metadata::MetadataPatchRequest,
        metadata::BatchApplyRequest,
        metadata::BatchShiftRequest,
        metadata::RecalculateTimezonesRequest,
        metadata::TimezoneExampleView,
        metadata::TimezonePreviewView,
        metadata::TimezoneApplyView,
        geotag::CopyLocationRequest,
        geotag::ImportGpxRequest,
        flags::AssetFlagsBody,
        flags::BatchFlagsRequest,
        crate::bulk::BulkOutcome,
        crate::bulk::BulkFailure,
        crate::bulk::FailureReason,
        libraries::LibraryView,
        libraries::CreateLibraryRequest,
        libraries::PatchLibraryRequest,
        libraries::DeleteLibraryResponse,
        libraries::PreviewResponse,
        libraries::ScanAccepted,
        libraries::ScanStatusView,
        libraries::LibraryStorageView,
        users::CreateUserRequest,
        users::PatchUserRequest,
        users::ChangePasswordRequest,
        users::SetHomeRequest,
        users::HomeView,
        preferences::UserPreferencesView,
        preferences::GridDensityView,
        preferences::NotificationPreferencesView,
        credentials::CreateAppPasswordRequest,
        credentials::AppPasswordView,
        credentials::AppPasswordCreatedView,
        sessions::SessionView,
        crate::problem::Problem,
    )),
    tags(
        (name = "setup", description = "Configurazione iniziale dell'istanza"),
        (name = "auth", description = "Autenticazione e sessioni"),
        (name = "users", description = "Gestione utenti"),
        (name = "timeline", description = "Bucket mensili e pagine keyset"),
        (name = "folders", description = "Albero delle cartelle"),
        (name = "media", description = "Miniature, preview e originali"),
        (name = "search", description = "Ricerca da AST e ricerche salvate"),
        (name = "places", description = "Geocoding locale GeoNames"),
        (name = "map", description = "Cluster geografici con visibilità applicata"),
        (name = "events", description = "WebSocket di notifica"),
        (name = "library", description = "Problemi e duplicati"),
        (name = "libraries", description = "Librerie e percorsi indicizzati"),
        (name = "trash", description = "Cancellazione a tre opzioni e ripristino"),
        (name = "metadata", description = "Metadati effettivi ed editing in blocco"),
        (name = "flags", description = "Voti di culling per utente: rating, pick, etichetta colore")
    )
)]
pub struct ApiDoc;

/// Serve il documento su `GET /api/openapi.json`.
pub async fn serve() -> axum::Json<utoipa::openapi::OpenApi> {
    axum::Json(ApiDoc::openapi())
}
