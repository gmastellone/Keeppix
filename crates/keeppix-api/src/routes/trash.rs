//! Cancellazione a tre opzioni (spec §6): ogni `DELETE` porta esplicitamente
//! cosa succede al file, mai un comportamento implicito. `restore` è l'unica
//! via indietro, e solo per `moved_to_trash`.

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
    /// `kept`, `moved_to_trash`, o `purged` — nessun default: il client deve
    /// sempre scegliere (spec §6).
    #[schema(example = "moved_to_trash")]
    pub disk_action: String,
}

/// `pub(crate)`: riusata da `routes::duplicates::resolve`, che applica la
/// stessa azione a ogni membro non tenuto di un gruppo di duplicati.
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
/// `400` se `disk_action` non è una delle tre opzioni; `401` se non
/// autenticato; `403` se l'asset non è visibile al chiamante (anche
/// inesistente), o se `purged` è richiesto da chi non è owner/admin della
/// libreria; `500` se l'operazione sul filesystem che accompagna la
/// scrittura fallisce.
#[utoipa::path(
    delete,
    path = "/api/v1/assets/{id}",
    tag = "trash",
    operation_id = "assets_delete",
    summary = "Delete an asset",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Id dell'asset")),
    request_body = DeleteAssetRequest,
    responses(
        (status = 204, description = "Azione applicata, riga di audit registrata"),
        (status = 400, description = "disk_action non riconosciuto", body = Problem),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Asset non visibile, o purged senza i permessi", body = Problem),
        (status = 500, description = "Errore del database o del filesystem", body = Problem)
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
    /// `kept`, `moved_to_trash`, o `purged` — obbligatorio, stesso
    /// vocabolario di `DELETE /api/v1/assets/{id}` (spec §6). Nessun
    /// default: il client scelga sempre.
    #[schema(example = "moved_to_trash")]
    pub disk_action: String,
}

/// Cancellazione a tre opzioni sull'intero lotto: ogni asset è una
/// transazione a sé ([`TrashRepo::choose`] riusato senza modifiche), tranne
/// per `purged`, dove l'autorizzazione è **all-or-nothing** — un solo id non
/// purgabile rifiuta l'intero lotto prima che qualunque file venga toccato
/// (spec Fase 10 §Task 4: «il buco più rilevante per il backend», l'unico
/// dialog dell'app che distrugge dati e ha più modi di fallire).
///
/// # Errors
/// `400` `keeppix/invalid-disk-action` se `disk_action` non è una delle tre
/// opzioni, o `keeppix/batch-too-large` se il lotto supera
/// [`crate::batch::MAX_BATCH_ASSETS`]; `401` se non autenticato; `403` se
/// `purged` è richiesto e il chiamante non può eliminare dal disco anche un
/// solo asset del lotto (owner/admin richiesti) — per `kept`/`moved_to_trash`
/// un id non visibile o non modificabile finisce invece in `failed` con
/// riuscita parziale.
#[utoipa::path(
    post,
    path = "/api/v1/assets/batch/delete",
    tag = "trash",
    operation_id = "assets_batch_delete",
    summary = "Delete multiple assets",
    security(("session_cookie" = [])),
    request_body = BatchDeleteRequest,
    responses(
        (status = 200, description = "Esito per asset (riuscita parziale ammessa, tranne per purged non autorizzato)", body = BulkOutcome),
        (status = 400, description = "disk_action non riconosciuto, o lotto troppo grande", body = Problem),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "purged richiesto da chi non è owner/admin di almeno un asset del lotto", body = Problem)
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
/// `401` se non autenticato; `403` se l'asset non è visibile al chiamante;
/// `409` se l'asset non ha un cestinamento pendente, o se il percorso
/// originale è di nuovo occupato — il ripristino non sovrascrive mai.
#[utoipa::path(
    post,
    path = "/api/v1/assets/{id}/restore",
    tag = "trash",
    operation_id = "assets_restore",
    summary = "Restore an asset from trash",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Id dell'asset")),
    responses(
        (status = 204, description = "File ripristinato al percorso originale"),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Asset non visibile", body = Problem),
        (status = 409, description = "Niente da ripristinare, o percorso occupato", body = Problem),
        (status = 500, description = "Errore del database o del filesystem", body = Problem)
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
/// `400` se il cursore non è leggibile; `401` se non autenticato.
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
        (status = 200, description = "Pagina del cestino navigabile", body = TrashListPage),
        (status = 400, description = "Cursore illeggibile", body = Problem),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 500, description = "Errore del database", body = Problem)
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
/// `401` se non autenticato; `403` se il chiamante non è owner di alcuna
/// libreria né admin.
#[utoipa::path(
    post,
    path = "/api/v1/trash/empty",
    tag = "trash",
    operation_id = "trash_empty",
    summary = "Empty the trash",
    security(("session_cookie" = [])),
    responses(
        (status = 200, description = "Cestino svuotato", body = EmptyTrashResponse),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Solo owner/admin", body = Problem),
        (status = 500, description = "Errore del database o del filesystem", body = Problem)
    )
)]
pub async fn empty(
    State(state): State<AppState>,
    Auth(ctx): Auth,
) -> Result<Json<EmptyTrashResponse>, Problem> {
    let emptied = TrashRepo::new(&state.db).empty(&ctx).await?;
    Ok(Json(EmptyTrashResponse { emptied }))
}
