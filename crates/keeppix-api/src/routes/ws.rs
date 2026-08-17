use std::collections::VecDeque;
use std::time::{Duration, Instant};

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{FromRequestParts, State};
use axum::http::request::Parts;
use axum::http::{HeaderMap, header};
use axum::response::Response;
use keeppix_db::{ChangeLogRepo, Db};
use keeppix_domain::AuthContext;
use serde::Serialize;
use serde_json::json;

use crate::extract::Auth;
use crate::json::Json;
use crate::problem::Problem;
use crate::state::AppState;

pub const QUEUE_CAP: usize = 256;
const PROTOCOL: &str = "keeppix.v1";

/// Consuma il ticket **prima** dell'upgrade: un handshake malformato non
/// deve lasciare il ticket riusabile.
pub struct TicketHandshake {
    ctx: AuthContext,
}

impl FromRequestParts<AppState> for TicketHandshake {
    type Rejection = Problem;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let origin = parts
            .headers
            .get(header::ORIGIN)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(Problem::forbidden)?;
        let host = parts
            .headers
            .get(header::HOST)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if !origin_allowed(origin, host, &state.allowed_origins) {
            return Err(Problem::forbidden());
        }
        let ticket = ticket_from_protocol(&parts.headers).ok_or_else(Problem::forbidden)?;
        let ctx = state
            .tickets
            .consume(&ticket)
            .ok_or_else(Problem::forbidden)?;
        Ok(Self { ctx })
    }
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct TicketResponse {
    ticket: String,
    expires_in: u32,
}

/// # Errors
/// `401` se non autenticato.
#[utoipa::path(
    post,
    path = "/api/v1/ws/ticket",
    tag = "events",
    operation_id = "ws_ticket",
    security(("session_cookie" = [])),
    responses(
        (status = 200, description = "Ticket monouso da 30 s", body = TicketResponse),
        (status = 401, description = "Non autenticato", body = Problem)
    )
)]
pub async fn ticket(State(state): State<AppState>, Auth(ctx): Auth) -> Json<TicketResponse> {
    Json(TicketResponse {
        ticket: state.tickets.issue(ctx),
        expires_in: 30,
    })
}

/// # Errors
/// `403` se Origin non è ammesso o il ticket manca, è scaduto o è già stato usato.
#[utoipa::path(
    get,
    path = "/api/v1/ws",
    tag = "events",
    operation_id = "ws_connect",
    security(("session_cookie" = [])),
    responses(
        (status = 101, description = "WebSocket keeppix.v1"),
        (status = 403, description = "Origin o ticket non validi", body = Problem)
    )
)]
pub async fn connect(
    State(state): State<AppState>,
    handshake: TicketHandshake,
    ws: WebSocketUpgrade,
) -> Response {
    ws.protocols([PROTOCOL])
        .on_upgrade(move |socket| socket_loop(socket, state, handshake.ctx))
}

const CHANGE_POLL: Duration = Duration::from_secs(1);

async fn socket_loop(mut socket: WebSocket, state: AppState, ctx: AuthContext) {
    let Ok(mut cursor) = ChangeLogRepo::new(&state.db).head_seq(&ctx).await else {
        return;
    };
    let mut outgoing = VecDeque::new();
    let mut heartbeat = tokio::time::interval(Duration::from_secs(30));
    let mut poll = tokio::time::interval(CHANGE_POLL);
    heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            _ = heartbeat.tick() => {
                if socket.send(Message::Ping(Vec::new().into())).await.is_err() {
                    break;
                }
            }
            _ = poll.tick() => {
                if drain_changes(&state.db, &ctx, &mut cursor, &mut outgoing)
                    .await
                    .is_err()
                {
                    break;
                }
                while let Some(msg) = outgoing.pop_front() {
                    let Ok(text) = serde_json::to_string(&msg) else {
                        continue;
                    };
                    if socket.send(Message::Text(text.into())).await.is_err() {
                        return;
                    }
                }
            }
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(Message::Ping(p))) => {
                        if socket.send(Message::Pong(p)).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Text(text))) if text.len() > 64 * 1024 => break,
                    Some(Ok(_) | Err(_)) => {}
                }
            }
        }
    }
}

