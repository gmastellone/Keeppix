//! Faces: photo details panel, review queue. Person/group CRUD lives in
//! `routes::persons`.
#![allow(clippy::missing_errors_doc)]

use axum::extract::{Path, State};
use axum::http::StatusCode;
use keeppix_db::FaceRepo;
use keeppix_domain::{AssetId, Face, FaceId, PersonId};
use serde::{Deserialize, Serialize};

use crate::bulk::FaceBulkOutcome;
use crate::extract::Auth;
use crate::json::Json;
use crate::problem::Problem;
use crate::state::AppState;

#[derive(Serialize, utoipa::ToSchema)]
pub struct FaceBBoxView {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct FaceView {
    pub id: String,
    pub asset_id: String,
    pub bbox: FaceBBoxView,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub person_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proposed_person_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proposed_score: Option<f32>,
    pub assigned_by_human: bool,
}

impl From<&Face> for FaceView {
    fn from(f: &Face) -> Self {
        Self {
            id: f.id.to_string(),
            asset_id: f.asset_id.to_string(),
            bbox: FaceBBoxView {
                x: f.bbox.x,
                y: f.bbox.y,
                w: f.bbox.w,
                h: f.bbox.h,
            },
            person_id: f.person_id.map(|id| id.to_string()),
            proposed_person_id: f.proposed_person_id.map(|id| id.to_string()),
            proposed_score: f.proposed_score,
            assigned_by_human: f.is_human_assigned(),
        }
    }
}

/// # Errors
/// `403` if the asset is not visible to the caller.
#[utoipa::path(
    get,
    path = "/api/v1/assets/{id}/faces",
    tag = "faces",
    operation_id = "assets_list_faces",
    summary = "List detected faces on an asset",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Asset id")),
    responses(
        (status = 200, description = "Detected faces (excluding rejected false positives)", body = Vec<FaceView>),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "Asset not visible to the caller", body = Problem)
    )
)]
pub async fn list_for_asset(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Path(id): Path<AssetId>,
) -> Result<Json<Vec<FaceView>>, Problem> {
    let faces = FaceRepo::new(&state.db).list_for_asset(&ctx, id).await?;
    Ok(Json(faces.iter().map(FaceView::from).collect()))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct AssignFaceRequest {
    #[schema(value_type = String)]
    pub person_id: PersonId,
}

/// Manual assignment, from the photo details panel or the review queue —
/// also covers "new person": the client first creates the person
/// (`POST /persons`), then assigns the face to it.
///
/// # Errors
/// `403` if the face's asset is not visible to the caller.
#[utoipa::path(
    post,
    path = "/api/v1/faces/{id}/assign",
    tag = "faces",
    operation_id = "faces_assign",
    summary = "Manually assign a face to a person",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Face id")),
    request_body = AssignFaceRequest,
    responses(
        (status = 204, description = "Assigned"),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "Asset not visible to the caller", body = Problem),
        (status = 404, description = "Face does not exist", body = Problem)
    )
)]
pub async fn assign(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Path(id): Path<FaceId>,
    Json(body): Json<AssignFaceRequest>,
) -> Result<StatusCode, Problem> {
    FaceRepo::new(&state.db)
        .assign(&ctx, id, body.person_id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// "Not a face": permanent false positive.
///
/// # Errors
/// `403` if the face's asset is not visible to the caller.
#[utoipa::path(
    post,
    path = "/api/v1/faces/{id}/reject",
    tag = "faces",
    operation_id = "faces_reject",
    summary = "Mark a detection as not a face (permanent)",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Face id")),
    responses(
        (status = 204, description = "Rejected"),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "Asset not visible to the caller", body = Problem),
        (status = 404, description = "Face does not exist", body = Problem)
    )
)]
pub async fn reject(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Path(id): Path<FaceId>,
) -> Result<StatusCode, Problem> {
    FaceRepo::new(&state.db).reject(&ctx, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Review queue: faces with a candidate person (uncertain distance from the
/// centroid) — "These faces look like [person]".
///
/// # Errors
/// `401` without an authenticated user.
#[utoipa::path(
    get,
    path = "/api/v1/faces/proposals",
    tag = "faces",
    operation_id = "faces_list_proposals",
    summary = "List faces pending review (proposed to a candidate person)",
    security(("session_cookie" = [])),
    responses(
        (status = 200, description = "Proposed faces, visible to the caller", body = Vec<FaceView>),
        (status = 401, description = "Not authenticated", body = Problem)
    )
)]
pub async fn list_proposals(
    State(state): State<AppState>,
    Auth(ctx): Auth,
) -> Result<Json<Vec<FaceView>>, Problem> {
    let faces = FaceRepo::new(&state.db).list_proposed(&ctx).await?;
    Ok(Json(faces.iter().map(FaceView::from).collect()))
}

/// Confirms a proposal: assigns the face to the candidate person.
///
/// # Errors
/// `403` if the asset is not visible; `409` if there is no (longer a)
/// pending proposal.
#[utoipa::path(
    post,
    path = "/api/v1/faces/{id}/confirm",
    tag = "faces",
    operation_id = "faces_confirm_proposal",
    summary = "Confirm a face's candidate person",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Face id")),
    responses(
        (status = 204, description = "Confirmed"),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "Asset not visible to the caller", body = Problem),
        (status = 409, description = "No pending proposal for this face", body = Problem)
    )
)]
pub async fn confirm_proposal(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Path(id): Path<FaceId>,
) -> Result<StatusCode, Problem> {
    FaceRepo::new(&state.db).confirm_proposal(&ctx, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// "Confirm all" for a candidate person (bulk action, partial-success
/// wrapper).
///
/// # Errors
/// `401` without an authenticated user.
#[utoipa::path(
    post,
    path = "/api/v1/persons/{id}/proposals/confirm",
    tag = "faces",
    operation_id = "persons_confirm_all_proposals",
    summary = "Confirm all pending proposals for a candidate person (bulk)",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Candidate person id")),
    responses(
        (status = 200, description = "Outcome", body = FaceBulkOutcome),
        (status = 401, description = "Not authenticated", body = Problem)
    )
)]
pub async fn confirm_all_proposals(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Path(id): Path<PersonId>,
) -> Result<Json<FaceBulkOutcome>, Problem> {
    let confirmed = FaceRepo::new(&state.db)
        .confirm_all_proposed_for_person(&ctx, id)
        .await?;
    Ok(Json(FaceBulkOutcome::from_partition(confirmed, &[])))
}

