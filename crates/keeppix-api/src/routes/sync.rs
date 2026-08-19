use axum::extract::{Query, State};
use keeppix_db::ChangeLogRepo;
use keeppix_domain::AssetId;
use serde::{Deserialize, Serialize};

use crate::extract::Auth;
use crate::json::Json;
use crate::problem::Problem;
use crate::state::AppState;

#[derive(Deserialize, utoipa::IntoParams)]
pub struct DeltaQuery {
    /// Last `seq` delivered to the client. Omit or `0` for the first page.
    pub cursor: Option<i64>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct DeltaView {
    pub cursor: i64,
    pub upserted: Vec<String>,
    pub deleted: Vec<String>,
    pub has_more: bool,
}

/// Incremental asset changes since `cursor`, with a safe high-water mark.
///
/// Reuses [`ChangeLogRepo::since`] — the same cursor safety as the WebSocket
/// channel, exposed for mobile/offline clients.
#[utoipa::path(
    get,
    path = "/api/v1/sync/delta",
    params(DeltaQuery),
    security(("session_cookie" = [])),
    responses(
        (status = 200, description = "Change page", body = DeltaView),
        (status = 401, description = "Not authenticated", body = Problem),
    )
)]
pub async fn delta(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Query(query): Query<DeltaQuery>,
) -> Result<Json<DeltaView>, Problem> {
    let cursor = query.cursor.unwrap_or(0);
    let page = ChangeLogRepo::new(&state.db)
        .since(&ctx, cursor)
        .await
        .map_err(Problem::from)?;

    Ok(Json(DeltaView {
        cursor: page.cursor,
        upserted: page.upserted.iter().map(id_str).collect(),
        deleted: page.deleted.iter().map(id_str).collect(),
        has_more: page.has_more,
    }))
}

fn id_str(id: &AssetId) -> String {
    id.to_string()
}
