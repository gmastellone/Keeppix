//! Superficie HTTP sulle librerie. Nessun SQL: solo `LibraryRepo` e
//! validazione di percorso (allowlist dopo `canonicalize`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use axum::extract::{Path as AxumPath, Query, State};
use axum::http::StatusCode;
use keeppix_db::{JobRepo, LibraryRepo, OperationsRepo};
use keeppix_domain::{FolderId, JobStatus, Library, LibraryId, NewLibrary, OperationKind};
use serde::{Deserialize, Serialize};

use crate::extract::{AdminAuth, Auth};
use crate::json::Json;
use crate::problem::Problem;
use crate::state::AppState;

#[derive(Serialize, utoipa::ToSchema)]
pub struct LibraryView {
    pub id: String,
    pub name: String,
    pub owner_id: String,
    pub root_path: String,
    pub scan_enabled: bool,
    /// Interruttore del riconoscimento facciale per questa libreria (Fase 8
    /// Task 10).
    pub faces_enabled: bool,
    pub exclude_patterns: Vec<String>,
    pub status: String,
    pub last_scan_at: Option<String>,
    pub created_at: String,
    /// Radice del culling a cartelle (Fase 9 Task 2, esposta via HTTP in
    /// Fase 11 Task 17). `None` finché il proprietario non ne designa una —
    /// culling si comporta come oggi, nessun comportamento nuovo forzato.
    pub culling_root_folder_id: Option<String>,
}

impl LibraryView {
    fn from_library(lib: &Library) -> Self {
        Self {
            id: lib.id.to_string(),
            name: lib.name.clone(),
            owner_id: lib.owner_id.to_string(),
            root_path: lib.root_path.to_string_lossy().into_owned(),
            scan_enabled: lib.scan_enabled,
            faces_enabled: lib.faces_enabled,
            exclude_patterns: lib.exclude_patterns.clone(),
            culling_root_folder_id: lib.culling_root_folder_id.map(|id| id.to_string()),
            status: match lib.status {
                keeppix_domain::LibraryStatus::Active => "active".to_owned(),
                keeppix_domain::LibraryStatus::Offline => "offline".to_owned(),
            },
            last_scan_at: lib.last_scan_at.map(|t| t.to_rfc3339()),
            created_at: lib.created_at.to_rfc3339(),
        }
    }
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreateLibraryRequest {
    pub name: String,
    pub root_path: String,
    #[serde(default)]
    pub exclude_patterns: Vec<String>,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct PatchLibraryRequest {
    pub name: Option<String>,
    pub scan_enabled: Option<bool>,
    pub faces_enabled: Option<bool>,
    pub exclude_patterns: Option<Vec<String>>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct DeleteLibraryResponse {
    /// Sempre `true`: la cancellazione è solo sul database.
    pub files_untouched: bool,
}

#[derive(Deserialize, utoipa::IntoParams)]
pub struct PreviewQuery {
    pub path: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct PreviewResponse {
    pub total: u64,
    pub extensions: BTreeMap<String, u64>,
}

/// `root_path` ammesso solo sotto una radice di `library_roots`, **dopo**
/// `canonicalize` — altrimenti `/photos/../etc` passerebbe.
fn ensure_allowed_path(path: &Path, roots: &[PathBuf]) -> Result<PathBuf, Problem> {
    let canonical = path.canonicalize().map_err(|_| path_not_allowed())?;
    for root in roots {
        let Ok(root_canon) = root.canonicalize() else {
            continue;
        };
        if canonical.starts_with(&root_canon) {
            return Ok(canonical);
        }
    }
    Err(path_not_allowed())
}

fn path_not_allowed() -> Problem {
    Problem::new(
        StatusCode::UNPROCESSABLE_ENTITY,
        "path-not-allowed",
        "Library root_path is outside KEEPPIX_LIBRARY_ROOTS",
    )
}

/// # Errors
/// `401` se non autenticato.
#[utoipa::path(
    get,
    path = "/api/v1/libraries",
    tag = "libraries",
    operation_id = "libraries_list",
    summary = "List libraries",
    security(("session_cookie" = [])),
    responses(
        (status = 200, description = "Librerie visibili", body = [LibraryView]),
        (status = 401, description = "Non autenticato", body = Problem)
    )
)]
pub async fn list(
    State(state): State<AppState>,
    Auth(ctx): Auth,
) -> Result<Json<Vec<LibraryView>>, Problem> {
    let libs = LibraryRepo::new(&state.db).list(&ctx).await?;
    Ok(Json(libs.iter().map(LibraryView::from_library).collect()))
}

/// # Errors
/// `403` se non admin; `409` se `root_path` già indicizzato; `422` se fuori
/// dall'allowlist.
#[utoipa::path(
    post,
    path = "/api/v1/libraries",
    tag = "libraries",
    operation_id = "libraries_create",
    summary = "Create a library",
    security(("session_cookie" = [])),
    request_body = CreateLibraryRequest,
    responses(
        (status = 201, description = "Libreria creata", body = LibraryView),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Solo admin", body = Problem),
        (status = 409, description = "root_path già indicizzato", body = Problem),
        (status = 422, description = "Percorso non ammesso", body = Problem)
    )
)]
pub async fn create(
    State(state): State<AppState>,
    AdminAuth(ctx): AdminAuth,
    Json(body): Json<CreateLibraryRequest>,
) -> Result<(StatusCode, Json<LibraryView>), Problem> {
    let owner_id = ctx.user_id().ok_or_else(Problem::unauthenticated)?;
    let root = ensure_allowed_path(Path::new(&body.root_path), &state.library_roots)?;
    let library = LibraryRepo::new(&state.db)
        .create(
            &ctx,
            NewLibrary {
                name: body.name,
                owner_id,
                root_path: root,
                exclude_patterns: body.exclude_patterns,
            },
        )
        .await?;
    if let Some(watchers) = &state.library_watchers {
        watchers.ensure(library.id, library.root_path.clone());
    }
    Ok((
        StatusCode::CREATED,
        Json(LibraryView::from_library(&library)),
    ))
}

