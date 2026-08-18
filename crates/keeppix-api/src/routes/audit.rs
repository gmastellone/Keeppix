//! Audit log (append-only). Admin read access.
#![allow(clippy::missing_errors_doc)]

use axum::extract::{Query, State};
use keeppix_db::AuditRepo;
use serde::Deserialize;

use crate::extract::AdminAuth;
use crate::json::Json;
use crate::problem::Problem;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct ListQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

const fn default_limit() -> i64 {
    100
}

pub async fn list(
    State(state): State<AppState>,
    AdminAuth(ctx): AdminAuth,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<keeppix_db::AuditEntry>>, Problem> {
    let limit = q.limit.clamp(1, 500);
    let offset = q.offset.max(0);
    let rows = AuditRepo::new(&state.db).list(&ctx, limit, offset).await?;
    Ok(Json(rows))
}
