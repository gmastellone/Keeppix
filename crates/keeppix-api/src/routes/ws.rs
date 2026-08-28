use std::collections::VecDeque;
use std::time::{Duration, Instant};

use std::collections::{HashMap, HashSet};

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{FromRequestParts, State};
use axum::http::request::Parts;
use axum::http::{HeaderMap, header};
use axum::response::Response;
use keeppix_db::{
    AssetRepo, AssetTagRepo, BackupRepo, BackupRunStatus, ChangeLogRepo, Db, FaceRepo, JobRepo,
    LibraryRepo, OperationsRepo, ProblemsRepo, RegionRepo,
};
use keeppix_domain::{AssetId, AuthContext, JobKind, LibraryId, OperationId, OperationStatus};
use serde::Serialize;
use serde_json::json;

use crate::extract::Auth;
use crate::json::Json;
use crate::problem::Problem;
use crate::state::AppState;

pub const QUEUE_CAP: usize = 256;
const PROTOCOL: &str = "keeppix.v1";

/// Consumes the ticket **before** the upgrade: a malformed handshake must
/// not leave the ticket reusable.
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
/// `401` if not authenticated.
#[utoipa::path(
    post,
    path = "/api/v1/ws/ticket",
    tag = "events",
    operation_id = "ws_ticket",
    summary = "Issue a WebSocket ticket",
    security(("session_cookie" = [])),
    responses(
        (status = 200, description = "30 s single-use ticket", body = TicketResponse),
        (status = 401, description = "Not authenticated", body = Problem)
    )
)]
pub async fn ticket(State(state): State<AppState>, Auth(ctx): Auth) -> Json<TicketResponse> {
    Json(TicketResponse {
        ticket: state.tickets.issue(ctx),
        expires_in: 30,
    })
}