/// # Errors
/// `403` su id altrui o inesistente (non-admin); `404` solo admin su id
/// assente.
#[utoipa::path(
    get,
    path = "/api/v1/libraries/{id}",
    tag = "libraries",
    operation_id = "libraries_get",
    summary = "Get a library",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Id della libreria")),
    responses(
        (status = 200, description = "Libreria", body = LibraryView),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Non visibile", body = Problem),
        (status = 404, description = "Inesistente (solo admin)", body = Problem)
    )
)]
pub async fn get(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    AxumPath(id): AxumPath<LibraryId>,
) -> Result<Json<LibraryView>, Problem> {
    let library = LibraryRepo::new(&state.db).find_by_id(&ctx, id).await?;
    Ok(Json(LibraryView::from_library(&library)))
}

/// # Errors
/// Come `get`; campi assenti restano invariati.
#[utoipa::path(
    patch,
    path = "/api/v1/libraries/{id}",
    tag = "libraries",
    operation_id = "libraries_patch",
    summary = "Update a library",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Id della libreria")),
    request_body = PatchLibraryRequest,
    responses(
        (status = 200, description = "Libreria aggiornata", body = LibraryView),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Non consentito", body = Problem)
    )
)]
pub async fn patch(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    AxumPath(id): AxumPath<LibraryId>,
    Json(body): Json<PatchLibraryRequest>,
) -> Result<Json<LibraryView>, Problem> {
    let library = LibraryRepo::new(&state.db)
        .update(
            &ctx,
            id,
            body.name.as_deref(),
            body.scan_enabled,
            body.faces_enabled,
            body.exclude_patterns.as_deref(),
        )
        .await?;
    Ok(Json(LibraryView::from_library(&library)))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct PatchCullingRootRequest {
    /// Id della cartella scelta come radice, o `null` per rimuoverla.
    pub folder_id: Option<String>,
}

/// Designa (o rimuove, con `folder_id: null`) la radice del culling a
/// cartelle (§17/§64). Rotta dedicata invece di un campo su `PATCH
/// /libraries/{id}`: `LibraryRepo::set_culling_root` pretende owner/admin
/// esplicito, più stretto del permesso generale di `update` — mescolarli in
/// un solo handler renderebbe silenziosamente più permissivo un campo che
/// decide dove finiscono fisicamente i file scelti/scartati.
///
/// # Errors
/// `401` se non autenticato; `403` se il chiamante vede la libreria ma non
/// ne è proprietario/admin; `409` se `folder_id` non appartiene a questa
/// libreria; `422` se `folder_id` non è un id leggibile.
#[utoipa::path(
    patch,
    path = "/api/v1/libraries/{id}/culling-root",
    tag = "libraries",
    operation_id = "libraries_set_culling_root",
    summary = "Set or clear a library's culling root folder",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Id della libreria")),
    request_body = PatchCullingRootRequest,
    responses(
        (status = 200, description = "Libreria aggiornata", body = LibraryView),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Non owner/admin", body = Problem),
        (status = 409, description = "La cartella non appartiene a questa libreria", body = Problem),
        (status = 422, description = "Id cartella illeggibile", body = Problem)
    )
)]
pub async fn set_culling_root(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    AxumPath(id): AxumPath<LibraryId>,
    Json(body): Json<PatchCullingRootRequest>,
) -> Result<Json<LibraryView>, Problem> {
    let folder_id = body
        .folder_id
        .map(|raw| {
            raw.parse::<FolderId>().map_err(|_| {
                Problem::new(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "invalid-id",
                    "Invalid folder id",
                )
            })
        })
        .transpose()?;
    let library = LibraryRepo::new(&state.db)
        .set_culling_root(&ctx, id, folder_id)
        .await?;
    Ok(Json(LibraryView::from_library(&library)))
}

