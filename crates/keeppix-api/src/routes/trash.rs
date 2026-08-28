//! Three-option deletion: every `DELETE` explicitly carries what happens to
//! the file, never implicit behavior. `restore` is the only way back, and
//! only for `moved_to_trash`.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use chrono::{DateTime, SecondsFormat, Utc};
use keeppix_db::{TRASH_RETENTION_DAYS, TrashRepo};
use keeppix_domain::{AssetId, DiskAction, TrashEntry, TrashEntryId};
use serde::{Deserialize, Serialize};

use crate::bulk::BulkOutcome;
use crate::extract::Auth;
use crate::json::Json;
use crate::problem::Problem;
use crate::state::AppState;

#[derive(Deserialize, utoipa::ToSchema)]
pub struct DeleteAssetRequest {
    /// `kept`, `moved_to_trash`, or `purged` — no default: the client must
    /// always choose.
    #[schema(example = "moved_to_trash")]
    pub disk_action: String,
}

/// `pub(crate)`: reused by `routes::duplicates::resolve`, which applies the
/// same action to every non-kept member of a duplicate group.
pub(crate) fn parse_action(raw: &str) -> Result<DiskAction, Problem> {
    DiskAction::parse(raw).map_err(|e| {
        Problem::bad_request(
            "invalid-disk-action",
            "disk_action must be kept, moved_to_trash, or purged",
        )
        .with_detail(e.to_string())
    })
}

/// # Errors
/// `400` if `disk_action` is not one of the three options; `401` if not
/// authenticated; `403` if the asset is not visible to the caller (even
/// nonexistent), or if `purged` is requested by someone who is not
/// owner/admin of the library; `500` if the filesystem operation
/// accompanying the write fails.
#[utoipa::path(
    delete,
    path = "/api/v1/assets/{id}",
    tag = "trash",
    operation_id = "assets_delete",
    summary = "Delete an asset",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Asset id")),
    request_body = DeleteAssetRequest,
    responses(
        (status = 204, description = "Action applied, audit row recorded"),
        (status = 400, description = "Unrecognized disk_action", body = Problem),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "Asset not visible, or purged without permission", body = Problem),
        (status = 500, description = "Database or filesystem error", body = Problem)
    )
)]
pub async fn delete(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Path(id): Path<AssetId>,
    Json(body): Json<DeleteAssetRequest>,
) -> Result<StatusCode, Problem> {
    let action = parse_action(&body.disk_action)?;
    TrashRepo::new(&state.db).choose(&ctx, id, action).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct BatchDeleteRequest {
    #[schema(value_type = Vec<String>)]
    pub asset_ids: Vec<AssetId>,
    /// `kept`, `moved_to_trash`, or `purged` — required, same vocabulary as
    /// `DELETE /api/v1/assets/{id}`. No default: the client must always
    /// choose.
    #[schema(example = "moved_to_trash")]
    pub disk_action: String,
}

/// Three-option deletion over the whole batch: each asset is its own
/// transaction ([`TrashRepo::choose`] reused unchanged), except for
/// `purged`, where authorization is **all-or-nothing** — a single
/// non-purgeable id rejects the entire batch before any file is touched
/// (the one dialog in the app that destroys data and has the most ways to
/// fail).
///
/// # Errors
/// `400` `keeppix/invalid-disk-action` if `disk_action` is not one of the
/// three options, or `keeppix/batch-too-large` if the batch exceeds
/// [`crate::batch::MAX_BATCH_ASSETS`]; `401` if not authenticated; `403` if
/// `purged` is requested and the caller cannot delete from disk even one
/// asset of the batch (owner/admin required) — for `kept`/`moved_to_trash`
/// a non-visible or non-editable id instead ends up in `failed` with
/// partial success.
#[utoipa::path(
    post,
    path = "/api/v1/assets/batch/delete",
    tag = "trash",
    operation_id = "assets_batch_delete",
    summary = "Delete multiple assets",
    security(("session_cookie" = [])),
    request_body = BatchDeleteRequest,
    responses(
        (status = 200, description = "Per-asset outcome (partial success allowed, except for unauthorized purged)", body = BulkOutcome),
        (status = 400, description = "Unrecognized disk_action, or batch too large", body = Problem),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "purged requested by someone who is not owner/admin of at least one asset in the batch", body = Problem)
    )
)]
pub async fn batch_delete(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Json(body): Json<BatchDeleteRequest>,
) -> Result<Json<BulkOutcome>, Problem> {
    crate::batch::reject_oversized_batch(&body.asset_ids)?;
    let action = parse_action(&body.disk_action)?;
    let trash = TrashRepo::new(&state.db);

    if matches!(action, DiskAction::Purged) {
        trash
            .assert_batch_purge_authorized(&ctx, &body.asset_ids)
            .await?;
    }

    let mut succeeded = Vec::new();
    let mut failed = Vec::new();
    for id in body.asset_ids {
        match trash.choose(&ctx, id, action).await {
            Ok(_) => succeeded.push(id),
            Err(e) => failed.push((id, e)),
        }
    }

    Ok(Json(BulkOutcome::from_partition(succeeded, &failed, None)))
}

