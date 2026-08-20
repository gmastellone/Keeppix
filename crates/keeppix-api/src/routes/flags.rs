//! Voti di culling per utente (spec §4.1): rating, pick/reject, etichetta
//! colore. Tutta la logica di visibilità e di "per utente" vive in
//! [`FlagRepo`]; qui solo la traduzione da/verso JSON e la validazione del
//! rating fuori range, che qui diventa un `400` invece di propagare
//! `DomainError` fino al database.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use keeppix_db::FlagRepo;
use keeppix_domain::{AssetFlags, AssetId, Pick, Rating};
use serde::{Deserialize, Serialize};

use crate::extract::Auth;
use crate::json::Json;
use crate::problem::Problem;
use crate::state::AppState;

#[derive(Debug, Clone, Default, Serialize, Deserialize, utoipa::ToSchema)]
pub struct AssetFlagsBody {
    /// 0..=5. `null` (o assente in scrittura) significa "nessun voto".
    #[schema(example = 4, minimum = 0, maximum = 5)]
    pub rating: Option<u8>,
    #[serde(default)]
    #[schema(value_type = String, example = "pick")]
    pub pick: Pick,
    pub color_label: Option<String>,
}

impl AssetFlagsBody {
    fn into_domain(self) -> Result<AssetFlags, Problem> {
        let rating = self.rating.map(Rating::parse).transpose().map_err(|e| {
            Problem::bad_request("invalid-rating", "rating must be between 0 and 5")
                .with_detail(e.to_string())
        })?;
        Ok(AssetFlags {
            rating,
            pick: self.pick,
            color_label: self.color_label,
        })
    }

    fn from_domain(flags: AssetFlags) -> Self {
        Self {
            rating: flags.rating.map(Rating::value),
            pick: flags.pick,
            color_label: flags.color_label,
        }
    }
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct BatchFlagsRequest {
    #[schema(value_type = Vec<String>)]
    pub asset_ids: Vec<AssetId>,
    #[serde(flatten)]
    pub flags: AssetFlagsBody,
}

/// # Errors
/// `401` se non autenticato; `403` se l'asset non è visibile.
#[utoipa::path(
    get,
    path = "/api/v1/assets/{id}/flags",
    tag = "flags",
    operation_id = "flags_get",
    summary = "Get asset flags",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Id dell'asset")),
    responses(
        (status = 200, description = "Flag del chiamante su questo asset, o i default se non ha votato", body = AssetFlagsBody),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Asset non visibile", body = Problem)
    )
)]
pub async fn get(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Path(id): Path<AssetId>,
) -> Result<Json<AssetFlagsBody>, Problem> {
    let flags = FlagRepo::new(&state.db).get(&ctx, id).await?;
    Ok(Json(AssetFlagsBody::from_domain(flags)))
}

/// # Errors
/// `400` se `rating` supera 5; `401` se non autenticato; `403` se l'asset non
/// è visibile.
#[utoipa::path(
    put,
    path = "/api/v1/assets/{id}/flags",
    tag = "flags",
    operation_id = "flags_set",
    summary = "Set asset flags",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Id dell'asset")),
    request_body = AssetFlagsBody,
    responses(
        (status = 204, description = "Flag del chiamante aggiornati"),
        (status = 400, description = "rating fuori range", body = Problem),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Asset non visibile", body = Problem)
    )
)]
pub async fn set(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Path(id): Path<AssetId>,
    Json(body): Json<AssetFlagsBody>,
) -> Result<StatusCode, Problem> {
    let flags = body.into_domain()?;
    FlagRepo::new(&state.db).set(&ctx, id, &flags).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// # Errors
/// `400` se `rating` supera 5; `401` se non autenticato; `403` se anche un
/// solo asset non è visibile.
#[utoipa::path(
    post,
    path = "/api/v1/flags/batch",
    tag = "flags",
    operation_id = "flags_batch_set",
    summary = "Set flags on multiple assets",
    security(("session_cookie" = [])),
    request_body = BatchFlagsRequest,
    responses(
        (status = 204, description = "Stessi flag applicati a ogni asset"),
        (status = 400, description = "rating fuori range o batch troppo grande", body = Problem),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Un asset non è visibile", body = Problem)
    )
)]
pub async fn batch_set(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Json(body): Json<BatchFlagsRequest>,
) -> Result<StatusCode, Problem> {
    crate::batch::reject_oversized_batch(&body.asset_ids)?;
    let flags = body.flags.into_domain()?;
    FlagRepo::new(&state.db)
        .batch_set(&ctx, &body.asset_ids, &flags)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