/// # Errors
/// `403` if Origin is not allowed or the ticket is missing, expired, or
/// already used.
#[utoipa::path(
    get,
    path = "/api/v1/ws",
    tag = "events",
    operation_id = "ws_connect",
    summary = "Open a WebSocket connection",
    security(("session_cookie" = [])),
    responses(
        (status = 101, description = "WebSocket keeppix.v1"),
        (status = 403, description = "Invalid Origin or ticket", body = Problem)
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

// One more `drain_*` (`suggestions.changed`) pushed this past the 100-line
// cap — the function stays a flat list of "try reading a source, bail if
// the socket is dead", nothing worth factoring out without inventing an
// abstraction (heterogeneous closures over differently-typed "seen" state)
// just to stay under the limit.
#[allow(clippy::too_many_lines)]
async fn socket_loop(mut socket: WebSocket, state: AppState, ctx: AuthContext) {
    let Ok(mut cursor) = ChangeLogRepo::new(&state.db).head_seq(&ctx).await else {
        return;
    };
    // Like `cursor`: only transcodes finished *after* the connection, not
    // the whole history — unlike `operations`/backup, knowing a video is
    // ready is no longer useful once the client has already reloaded the
    // page showing it.
    let Ok(mut derivative_cursor) = JobRepo::new(&state.db)
        .max_done_id(JobKind::TranscodeVideo)
        .await
    else {
        return;
    };
    let mut outgoing = VecDeque::new();
    // Empty on every new connection **on purpose**: a client that
    // reconnects mid-operation has no memory of what it has already seen,
    // so the first poll round treats it as never seen and immediately
    // emits its current state ("arrives even after a reconnection").
    // `operations` is the source of truth, not this map.
    let mut op_seen: HashMap<OperationId, OperationProgressKey> = HashMap::new();
    let mut scan_seen: HashMap<LibraryId, ScanProgressKey> = HashMap::new();
    let mut problems_seen: Option<String> = None;
    let mut backup_seen: Option<(uuid::Uuid, BackupRunStatus)> = None;
    let mut region_seen: HashMap<String, RegionProgressKey> = HashMap::new();
    let mut suggestions_seen: Option<i64> = None;
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
                if drain_operations(&state.db, &ctx, &mut op_seen, &mut outgoing)
                    .await
                    .is_err()
                {
                    break;
                }
                if drain_scan_progress(&state.db, &ctx, &mut scan_seen, &mut outgoing)
                    .await
                    .is_err()
                {
                    break;
                }
                if drain_problems(&state.db, &ctx, &mut problems_seen, &mut outgoing)
                    .await
                    .is_err()
                {
                    break;
                }
                if drain_derivatives(&state.db, &ctx, &mut derivative_cursor, &mut outgoing)
                    .await
                    .is_err()
                {
                    break;
                }
                if drain_backup(&state.db, &ctx, &mut backup_seen, &mut outgoing)
                    .await
                    .is_err()
                {
                    break;
                }
                if drain_regions(&state.db, &ctx, &mut region_seen, &mut outgoing)
                    .await
                    .is_err()
                {
                    break;
                }
                if drain_suggestions(&state.db, &ctx, &mut suggestions_seen, &mut outgoing)
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

/// `(done, total, phase)`: the key `drain_operations` uses to decide
/// whether an operation has actually made progress worth reporting, not
/// just whether a poll round has passed.
type OperationProgressKey = (i64, Option<i64>, String);

/// Emits `operation.progress` for every `running` operation of the caller
/// that changed since the last round, plus a final message when a tracked
/// operation leaves `running` (`{operation_id, done, total, phase}`).
/// `operations` stays the only source of truth — this reads, never
/// writes — so a reconnected client sees the current state on the first
/// useful round without needing a replay of missed events.
async fn drain_operations(
    db: &Db,
    ctx: &AuthContext,
    seen: &mut HashMap<OperationId, OperationProgressKey>,
    q: &mut VecDeque<serde_json::Value>,
) -> Result<(), keeppix_db::DbError> {
    let ops = OperationsRepo::new(db);
    let running = ops.list_running(ctx).await?;
    let mut still_running = HashSet::with_capacity(running.len());
    for op in running {
        still_running.insert(op.id);
        let key: OperationProgressKey = (op.done, op.total, op.phase.clone());
        if seen.get(&op.id) == Some(&key) {
            continue;
        }
        seen.insert(op.id, key);
        enqueue(
            q,
            operation_progress_event(op.id, op.done, op.total, &op.phase),
        );
    }

    let finished: Vec<OperationId> = seen
        .keys()
        .copied()
        .filter(|id| !still_running.contains(id))
        .collect();
    for id in finished {
        seen.remove(&id);
        // Best-effort: if it's no longer visible (unlikely, the owner
        // doesn't change) it's skipped without breaking the socket over a
        // single message.
        if let Ok(op) = ops.find(ctx, id).await {
            let phase = match op.status {
                OperationStatus::Done => "done",
                OperationStatus::Cancelled => "cancelled",
                OperationStatus::Failed => "failed",
                OperationStatus::Running => op.phase.as_str(),
            };
            enqueue(q, operation_progress_event(op.id, op.done, op.total, phase));
        }
    }
    Ok(())
}

fn operation_progress_event(
    operation_id: OperationId,
    done: i64,
    total: Option<i64>,
    phase: &str,
) -> serde_json::Value {
    json!({
        "v": 1,
        "type": "operation.progress",
        "payload": {
            "operation_id": operation_id.to_string(),
            "done": done,
            "total": total,
            "phase": phase,
        }
    })
}

/// `(phase, asset_count)`: the key `drain_scan_progress` uses to decide
/// whether a library has actually made progress worth reporting.
type ScanProgressKey = (String, i64);

/// Emits `scan.progress` for every library visible to the caller whose
/// `(phase, asset count)` changed since the last round. Same source
/// already used by `GET /libraries/{id}/scan`
/// (`JobRepo::discover_status_for_library` + `AssetRepo::count_in_library`),
/// not a second invented state — this also covers rescans triggered by the
/// watcher, which have no `operation_id` and so don't show up in
/// `operation.progress`.
async fn drain_scan_progress(
    db: &Db,
    ctx: &AuthContext,
    seen: &mut HashMap<LibraryId, ScanProgressKey>,
    q: &mut VecDeque<serde_json::Value>,
) -> Result<(), keeppix_db::DbError> {
    let libraries = LibraryRepo::new(db).list(ctx).await?;
    let jobs = JobRepo::new(db);
    let assets = AssetRepo::new(db);
    let mut still_visible = HashSet::with_capacity(libraries.len());
    for library in libraries {
        still_visible.insert(library.id);
        let job = jobs.discover_status_for_library(library.id).await?;
        let phase = crate::routes::libraries::scan_phase(library.status, job.map(|j| j.status));
        let asset_count = assets.count_in_library(library.id).await?;
        let key: ScanProgressKey = (phase.to_owned(), asset_count);
        if seen.get(&library.id) == Some(&key) {
            continue;
        }
        seen.insert(library.id, key.clone());
        enqueue(
            q,
            json!({
                "v": 1,
                "type": "scan.progress",
                "payload": {
                    "library_id": library.id.to_string(),
                    "phase": key.0,
                    "asset_count": key.1,
                }
            }),
        );
    }
    seen.retain(|id, _| still_visible.contains(id));
    Ok(())
}

/// Emits `problems.changed` when the composed list (`ProblemsRepo::list`,
/// already used by `GET /problems`) changes relative to the last round.
/// Deliberately thin by contract (*"a signal, not a state"*): it carries a
/// count as a convenience, never the ids — a client that misses it must
/// reload `GET /problems`, not trust the number.
///
/// The first round with an empty list does not emit: there is no change to
/// report, and a `count: 0` queued right away would race with the opening
/// Ping — tests (and a slow client) would mistake it for the next "there is
/// a problem" event. A reconnect with problems already present keeps
/// emitting right away (like `operations`). Returning to zero problems
/// (previous non-empty signature → count 0) emits normally.
async fn drain_problems(
    db: &Db,
    ctx: &AuthContext,
    seen: &mut Option<String>,
    q: &mut VecDeque<serde_json::Value>,
) -> Result<(), keeppix_db::DbError> {
    let set = ProblemsRepo::new(db).list(ctx).await?;
    let signature = problems_signature(&set);
    if seen.as_deref() == Some(signature.as_str()) {
        return Ok(());
    }
    let count = set.offline_libraries.len() + set.failed_jobs.len() + set.error_assets.len();
    let first_empty_baseline = seen.is_none() && count == 0;
    *seen = Some(signature);
    if first_empty_baseline {
        return Ok(());
    }
    enqueue(
        q,
        json!({
            "v": 1,
            "type": "problems.changed",
            "payload": { "count": count }
        }),
    );
    Ok(())
}

fn problems_signature(set: &keeppix_db::ProblemSet) -> String {
    let mut offline: Vec<String> = set
        .offline_libraries
        .iter()
        .map(|l| l.id.to_string())
        .collect();
    let mut jobs: Vec<String> = set.failed_jobs.iter().map(|j| j.id.to_string()).collect();
    let mut assets: Vec<String> = set.error_assets.iter().map(|a| a.id.to_string()).collect();
    offline.sort_unstable();
    jobs.sort_unstable();
    assets.sort_unstable();
    format!(
        "{}|{}|{}",
        offline.join(","),
        jobs.join(","),
        assets.join(",")
    )
}

/// Emits `suggestions.changed` — the Review badge. Same tag+face sum as
/// `GET /bootstrap` (`AssetTagRepo::count_proposed_visible` +
/// `FaceRepo::count_proposed_visible`), a single channel because the badge
/// is already a combined count, not two.
///
/// Like `problems.changed`, the number travels as a convenience (the
/// contract stays "reload the counter", the client must not trust the
/// value to decide whether it changed — carrying the number remains
/// allowed as a convenience, never as a guarantee). Same "first connection
/// at zero does not emit" guard as `drain_problems`, for the same reason:
/// a `count: 0` queued on the first connection would race with the opening
/// Ping.
async fn drain_suggestions(
    db: &Db,
    ctx: &AuthContext,
    seen: &mut Option<i64>,
    q: &mut VecDeque<serde_json::Value>,
) -> Result<(), keeppix_db::DbError> {
    let tag_count = AssetTagRepo::new(db).count_proposed_visible(ctx).await?;
    let face_count = FaceRepo::new(db).count_proposed_visible(ctx).await?;
    let count = tag_count.saturating_add(face_count);
    if *seen == Some(count) {
        return Ok(());
    }
    let first_empty_baseline = seen.is_none() && count == 0;
    *seen = Some(count);
    if first_empty_baseline {
        return Ok(());
    }
    enqueue(
        q,
        json!({
            "v": 1,
            "type": "suggestions.changed",
            "payload": { "count": count }
        }),
    );
    Ok(())
}

/// Emits `asset.derivative.ready` for every `TranscodeVideo` finished
/// after `cursor` whose asset is visible to the caller. Reads the same
/// `jobs` queue as the real pipeline, not an in-process channel: a job
/// completed by any worker arrives here on the next poll round.
async fn drain_derivatives(
    db: &Db,
    ctx: &AuthContext,
    cursor: &mut i64,
    q: &mut VecDeque<serde_json::Value>,
) -> Result<(), keeppix_db::DbError> {
    let done = JobRepo::new(db)
        .list_recently_done(JobKind::TranscodeVideo, *cursor, 50)
        .await?;
    if done.is_empty() {
        return Ok(());
    }
    let mut ids = Vec::with_capacity(done.len());
    for job in &done {
        *cursor = (*cursor).max(job.id);
        if let Some(id) = job
            .payload
            .get("asset_id")
            .and_then(serde_json::Value::as_str)
            .and_then(|raw| uuid::Uuid::parse_str(raw).ok())
        {
            ids.push(AssetId::from_uuid(id));
        }
    }
    let visible = AssetRepo::new(db).filter_visible(ctx, &ids).await?;
    for id in visible {
        enqueue(
            q,
            json!({
                "v": 1,
                "type": "asset.derivative.ready",
                "payload": { "asset_id": id.to_string() }
            }),
        );
    }
    Ok(())
}

/// Emits `backup.finished` when the last run (`BackupRepo::list_runs`,
/// admin-only like the Settings page) has left the `running` state since
/// the last round. Admin only: a regular user cannot see backups, so this
/// returns early instead of propagating the `Forbidden` that would break
/// the socket.
async fn drain_backup(
    db: &Db,
    ctx: &AuthContext,
    seen: &mut Option<(uuid::Uuid, BackupRunStatus)>,
    q: &mut VecDeque<serde_json::Value>,
) -> Result<(), keeppix_db::DbError> {
    if !ctx.is_admin() {
        return Ok(());
    }
    let Some(run) = BackupRepo::new(db)
        .list_runs(ctx, 1)
        .await?
        .into_iter()
        .next()
    else {
        return Ok(());
    };
    let key = (run.id, run.status);
    if *seen == Some(key) {
        return Ok(());
    }
    *seen = Some(key);
    if run.status == BackupRunStatus::Running {
        return Ok(());
    }
    enqueue(
        q,
        json!({
            "v": 1,
            "type": "backup.finished",
            "payload": {
                "run_id": run.id.to_string(),
                "status": run.status.as_str(),
                "size_bytes": run.size_bytes,
                "error": run.error,
            }
        }),
    );
    Ok(())
}

/// `(status, downloaded_bytes, last_error)`: the key `drain_regions` uses
/// to decide whether a map region has actually made progress worth
/// reporting.
type RegionProgressKey = (String, i64, Option<String>);

/// Emits `region.progress` for every map region whose `(status,
/// downloaded_bytes, last_error)` changed since the last round.
/// `RegionView` already carries these fields — this reads the same
/// `RegionRepo::list`, not a second invented state, and builds the payload
/// from those fields. `RegionRepo::list` only requires an authenticated
/// user: map regions are global to the instance, not owned by a single
/// user.
async fn drain_regions(
    db: &Db,
    ctx: &AuthContext,
    seen: &mut HashMap<String, RegionProgressKey>,
    q: &mut VecDeque<serde_json::Value>,
) -> Result<(), keeppix_db::DbError> {
    let regions = RegionRepo::new(db).list(ctx).await?;
    let mut still_present = HashSet::with_capacity(regions.len());
    for region in regions {
        still_present.insert(region.id.clone());
        let key: RegionProgressKey = (
            region.status.as_str().to_owned(),
            region.downloaded_bytes,
            region.last_error.clone(),
        );
        if seen.get(&region.id) == Some(&key) {
            continue;
        }
        seen.insert(region.id.clone(), key.clone());
        enqueue(
            q,
            json!({
                "v": 1,
                "type": "region.progress",
                "payload": {
                    "region_id": region.id,
                    "status": key.0,
                    "downloaded_bytes": key.1,
                    "size_bytes": region.size_bytes,
                    "last_error": key.2,
                }
            }),
        );
    }
    seen.retain(|id, _| still_present.contains(id));
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

/// Per-connection queue: at the 257th message it is cleared and only a
/// `resync` remains.
pub(crate) fn enqueue(q: &mut VecDeque<serde_json::Value>, msg: serde_json::Value) {
    if q.len() >= QUEUE_CAP {
        q.clear();
        q.push_back(serde_json::json!({"v": 1, "type": "resync", "payload": {}}));
        return;
    }
    q.push_back(msg);
}

/// A `scan.progress` at most every 250 ms.
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