/// Cancella la riga nel database. **Non tocca i file sul disco.**
///
/// # Errors
/// `403` se non admin; `404` se l'id non esiste.
#[utoipa::path(
    delete,
    path = "/api/v1/libraries/{id}",
    tag = "libraries",
    operation_id = "libraries_delete",
    summary = "Delete a library",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Id della libreria")),
    responses(
        (status = 200, description = "Eliminata; i file restano", body = DeleteLibraryResponse),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Solo admin", body = Problem),
        (status = 404, description = "Inesistente", body = Problem)
    )
)]
pub async fn delete(
    State(state): State<AppState>,
    AdminAuth(ctx): AdminAuth,
    AxumPath(id): AxumPath<LibraryId>,
) -> Result<Json<DeleteLibraryResponse>, Problem> {
    LibraryRepo::new(&state.db).delete(&ctx, id).await?;
    Ok(Json(DeleteLibraryResponse {
        files_untouched: true,
    }))
}

/// Conteggio per estensione sotto `path`, senza creare nulla.
///
/// # Errors
/// `403` se non admin; `422` se il percorso è fuori dall'allowlist.
#[utoipa::path(
    get,
    path = "/api/v1/libraries/preview",
    tag = "libraries",
    operation_id = "libraries_preview",
    summary = "Preview a library path",
    security(("session_cookie" = [])),
    params(PreviewQuery),
    responses(
        (status = 200, description = "Conteggi per estensione", body = PreviewResponse),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Solo admin", body = Problem),
        (status = 422, description = "Percorso non ammesso", body = Problem)
    )
)]
pub async fn preview(
    State(state): State<AppState>,
    AdminAuth(_ctx): AdminAuth,
    Query(query): Query<PreviewQuery>,
) -> Result<Json<PreviewResponse>, Problem> {
    let root = ensure_allowed_path(Path::new(&query.path), &state.library_roots)?;
    let mut extensions = BTreeMap::new();
    let mut total = 0_u64;
    for walked in keeppix_media::walk::iter_entries(&root, &[]) {
        total += 1;
        let ext = std::path::Path::new(&walked.filename)
            .extension()
            .and_then(|e| e.to_str())
            .map_or_else(String::new, |e| format!(".{}", e.to_ascii_lowercase()));
        *extensions.entry(ext).or_insert(0) += 1;
    }
    Ok(Json(PreviewResponse { total, extensions }))
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct ScanAccepted {
    pub library_id: String,
    pub status: String,
    /// Presente solo quando **questa** richiesta è quella che segue
    /// davvero il job accodato (Fase 10 Task 16): se una scansione per la
    /// stessa libreria era già `pending`/`running` — accodata dal watcher o
    /// da una richiesta precedente — quella vince per via della `dedup_key`
    /// condivisa, e questa risposta non ha un'operazione propria da offrire.
    pub operation_id: Option<String>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct ScanStatusView {
    pub library_id: String,
    pub library_status: String,
    /// `idle` | `discovering` | `failed` | `offline`
    pub phase: String,
    pub asset_count: i64,
    pub job_status: Option<String>,
    pub last_error: Option<String>,
    /// Sempre `null` per ora: niente stima finché non c'è throughput misurato.
    pub eta_seconds: Option<i64>,
    pub last_scan_at: Option<String>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct LibraryStorageView {
    pub free_bytes: u64,
    pub total_bytes: u64,
}

impl From<keeppix_db::LibraryStorage> for LibraryStorageView {
    fn from(usage: keeppix_db::LibraryStorage) -> Self {
        Self {
            free_bytes: usage.free_bytes,
            total_bytes: usage.total_bytes,
        }
    }
}

/// Accoda `DiscoverLibrary` (idempotente via `dedup_key`) e, quando questa
/// richiesta è quella che ottiene davvero il job, apre un'operazione
/// tracciata (Fase 10 Task 16): avanzamento sul WebSocket, annullamento via
/// `POST /operations/{id}/cancel`.
///
/// # Errors
/// Visibilità come `get`; errori di coda → 503.
#[utoipa::path(
    post,
    path = "/api/v1/libraries/{id}/scan",
    tag = "libraries",
    operation_id = "libraries_scan_start",
    summary = "Start a library scan",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Id della libreria")),
    responses(
        (status = 202, description = "Scansione accodata", body = ScanAccepted),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Non consentito", body = Problem)
    )
)]
pub async fn start_scan(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    AxumPath(id): AxumPath<LibraryId>,
) -> Result<(StatusCode, Json<ScanAccepted>), Problem> {
    LibraryRepo::new(&state.db).find_by_id(&ctx, id).await?;

    // Non creiamo un'operazione se una scansione per questa libreria è già
    // `pending`/`running`: la stessa `dedup_key` di `enqueue_rescan` la
    // farebbe comunque collassare su quel job (Ruling Task 16, vedi
    // `enqueue_rescan_with_operation`), e crearne una che nessun job farà
    // avanzare lascerebbe un'operazione "running" per sempre.
    let already_in_flight = JobRepo::new(&state.db)
        .discover_status_for_library(id)
        .await?
        .is_some_and(|job| matches!(job.status, JobStatus::Pending | JobStatus::Running));

    let operation_id = if already_in_flight {
        None
    } else {
        let operation = OperationsRepo::new(&state.db)
            .create(&ctx, OperationKind::LibraryScan)
            .await?;
        let attached =
            keeppix_jobs::watch::enqueue_rescan_with_operation(&state.db, id, operation.id)
                .await
                .map_err(|e| {
                    tracing::error!(error = %e, "enqueue discover");
                    Problem::service_unavailable()
                })?;
        if attached {
            Some(operation.id)
        } else {
            // Raro: un'altra richiesta ha vinto la corsa fra il controllo
            // sopra e questo accodamento. La nostra operazione non sarà mai
            // fatta avanzare da nessun job: la chiudiamo qui invece di
            // lasciarla `running` all'infinito, con l'involucro parziale
            // onestamente vuoto (nulla è stato applicato da questa
            // richiesta).
            OperationsRepo::new(&state.db)
                .finish_cancelled(operation.id)
                .await?;
            None
        }
    };

    Ok((
        StatusCode::ACCEPTED,
        Json(ScanAccepted {
            library_id: id.to_string(),
            status: "accepted".to_owned(),
            operation_id: operation_id.map(|o| o.to_string()),
        }),
    ))
}

/// Stato corrente della scansione / indicizzazione della libreria.
///
/// # Errors
/// Visibilità come `get`.
#[utoipa::path(
    get,
    path = "/api/v1/libraries/{id}/scan",
    tag = "libraries",
    operation_id = "libraries_scan_status",
    summary = "Get library scan status",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Id della libreria")),
    responses(
        (status = 200, description = "Stato scansione", body = ScanStatusView),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Non consentito", body = Problem)
    )
)]
pub async fn scan_status(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    AxumPath(id): AxumPath<LibraryId>,
) -> Result<Json<ScanStatusView>, Problem> {
    let library = LibraryRepo::new(&state.db).find_by_id(&ctx, id).await?;
    let asset_count = keeppix_db::AssetRepo::new(&state.db)
        .count_in_library(id)
        .await?;
    let job = keeppix_db::JobRepo::new(&state.db)
        .discover_status_for_library(id)
        .await?;

    let library_status = match library.status {
        keeppix_domain::LibraryStatus::Active => "active",
        keeppix_domain::LibraryStatus::Offline => "offline",
    };
    let phase = scan_phase(library.status, job.as_ref().map(|j| j.status));

    Ok(Json(ScanStatusView {
        library_id: id.to_string(),
        library_status: library_status.to_owned(),
        phase: phase.to_owned(),
        asset_count,
        job_status: job.as_ref().map(|j| j.status.as_str().to_owned()),
        last_error: job.and_then(|j| j.last_error),
        eta_seconds: None,
        last_scan_at: library.last_scan_at.map(|t| t.to_rfc3339()),
    }))
}

