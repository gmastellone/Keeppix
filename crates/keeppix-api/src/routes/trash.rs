//! Cancellazione a tre opzioni (spec §6): ogni `DELETE` porta esplicitamente
//! cosa succede al file, mai un comportamento implicito. `restore` è l'unica
//! via indietro, e solo per `moved_to_trash`.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use keeppix_db::TrashRepo;
use keeppix_domain::{AssetId, DiskAction};
use serde::Deserialize;

use crate::extract::Auth;
use crate::json::Json;
use crate::problem::Problem;
use crate::state::AppState;

#[derive(Deserialize, utoipa::ToSchema)]
pub struct DeleteAssetRequest {
    /// `kept`, `moved_to_trash`, o `purged` — nessun default: il client deve
    /// sempre scegliere (spec §6).
    #[schema(example = "moved_to_trash")]
    pub disk_action: String,
}

/// `pub(crate)`: riusata da `routes::duplicates::resolve`, che applica la
/// stessa azione a ogni membro non tenuto di un gruppo di duplicati.
pub(crate) fn parse_action(raw: &str) -> Result<DiskAction, Problem> {
    DiskAction::parse(raw).map_err(|e| {
        Problem::bad_request(
            "invalid-disk-action",
            "disk_action must be kept, moved_to_trash, or purged",
        )
        .with_detail(e.to_string())
    })
}

/// # Errors
/// `400` se `disk_action` non è una delle tre opzioni; `401` se non
/// autenticato; `403` se l'asset non è visibile al chiamante (anche
/// inesistente), o se `purged` è richiesto da chi non è owner/admin della
/// libreria; `500` se l'operazione sul filesystem che accompagna la
/// scrittura fallisce.
#[utoipa::path(
    delete,
    path = "/api/v1/assets/{id}",
    tag = "trash",
    operation_id = "assets_delete",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Id dell'asset")),
    request_body = DeleteAssetRequest,
    responses(
        (status = 204, description = "Azione applicata, riga di audit registrata"),
        (status = 400, description = "disk_action non riconosciuto", body = Problem),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Asset non visibile, o purged senza i permessi", body = Problem),
        (status = 500, description = "Errore del database o del filesystem", body = Problem)
    )
)]
pub async fn delete(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Path(id): Path<AssetId>,
    Json(body): Json<DeleteAssetRequest>,
) -> Result<StatusCode, Problem> {
    let action = parse_action(&body.disk_action)?;
    TrashRepo::new(&state.db).choose(&ctx, id, action).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// # Errors
/// `401` se non autenticato; `403` se l'asset non è visibile al chiamante;
/// `409` se l'asset non ha un cestinamento pendente, o se il percorso
/// originale è di nuovo occupato — il ripristino non sovrascrive mai.
#[utoipa::path(
    post,
    path = "/api/v1/assets/{id}/restore",
    tag = "trash",
    operation_id = "assets_restore",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Id dell'asset")),
    responses(
        (status = 204, description = "File ripristinato al percorso originale"),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Asset non visibile", body = Problem),
        (status = 409, description = "Niente da ripristinare, o percorso occupato", body = Problem),
        (status = 500, description = "Errore del database o del filesystem", body = Problem)
    )
)]
pub async fn restore(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Path(id): Path<AssetId>,
) -> Result<StatusCode, Problem> {
    TrashRepo::new(&state.db).restore(&ctx, id).await?;
    Ok(StatusCode::NO_CONTENT)
}
