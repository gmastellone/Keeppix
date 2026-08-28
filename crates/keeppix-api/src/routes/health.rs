use axum::extract::State;
use serde::Serialize;

use crate::json::Json;
use crate::state::AppState;

#[derive(Serialize, utoipa::ToSchema)]
pub struct Health {
    status: &'static str,
    version: &'static str,
    /// `"ok"`, `"unreachable"`, or `"not checked"`.
    database: &'static str,
}

// `status` always stays "ok" regardless of `database`: the process is
// alive either way, and changing the HTTP status here would risk
// unnecessary restarts for anyone who orchestrates this route as a
// liveness probe in the future. Today this route is only manual
// diagnostics (docs/DEPLOY.md "Diagnostics", a human `curl`) — the actual
// Docker health check is a separate TCP check (`keeppix healthcheck` in
// main.rs) and does not touch this route.

#[utoipa::path(
    get,
    path = "/health",
    tag = "health",
    operation_id = "health_get",
    summary = "Liveness check",
    responses(
        (status = 200, description = "The process is responding", body = Health)
    )
)]
pub async fn get(State(state): State<AppState>) -> Json<Health> {
    let database = if state.db.ping().await.is_ok() {
        "ok"
    } else {
        "unreachable"
    };
    Json(Health {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
        database,
    })
}

/// Stateless variant for `router_without_state()` — tests that don't want
/// to depend on the database. Same body, `database` honestly declares it
/// checked nothing, instead of lying with `"ok"`.
pub async fn get_without_db() -> Json<Health> {
    Json(Health {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
        database: "not checked",
    })
}
