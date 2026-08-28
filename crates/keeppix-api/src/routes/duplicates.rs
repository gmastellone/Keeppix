//! Duplicates by `content_hash`. The list of groups already lived in
//! [`crate::routes::problems`]; this module moves it to its proper home and
//! adds a group's members and the resolution action, both backed by
//! [`DuplicateRepo`].

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
    /// Id of the asset to keep: all other non-trashed members of the group
    /// receive `disk_action`.
    #[schema(value_type = String)]
    pub keep: AssetId,
    /// `kept`, `moved_to_trash`, or `purged` — same vocabulary as
    /// `DELETE /api/v1/assets/{id}`.
    #[schema(example = "moved_to_trash")]
    pub disk_action: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct ResolveDuplicateResponse {
    /// How many members were touched (the group minus `keep`).
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
/// `401` if not authenticated.
#[utoipa::path(
    get,
    path = "/api/v1/duplicates",
    tag = "library",
    operation_id = "duplicates_list",
    summary = "List duplicate groups",
    security(("session_cookie" = [])),
    responses(
        (status = 200, description = "Groups sharing content_hash, excluding trashed", body = [DuplicateGroupView]),
        (status = 401, description = "Not authenticated", body = Problem)
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
/// `400` if `content_hash` is not 64 hex characters; `401` if not
/// authenticated.
#[utoipa::path(
    get,
    path = "/api/v1/duplicates/{content_hash}",
    tag = "library",
    operation_id = "duplicates_members",
    summary = "List members of a duplicate group",
    security(("session_cookie" = [])),
    params(("content_hash" = String, Path, description = "blake3 hex, 64 characters")),
    responses(
        (status = 200, description = "Non-trashed members of the group", body = [AssetView]),
        (status = 400, description = "content_hash is not valid hex", body = Problem),
        (status = 401, description = "Not authenticated", body = Problem)
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
/// `400` if `content_hash` or `disk_action` are invalid; `401` if not
/// authenticated; `403` if `keep` is not visible or does not belong to the
/// group, or if `purged` is requested by someone who is not owner/admin of
/// the library.
#[utoipa::path(
    post,
    path = "/api/v1/duplicates/{content_hash}/resolve",
    tag = "library",
    operation_id = "duplicates_resolve",
    summary = "Resolve a duplicate group",
    security(("session_cookie" = [])),
    params(("content_hash" = String, Path, description = "blake3 hex, 64 characters")),
    request_body = ResolveDuplicateRequest,
    responses(
        (status = 200, description = "Action applied to the other members", body = ResolveDuplicateResponse),
        (status = 400, description = "content_hash or disk_action invalid", body = Problem),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "keep not in the group, or purged without permission", body = Problem)
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
