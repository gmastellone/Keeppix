use axum::extract::State;
use serde::Serialize;

use crate::json::Json;
use crate::state::AppState;

#[derive(Serialize, utoipa::ToSchema)]
pub struct Health {
    status: &'static str,
    version: &'static str,
    /// `"ok"`, `"unreachable"`, o `"not checked"`.
    database: &'static str,
}

// `status` resta sempre "ok" a prescindere da `database`: il processo è
// vivo comunque, cambiare l'HTTP status qui rischierebbe riavvii inutili da
// parte di chi orchestrasse questa rotta come liveness probe in futuro.
// Questa rotta oggi è solo diagnostica manuale (docs/DEPLOY.md "Diagnosi",
// un `curl` umano) — l'health-check Docker reale è un controllo TCP
// separato (`keeppix healthcheck` in main.rs), non tocca questa rotta.
// `database` chiude il debito `Db::ping` (scripts/wired-exceptions.txt,
// mai consumato dalla Fase 0).

#[utoipa::path(
    get,
    path = "/health",
    tag = "health",
    operation_id = "health_get",
    summary = "Liveness check",
    responses(
        (status = 200, description = "Il processo risponde", body = Health)
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

/// Variante senza stato per `router_without_state()` — test che non vogliono
/// dipendere dal database. Stesso corpo, `database` dichiara onestamente di
/// non aver controllato nulla, invece di mentire con `"ok"`.
pub async fn get_without_db() -> Json<Health> {
    Json(Health {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
        database: "not checked",
    })
}
