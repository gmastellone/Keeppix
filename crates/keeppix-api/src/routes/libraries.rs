//! HTTP surface for libraries. No SQL: only `LibraryRepo` and path
//! validation (allowlist after `canonicalize`).

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
    /// Face recognition toggle for this library.
    pub faces_enabled: bool,
    pub exclude_patterns: Vec<String>,
    pub status: String,
    pub last_scan_at: Option<String>,
    pub created_at: String,
    /// Root for folder-based culling. `None` until the owner designates
    /// one — culling behaves as before, no new behavior is forced.
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
    /// Always `true`: deletion only touches the database.
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

/// `root_path` allowed only under one of `library_roots`, **after**
/// `canonicalize` — otherwise `/photos/../etc` would pass.
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
/// `401` if not authenticated.
#[utoipa::path(
    get,
    path = "/api/v1/libraries",
    tag = "libraries",
    operation_id = "libraries_list",
    summary = "List libraries",
    security(("session_cookie" = [])),
    responses(
        (status = 200, description = "Visible libraries", body = [LibraryView]),
        (status = 401, description = "Not authenticated", body = Problem)
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
/// `403` if not admin; `409` if `root_path` is already indexed; `422` if
/// outside the allowlist.
#[utoipa::path(
    post,
    path = "/api/v1/libraries",
    tag = "libraries",
    operation_id = "libraries_create",
    summary = "Create a library",
    security(("session_cookie" = [])),
    request_body = CreateLibraryRequest,
    responses(
        (status = 201, description = "Library created", body = LibraryView),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "Admin only", body = Problem),
        (status = 409, description = "root_path already indexed", body = Problem),
        (status = 422, description = "Path not allowed", body = Problem)
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
/// `403` on another's or a nonexistent id (non-admin); `404` only for an
/// admin on a missing id.
#[utoipa::path(
    get,
    path = "/api/v1/libraries/{id}",
    tag = "libraries",
    operation_id = "libraries_get",
    summary = "Get a library",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Library id")),
    responses(
        (status = 200, description = "Library", body = LibraryView),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "Not visible", body = Problem),
        (status = 404, description = "Does not exist (admin only)", body = Problem)
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
/// Same as `get`; absent fields remain unchanged.
#[utoipa::path(
    patch,
    path = "/api/v1/libraries/{id}",
    tag = "libraries",
    operation_id = "libraries_patch",
    summary = "Update a library",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Library id")),
    request_body = PatchLibraryRequest,
    responses(
        (status = 200, description = "Library updated", body = LibraryView),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "Not allowed", body = Problem)
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
    /// Id of the folder chosen as root, or `null` to remove it.
    pub folder_id: Option<String>,
}

/// Designates (or removes, with `folder_id: null`) the folder-based
/// culling root. A dedicated route instead of a field on `PATCH
/// /libraries/{id}`: `LibraryRepo::set_culling_root` expects explicit
/// owner/admin, stricter than the general `update` permission — mixing
/// them into a single handler would silently make more permissive a field
/// that decides where picked/rejected files physically end up.
///
/// # Errors
/// `401` if not authenticated; `403` if the caller can see the library but
/// is not its owner/admin; `409` if `folder_id` does not belong to this
/// library; `422` if `folder_id` is not a readable id.
#[utoipa::path(
    patch,
    path = "/api/v1/libraries/{id}/culling-root",
    tag = "libraries",
    operation_id = "libraries_set_culling_root",
    summary = "Set or clear a library's culling root folder",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Library id")),
    request_body = PatchCullingRootRequest,
    responses(
        (status = 200, description = "Library updated", body = LibraryView),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "Not owner/admin", body = Problem),
        (status = 409, description = "The folder does not belong to this library", body = Problem),
        (status = 422, description = "Unreadable folder id", body = Problem)
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

/// Deletes the row in the database. **Does not touch files on disk.**
///
/// # Errors
/// `403` if not admin; `404` if the id does not exist.
#[utoipa::path(
    delete,
    path = "/api/v1/libraries/{id}",
    tag = "libraries",
    operation_id = "libraries_delete",
    summary = "Delete a library",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Library id")),
    responses(
        (status = 200, description = "Deleted; files remain", body = DeleteLibraryResponse),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "Admin only", body = Problem),
        (status = 404, description = "Does not exist", body = Problem)
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