async fn drain_changes(
    db: &Db,
    ctx: &AuthContext,
    cursor: &mut i64,
    q: &mut VecDeque<serde_json::Value>,
) -> Result<(), keeppix_db::DbError> {
    let page = ChangeLogRepo::new(db).since(ctx, *cursor).await?;
    if page.cursor == *cursor && page.upserted.is_empty() && page.deleted.is_empty() {
        return Ok(());
    }
    *cursor = page.cursor;
    if !page.upserted.is_empty() {
        enqueue(
            q,
            json!({
                "v": 1,
                "type": "assets.upserted",
                "payload": {
                    "ids": page.upserted,
                    "count": page.upserted.len()
                }
            }),
        );
    }
    if !page.deleted.is_empty() {
        enqueue(
            q,
            json!({
                "v": 1,
                "type": "assets.deleted",
                "payload": { "ids": page.deleted }
            }),
        );
    }
    Ok(())
}

pub(crate) fn origin_allowed(origin: &str, host: &str, allowlist: &[String]) -> bool {
    if allowlist.iter().any(|allowed| allowed == origin) {
        return true;
    }
    if !allowlist.is_empty() {
        return false;
    }
    if origin == format!("https://{host}") {
        return true;
    }
    origin == format!("http://{host}") && loopback_host(host)
}

fn loopback_host(host: &str) -> bool {
    let name = host.rsplit_once(':').map_or(host, |(h, _)| h);
    matches!(name, "localhost" | "127.0.0.1" | "::1" | "[::1]")
}

fn ticket_from_protocol(headers: &HeaderMap) -> Option<String> {
    let raw = headers
        .get("sec-websocket-protocol")
        .or_else(|| headers.get("Sec-WebSocket-Protocol"))
        .and_then(|v| v.to_str().ok())?;
    let parts: Vec<&str> = raw.split(',').map(str::trim).collect();
    if !parts.contains(&PROTOCOL) {
        return None;
    }
    parts
        .iter()
        .find_map(|p| p.strip_prefix("ticket."))
        .map(ToOwned::to_owned)
}

/// Coda per connessione: al 257° messaggio si svuota e resta un `resync`.
pub(crate) fn enqueue(q: &mut VecDeque<serde_json::Value>, msg: serde_json::Value) {
    if q.len() >= QUEUE_CAP {
        q.clear();
        q.push_back(serde_json::json!({"v": 1, "type": "resync", "payload": {}}));
        return;
    }
    q.push_back(msg);
}

/// Un `scan.progress` al massimo ogni 250 ms.
#[allow(dead_code)]
pub(crate) fn should_emit_progress(last: Option<Instant>, now: Instant) -> bool {
    last.is_none_or(|t| now.saturating_duration_since(t) >= Duration::from_millis(250))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn overflow_replaces_the_queue_with_resync() {
        let mut q = VecDeque::new();
        for i in 0..QUEUE_CAP {
            enqueue(&mut q, serde_json::json!({"n": i}));
        }
        assert_eq!(q.len(), QUEUE_CAP);
        enqueue(&mut q, serde_json::json!({"n": "overflow"}));
        assert_eq!(q.len(), 1);
        assert_eq!(q[0]["type"], "resync");
    }

    #[test]
    fn progress_coalesces_inside_250ms() {
        let t0 = Instant::now();
        assert!(should_emit_progress(None, t0));
        assert!(!should_emit_progress(
            Some(t0),
            t0 + Duration::from_millis(100)
        ));
        assert!(should_emit_progress(
            Some(t0),
            t0 + Duration::from_millis(250)
        ));
    }

    #[test]
    fn empty_allowlist_is_same_origin_only() {
        assert!(origin_allowed(
            "http://127.0.0.1:5673",
            "127.0.0.1:5673",
            &[]
        ));
        assert!(!origin_allowed(
            "https://evil.example",
            "127.0.0.1:5673",
            &[]
        ));
        assert!(origin_allowed(
            "https://foto.example.com",
            "foto.example.com",
            &[]
        ));
        assert!(
            !origin_allowed("http://foto.example.com", "foto.example.com", &[]),
            "http Origin must not match a non-loopback Host"
        );
    }
}
