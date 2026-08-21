//! Audit log (append-only). Admin read access.
#![allow(clippy::missing_errors_doc)]

use axum::extract::{Query, State};
use chrono::{DateTime, Utc};
use keeppix_db::AuditRepo;
use serde::{Deserialize, Serialize};

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

/// Voce del registro audit, con lo stesso schema di
/// `keeppix_db::AuditEntry` — solo lo schema `utoipa` sta nella crate HTTP
/// invece che in `keeppix-db`, che non deve conoscere `utoipa`.
#[derive(Serialize, utoipa::ToSchema)]
pub struct AuditEntryView {
    pub id: i64,
    pub actor_id: Option<String>,
    pub actor_kind: String,
    pub action: String,
    pub object_type: Option<String>,
    pub object_id: Option<String>,
    #[schema(value_type = Option<Object>)]
    pub detail: Option<serde_json::Value>,
    pub ip: Option<String>,
    pub at: DateTime<Utc>,
}

impl From<keeppix_db::AuditEntry> for AuditEntryView {
    fn from(e: keeppix_db::AuditEntry) -> Self {
        Self {
            id: e.id,
            actor_id: e.actor_id.map(|id| id.to_string()),
            actor_kind: e.actor_kind,
            action: e.action,
            object_type: e.object_type,
            object_id: e.object_id.map(|id| id.to_string()),
            detail: e.detail,
            ip: e.ip,
            at: e.at,
        }
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/audit",
    tag = "audit",
    operation_id = "audit_list",
    summary = "List audit log entries",
    security(("session_cookie" = [])),
    params(
        ("limit" = Option<i64>, Query, description = "Max risultati (1..=500, default 100)"),
        ("offset" = Option<i64>, Query, description = "Da saltare, per pagina successiva")
    ),
    responses(
        (status = 200, description = "Voci più recenti prima", body = Vec<AuditEntryView>),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Solo admin", body = Problem)
    )
)]
pub async fn list(
    State(state): State<AppState>,
    AdminAuth(ctx): AdminAuth,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<AuditEntryView>>, Problem> {
    let limit = q.limit.clamp(1, 500);
    let offset = q.offset.max(0);
    let rows = AuditRepo::new(&state.db).list(&ctx, limit, offset).await?;
    Ok(Json(rows.into_iter().map(AuditEntryView::from).collect()))
}
