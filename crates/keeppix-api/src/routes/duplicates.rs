//! Duplicati per `content_hash` (spec §7). La lista dei gruppi era già in
//! [`crate::routes::problems`] dalla Fase 1c; questo modulo la porta al posto
//! che il piano della Fase 2 le assegna e aggiunge i membri di un gruppo e
//! l'azione di risoluzione, entrambi appoggiati su [`DuplicateRepo`].

use axum::extract::{Path, State};
use axum::http::StatusCode;
use keeppix_db::DuplicateRepo;
use keeppix_domain::AssetId;
use serde::{Deserialize, Serialize};

use crate::extract::Auth;
use crate::json::Json;
use crate::problem::Problem;
use crate::routes::timeline::{AssetView, hex_bytes};
use crate::routes::trash::parse_action;
use crate::state::AppState;

#[derive(Serialize, utoipa::ToSchema)]
pub struct DuplicateGroupView {
    content_hash: String,
    count: i64,
    size_bytes: i64,
    reclaimable_bytes: i64,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct ResolveDuplicateRequest {
    /// Id dell'asset da tenere: tutti gli altri membri non cestinati del
    /// gruppo ricevono `disk_action`.
    #[schema(value_type = String)]
    pub keep: AssetId,
    /// `kept`, `moved_to_trash`, o `purged` — stesso vocabolario di
    /// `DELETE /api/v1/assets/{id}` (spec §6).
    #[schema(example = "moved_to_trash")]
    pub disk_action: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct ResolveDuplicateResponse {
    /// Quanti membri sono stati toccati (il gruppo meno `keep`).
    pub resolved: usize,
}

fn parse_hash(hex: &str) -> Result<[u8; 32], Problem> {
    if hex.len() != 64 {
        return Err(Problem::bad_request(
            "invalid-content-hash",
            "content_hash must be 64 hex characters",
        ));
    }
    let mut out = [0u8; 32];
    for (i, chunk) in hex.as_bytes().chunks(2).enumerate() {
        let s = core::str::from_utf8(chunk).map_err(|_| {
            Problem::bad_request("invalid-content-hash", "content_hash must be hex")
        })?;
        out[i] = u8::from_str_radix(s, 16).map_err(|_| {
            Problem::bad_request("invalid-content-hash", "content_hash must be hex")
        })?;
    }
    Ok(out)
}

/// # Errors
/// `401` se non autenticato.
#[utoipa::path(
    get,
    path = "/api/v1/duplicates",
    tag = "library",
    operation_id = "duplicates_list",
    summary = "List duplicate groups",
    security(("session_cookie" = [])),
    responses(
        (status = 200, description = "Gruppi con lo stesso content_hash, esclusi i trashed", body = [DuplicateGroupView]),
        (status = 401, description = "Non autenticato", body = Problem)
    )
)]
pub async fn list(
    State(state): State<AppState>,
    Auth(ctx): Auth,
) -> Result<Json<Vec<DuplicateGroupView>>, Problem> {
    let groups = DuplicateRepo::new(&state.db).groups(&ctx).await?;
    Ok(Json(
        groups
            .into_iter()
            .map(|g| DuplicateGroupView {
                content_hash: hex_bytes(&g.content_hash),
                count: g.count,
                size_bytes: g.size_bytes,
                reclaimable_bytes: g.reclaimable_bytes(),
            })
            .collect(),
    ))
}

/// # Errors
/// `400` se `content_hash` non è 64 caratteri esadecimali; `401` se non
/// autenticato.
#[utoipa::path(
    get,
    path = "/api/v1/duplicates/{content_hash}",
    tag = "library",
    operation_id = "duplicates_members",
    summary = "List members of a duplicate group",
    security(("session_cookie" = [])),
    params(("content_hash" = String, Path, description = "blake3 hex da 64 caratteri")),
    responses(
        (status = 200, description = "Membri non cestinati del gruppo", body = [AssetView]),
        (status = 400, description = "content_hash non è hex valido", body = Problem),
        (status = 401, description = "Non autenticato", body = Problem)
    )
)]
pub async fn members(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Path(content_hash): Path<String>,
) -> Result<Json<Vec<AssetView>>, Problem> {
    let hash = parse_hash(&content_hash)?;
    let assets = DuplicateRepo::new(&state.db).members(&ctx, &hash).await?;
    Ok(Json(assets.iter().map(AssetView::from_asset).collect()))
}

/// # Errors
/// `400` se `content_hash` o `disk_action` non sono validi; `401` se non
/// autenticato; `403` se `keep` non è visibile o non appartiene al gruppo, o
/// se `purged` è richiesto da chi non è owner/admin della libreria.
#[utoipa::path(
    post,
    path = "/api/v1/duplicates/{content_hash}/resolve",
    tag = "library",
    operation_id = "duplicates_resolve",
    summary = "Resolve a duplicate group",
    security(("session_cookie" = [])),
    params(("content_hash" = String, Path, description = "blake3 hex da 64 caratteri")),
    request_body = ResolveDuplicateRequest,
    responses(
        (status = 200, description = "Azione applicata agli altri membri", body = ResolveDuplicateResponse),
        (status = 400, description = "content_hash o disk_action non validi", body = Problem),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "keep non nel gruppo, o purged senza i permessi", body = Problem)
    )
)]
pub async fn resolve(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Path(content_hash): Path<String>,
    Json(body): Json<ResolveDuplicateRequest>,
) -> Result<(StatusCode, Json<ResolveDuplicateResponse>), Problem> {
    let hash = parse_hash(&content_hash)?;
    let action = parse_action(&body.disk_action)?;
    let resolved = DuplicateRepo::new(&state.db)
        .resolve(&ctx, &hash, body.keep, action)
        .await?;
    Ok((StatusCode::OK, Json(ResolveDuplicateResponse { resolved })))
}