/// Count by extension under `path`, without creating anything.
///
/// # Errors
/// `403` if not admin; `422` if the path is outside the allowlist.
#[utoipa::path(
    get,
    path = "/api/v1/libraries/preview",
    tag = "libraries",
    operation_id = "libraries_preview",
    summary = "Preview a library path",
    security(("session_cookie" = [])),
    params(PreviewQuery),
    responses(
        (status = 200, description = "Counts by extension", body = PreviewResponse),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "Admin only", body = Problem),
        (status = 422, description = "Path not allowed", body = Problem)
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
    /// Present only when **this** request is the one that actually
    /// follows the queued job: if a scan for the same library was already
    /// `pending`/`running` — queued by the watcher or an earlier request —
    /// that one wins via the shared `dedup_key`, and this response has no
    /// operation of its own to offer.
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
    /// Always `null` for now: no estimate until there is measured throughput.
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

/// Queues `DiscoverLibrary` (idempotent via `dedup_key`) and, when this
/// request is the one that actually gets the job, opens a tracked
/// operation: progress over the WebSocket, cancellation via
/// `POST /operations/{id}/cancel`.
///
/// # Errors
/// Visibility as `get`; queueing errors → 503.
#[utoipa::path(
    post,
    path = "/api/v1/libraries/{id}/scan",
    tag = "libraries",
    operation_id = "libraries_scan_start",
    summary = "Start a library scan",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Library id")),
    responses(
        (status = 202, description = "Scan queued", body = ScanAccepted),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "Not allowed", body = Problem)
    )
)]
pub async fn start_scan(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    AxumPath(id): AxumPath<LibraryId>,
) -> Result<(StatusCode, Json<ScanAccepted>), Problem> {
    LibraryRepo::new(&state.db).find_by_id(&ctx, id).await?;

    // We don't create an operation if a scan for this library is already
    // `pending`/`running`: `enqueue_rescan`'s same `dedup_key` would
    // collapse it onto that job anyway (see
    // `enqueue_rescan_with_operation`), and creating one that no job will
    // ever advance would leave a "running" operation forever.
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
            // Rare: another request won the race between the check above
            // and this enqueue. Our operation will never be advanced by
            // any job: we close it here instead of leaving it `running`
            // forever, with the partial-outcome wrapper honestly empty
            // (nothing was applied by this request).
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

/// Current status of the library scan / indexing.
///
/// # Errors
/// Visibility as `get`.
#[utoipa::path(
    get,
    path = "/api/v1/libraries/{id}/scan",
    tag = "libraries",
    operation_id = "libraries_scan_status",
    summary = "Get library scan status",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Library id")),
    responses(
        (status = 200, description = "Scan status", body = ScanStatusView),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "Not allowed", body = Problem)
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

/// `idle` | `discovering` | `failed` | `offline` — shared by `GET
/// .../scan` and the WebSocket `scan.progress` poll, so the two surfaces
/// can never report different phases for the same library.
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

/// Reachability check for the network path ("Retry connection"): updates
/// the status and returns the library as it stands now, so the UI can
/// dismiss the problem without a second round trip.
///
/// # Errors
/// Visibility as `get`.
#[utoipa::path(
    post,
    path = "/api/v1/libraries/{id}/probe",
    tag = "libraries",
    operation_id = "libraries_probe",
    summary = "Retry reaching a library's root path",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Library id")),
    responses(
        (status = 200, description = "Check outcome", body = LibraryView),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "Not visible", body = Problem)
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

/// Free and total space on the library's volume. The value is cached
/// briefly (60 s): the sidebar requests it on every load.
///
/// # Errors
/// Visibility as `get`; `503` if `statvfs` fails unrecoverably.
#[utoipa::path(
    get,
    path = "/api/v1/libraries/{id}/storage",
    tag = "libraries",
    operation_id = "libraries_storage",
    summary = "Get library disk usage",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Library id")),
    responses(
        (status = 200, description = "Free and total space", body = LibraryStorageView),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "Not visible", body = Problem)
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