/// Like [`confirm_all_proposals`], but rejects — "reject all".
///
/// # Errors
/// `401` without an authenticated user.
#[utoipa::path(
    post,
    path = "/api/v1/persons/{id}/proposals/reject",
    tag = "faces",
    operation_id = "persons_reject_all_proposals",
    summary = "Reject all pending proposals for a candidate person (bulk)",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Candidate person id")),
    responses(
        (status = 200, description = "Outcome", body = FaceBulkOutcome),
        (status = 401, description = "Not authenticated", body = Problem)
    )
)]
pub async fn reject_all_proposals(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Path(id): Path<PersonId>,
) -> Result<Json<FaceBulkOutcome>, Problem> {
    let rejected = FaceRepo::new(&state.db)
        .reject_all_proposed_for_person(&ctx, id)
        .await?;
    Ok(Json(FaceBulkOutcome::from_partition(rejected, &[])))
}

/// "Delete all face data": distinct from the per-library toggle
/// (`PATCH /libraries/{id}`, `faces_enabled`), which stops computing but
/// keeps what has already been collected. Deletes `faces` (embeddings
/// included), `persons`, `person_groups` — **global**, not per-library:
/// person clusters were never scoped by library, so there is no library
/// boundary for this action.
///
/// # Errors
/// `403` for non-administrators.
#[utoipa::path(
    delete,
    path = "/api/v1/faces/data",
    tag = "faces",
    operation_id = "faces_delete_all_data",
    summary = "Delete all face/person data system-wide (admin only, irreversible)",
    security(("session_cookie" = [])),
    responses(
        (status = 204, description = "Deleted"),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "Administrators only", body = Problem)
    )
)]
pub async fn delete_all_data(
    State(state): State<AppState>,
    Auth(ctx): Auth,
) -> Result<StatusCode, Problem> {
    FaceRepo::new(&state.db).delete_all_data(&ctx).await?;
    Ok(StatusCode::NO_CONTENT)
}
