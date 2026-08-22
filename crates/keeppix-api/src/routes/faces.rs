//! Volti: pannello dettagli foto (Fase 8 Task 6), coda di revisione
//! (Task 8). CRUD di persone/gruppi in `routes::persons`.
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
/// `403` se l'asset non è visibile al chiamante.
#[utoipa::path(
    get,
    path = "/api/v1/assets/{id}/faces",
    tag = "faces",
    operation_id = "assets_list_faces",
    summary = "List detected faces on an asset",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Id asset")),
    responses(
        (status = 200, description = "Volti rilevati (esclusi i falsi positivi rifiutati)", body = Vec<FaceView>),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Asset non visibile al chiamante", body = Problem)
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

/// Assegnazione manuale, dal pannello dettagli foto o dalla coda di
/// revisione — copre anche «nuova persona»: il client crea prima la persona
/// (`POST /persons`), poi assegna il volto a quella.
///
/// # Errors
/// `403` se l'asset del volto non è visibile al chiamante.
#[utoipa::path(
    post,
    path = "/api/v1/faces/{id}/assign",
    tag = "faces",
    operation_id = "faces_assign",
    summary = "Manually assign a face to a person",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Id volto")),
    request_body = AssignFaceRequest,
    responses(
        (status = 204, description = "Assegnato"),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Asset non visibile al chiamante", body = Problem),
        (status = 404, description = "Volto inesistente", body = Problem)
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

/// «Non è un volto»: falso positivo permanente (spec §4.2).
///
/// # Errors
/// `403` se l'asset del volto non è visibile al chiamante.
#[utoipa::path(
    post,
    path = "/api/v1/faces/{id}/reject",
    tag = "faces",
    operation_id = "faces_reject",
    summary = "Mark a detection as not a face (permanent)",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Id volto")),
    responses(
        (status = 204, description = "Rifiutato"),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Asset non visibile al chiamante", body = Problem),
        (status = 404, description = "Volto inesistente", body = Problem)
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

/// Coda di revisione: volti con una persona candidata (distanza dubbia dal
/// centroide, spec §4.1) — «Questi volti sembrano Giovanni».
///
/// # Errors
/// `401` senza utente autenticato.
#[utoipa::path(
    get,
    path = "/api/v1/faces/proposals",
    tag = "faces",
    operation_id = "faces_list_proposals",
    summary = "List faces pending review (proposed to a candidate person)",
    security(("session_cookie" = [])),
    responses(
        (status = 200, description = "Volti proposti, visibili al chiamante", body = Vec<FaceView>),
        (status = 401, description = "Non autenticato", body = Problem)
    )
)]
pub async fn list_proposals(
    State(state): State<AppState>,
    Auth(ctx): Auth,
) -> Result<Json<Vec<FaceView>>, Problem> {
    let faces = FaceRepo::new(&state.db).list_proposed(&ctx).await?;
    Ok(Json(faces.iter().map(FaceView::from).collect()))
}

/// Conferma una proposta: assegna il volto alla persona candidata.
///
/// # Errors
/// `403` se l'asset non è visibile; `409` se non c'è (più) una proposta in attesa.
#[utoipa::path(
    post,
    path = "/api/v1/faces/{id}/confirm",
    tag = "faces",
    operation_id = "faces_confirm_proposal",
    summary = "Confirm a face's candidate person",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Id volto")),
    responses(
        (status = 204, description = "Confermato"),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Asset non visibile al chiamante", body = Problem),
        (status = 409, description = "Nessuna proposta in attesa per questo volto", body = Problem)
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

/// «Conferma tutti» per una persona candidata (azione in blocco, involucro
/// di riuscita parziale — Task 8).
///
/// # Errors
/// `401` senza utente autenticato.
#[utoipa::path(
    post,
    path = "/api/v1/persons/{id}/proposals/confirm",
    tag = "faces",
    operation_id = "persons_confirm_all_proposals",
    summary = "Confirm all pending proposals for a candidate person (bulk)",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Id persona candidata")),
    responses(
        (status = 200, description = "Esito", body = FaceBulkOutcome),
        (status = 401, description = "Non autenticato", body = Problem)
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

/// Come [`confirm_all_proposals`], ma rifiuta — «rifiuta tutti».
///
/// # Errors
/// `401` senza utente autenticato.
#[utoipa::path(
    post,
    path = "/api/v1/persons/{id}/proposals/reject",
    tag = "faces",
    operation_id = "persons_reject_all_proposals",
    summary = "Reject all pending proposals for a candidate person (bulk)",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Id persona candidata")),
    responses(
        (status = 200, description = "Esito", body = FaceBulkOutcome),
        (status = 401, description = "Non autenticato", body = Problem)
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