/// # Errors
/// `401` if not authenticated; `403` if the asset is not visible to the
/// caller; `409` if the asset has no pending trashing, or if the original
/// path is occupied again — restore never overwrites.
#[utoipa::path(
    post,
    path = "/api/v1/assets/{id}/restore",
    tag = "trash",
    operation_id = "assets_restore",
    summary = "Restore an asset from trash",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Asset id")),
    responses(
        (status = 204, description = "File restored to its original path"),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "Asset not visible", body = Problem),
        (status = 409, description = "Nothing to restore, or path occupied", body = Problem),
        (status = 500, description = "Database or filesystem error", body = Problem)
    )
)]
pub async fn restore(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Path(id): Path<AssetId>,
) -> Result<StatusCode, Problem> {
    TrashRepo::new(&state.db).restore(&ctx, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct TrashListQuery {
    cursor: Option<String>,
    limit: Option<i64>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct TrashItemView {
    pub id: String,
    pub asset_id: String,
    pub deleted_at: DateTime<Utc>,
    pub original_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trash_path: Option<String>,
    pub disk_action: String,
    pub days_remaining: i64,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct TrashListPage {
    pub items: Vec<TrashItemView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct EmptyTrashResponse {
    pub emptied: u64,
}

fn days_remaining(deleted_at: DateTime<Utc>) -> i64 {
    let elapsed = (Utc::now() - deleted_at).num_days();
    (TRASH_RETENTION_DAYS - elapsed).max(0)
}

fn trash_item_view(entry: &TrashEntry) -> TrashItemView {
    TrashItemView {
        id: entry.id.to_string(),
        asset_id: entry.asset_id.to_string(),
        deleted_at: entry.deleted_at,
        original_path: entry.original_path.clone(),
        trash_path: entry.trash_path.clone(),
        disk_action: entry.disk_action.as_str().to_owned(),
        days_remaining: days_remaining(entry.deleted_at),
    }
}

fn parse_trash_cursor(raw: &str) -> Result<(DateTime<Utc>, TrashEntryId), Problem> {
    let (time, id) = raw.split_once('|').ok_or_else(|| {
        Problem::bad_request("invalid-query", "Invalid trash cursor").with_detail(raw)
    })?;
    let deleted_at = DateTime::parse_from_rfc3339(time)
        .map(|t| t.with_timezone(&Utc))
        .map_err(|_| {
            Problem::bad_request("invalid-query", "Invalid trash cursor").with_detail(raw)
        })?;
    let entry_id = id.parse::<TrashEntryId>().map_err(|_| {
        Problem::bad_request("invalid-query", "Invalid trash cursor").with_detail(raw)
    })?;
    Ok((deleted_at, entry_id))
}

fn encode_trash_cursor(entry: &TrashEntry) -> String {
    let deleted_at = entry
        .deleted_at
        .to_rfc3339_opts(SecondsFormat::Micros, true);
    format!("{deleted_at}|{}", entry.id)
}

/// # Errors
/// `400` if the cursor is not readable; `401` if not authenticated.
#[utoipa::path(
    get,
    path = "/api/v1/trash",
    tag = "trash",
    operation_id = "trash_list",
    summary = "List trash assets",
    security(("session_cookie" = [])),
    params(
        ("cursor" = Option<String>, Query, description = "Keyset deleted_at|id"),
        ("limit" = Option<i64>, Query, description = "1..=100, default 50")
    ),
    responses(
        (status = 200, description = "Paginated trash page", body = TrashListPage),
        (status = 400, description = "Unreadable cursor", body = Problem),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 500, description = "Database error", body = Problem)
    )
)]
pub async fn list(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Query(query): Query<TrashListQuery>,
) -> Result<Json<TrashListPage>, Problem> {
    let cursor = match query.cursor.as_deref() {
        None | Some("") => None,
        Some(raw) => Some(parse_trash_cursor(raw)?),
    };
    let limit = query.limit.unwrap_or(50).clamp(1, 100);
    let entries = TrashRepo::new(&state.db)
        .list_pending(&ctx, cursor, limit)
        .await?;
    let filled = i64::try_from(entries.len()).unwrap_or(i64::MAX) >= limit;
    let next_cursor = filled
        .then(|| entries.last().map(encode_trash_cursor))
        .flatten();
    Ok(Json(TrashListPage {
        items: entries.iter().map(trash_item_view).collect(),
        next_cursor,
    }))
}

/// # Errors
/// `401` if not authenticated; `403` if the caller is not owner of any
/// library, nor admin.
#[utoipa::path(
    post,
    path = "/api/v1/trash/empty",
    tag = "trash",
    operation_id = "trash_empty",
    summary = "Empty the trash",
    security(("session_cookie" = [])),
    responses(
        (status = 200, description = "Trash emptied", body = EmptyTrashResponse),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "Owner/admin only", body = Problem),
        (status = 500, description = "Database or filesystem error", body = Problem)
    )
)]
pub async fn empty(
    State(state): State<AppState>,
    Auth(ctx): Auth,
) -> Result<Json<EmptyTrashResponse>, Problem> {
    let emptied = TrashRepo::new(&state.db).empty(&ctx).await?;
    Ok(Json(EmptyTrashResponse { emptied }))
}