/// `idle` | `discovering` | `failed` | `offline` — condivisa da `GET
/// .../scan` e dal poll `scan.progress` del WebSocket (Fase 10 Task 19), così
/// le due superfici non possono raccontare fasi diverse per la stessa
/// libreria.
pub(crate) const fn scan_phase(
    library_status: keeppix_domain::LibraryStatus,
    job_status: Option<keeppix_domain::JobStatus>,
) -> &'static str {
    if matches!(library_status, keeppix_domain::LibraryStatus::Offline) {
        return "offline";
    }
    match job_status {
        Some(keeppix_domain::JobStatus::Pending | keeppix_domain::JobStatus::Running) => {
            "discovering"
        }
        Some(keeppix_domain::JobStatus::Failed) => "failed",
        _ => "idle",
    }
}

/// Verifica di raggiungibilità del percorso di rete (§47 «Riprova
/// connessione»): aggiorna lo stato e restituisce la libreria come sta ora,
/// così la UI può far scomparire il problema senza un secondo giro.
///
/// # Errors
/// Visibilità come `get`.
#[utoipa::path(
    post,
    path = "/api/v1/libraries/{id}/probe",
    tag = "libraries",
    operation_id = "libraries_probe",
    summary = "Retry reaching a library's root path",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Id della libreria")),
    responses(
        (status = 200, description = "Esito della verifica", body = LibraryView),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Non visibile", body = Problem)
    )
)]
pub async fn probe(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    AxumPath(id): AxumPath<LibraryId>,
) -> Result<Json<LibraryView>, Problem> {
    let library = LibraryRepo::new(&state.db).probe(&ctx, id).await?;
    Ok(Json(LibraryView::from_library(&library)))
}

/// Spazio libero e totale sul volume della libreria. Il valore è in cache
/// breve (60 s): la sidebar lo chiede a ogni caricamento.
///
/// # Errors
/// Visibilità come `get`; `503` se `statvfs` fallisce in modo irrecuperabile.
#[utoipa::path(
    get,
    path = "/api/v1/libraries/{id}/storage",
    tag = "libraries",
    operation_id = "libraries_storage",
    summary = "Get library disk usage",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Id della libreria")),
    responses(
        (status = 200, description = "Spazio libero e totale", body = LibraryStorageView),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Non visibile", body = Problem)
    )
)]
pub async fn storage(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    AxumPath(id): AxumPath<LibraryId>,
) -> Result<Json<LibraryStorageView>, Problem> {
    let usage = LibraryRepo::new(&state.db).storage(&ctx, id).await?;
    Ok(Json(LibraryStorageView::from(usage)))
}
