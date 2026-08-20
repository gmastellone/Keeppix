use axum::extract::{Query, State};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use keeppix_db::{FlagRepo, SearchNode, SearchRepo};
use keeppix_domain::AssetId;
use serde::{Deserialize, Serialize};

use crate::extract::{Auth, SessionNotShare};
use crate::json::Json;
use crate::problem::Problem;
use crate::routes::timeline::{AssetView, encode_cursor};
use crate::state::AppState;

#[derive(Deserialize, utoipa::ToSchema)]
pub struct SearchRequest {
    #[schema(value_type = Object)]
    ast: SearchNode,
    cursor: Option<String>,
    limit: Option<i64>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct SearchPage {
    assets: Vec<AssetView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    next_cursor: Option<String>,
}

#[derive(Deserialize)]
pub struct SuggestQuery {
    q: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct SuggestResponse {
    suggestions: Vec<String>,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct SavedSearchRequest {
    name: String,
    query_text: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct SavedSearchView {
    id: String,
    name: String,
    query_text: String,
}

/// # Errors
/// `400` se il cursore è illeggibile; `401` se non autenticato.
#[utoipa::path(
    post,
    path = "/api/v1/search",
    tag = "search",
    operation_id = "search_run",
    summary = "Run a search",
    security(("session_cookie" = [])),
    request_body = SearchRequest,
    responses(
        (status = 200, description = "Risultati visibili", body = SearchPage),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 500, description = "Errore del database", body = Problem)
    )
)]
pub async fn run(
    State(state): State<AppState>,
    SessionNotShare(ctx): SessionNotShare,
    Json(body): Json<SearchRequest>,
) -> Result<Json<SearchPage>, Problem> {
    let cursor = match body.cursor.as_deref() {
        None | Some("") => None,
        Some(raw) => Some(parse_cursor(raw)?),
    };
    let assets = SearchRepo::new(&state.db)
        .run(&ctx, &body.ast, cursor, body.limit.unwrap_or(200))
        .await?;
    let limit = body.limit.unwrap_or(200).clamp(1, 200);
    let filled = i64::try_from(assets.len()).unwrap_or(i64::MAX) >= limit;
    let next_cursor = filled
        .then(|| assets.last().map(|a| encode_cursor(a)))
        .flatten();
    let ids: Vec<AssetId> = assets.iter().map(|a| a.id).collect();
    let favorites = FlagRepo::new(&state.db).favorites_among(&ctx, &ids).await?;
    Ok(Json(SearchPage {
        assets: assets
            .iter()
            .map(|a| AssetView::from_asset_with_stack(a).with_favorite(favorites.contains(&a.id)))
            .collect(),
        next_cursor,
    }))
}

/// # Errors
/// `401` se non autenticato.
#[utoipa::path(
    get,
    path = "/api/v1/search/suggest",
    tag = "search",
    operation_id = "search_suggest",
    summary = "Suggest search terms",
    security(("session_cookie" = [])),
    params(("q" = String, Query, description = "Prefisso")),
    responses(
        (status = 200, description = "Suggerimenti visibili", body = SuggestResponse),
        (status = 401, description = "Non autenticato", body = Problem)
    )
)]
pub async fn suggest(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Query(query): Query<SuggestQuery>,
) -> Result<Json<SuggestResponse>, Problem> {
    let suggestions = SearchRepo::new(&state.db).suggest(&ctx, &query.q).await?;
    Ok(Json(SuggestResponse { suggestions }))
}

/// # Errors
/// `401` se non autenticato.
#[utoipa::path(
    get,
    path = "/api/v1/saved-searches",
    tag = "search",
    operation_id = "saved_searches_list",
    summary = "List saved searches",
    security(("session_cookie" = [])),
    responses(
        (status = 200, description = "Ricerche salvate dell'utente", body = [SavedSearchView]),
        (status = 401, description = "Non autenticato", body = Problem)
    )
)]
pub async fn list_saved(
    State(state): State<AppState>,
    Auth(ctx): Auth,
) -> Result<Json<Vec<SavedSearchView>>, Problem> {
    let rows = SearchRepo::new(&state.db).list_saved(&ctx).await?;
    Ok(Json(
        rows.into_iter()
            .map(|s| SavedSearchView {
                id: s.id.to_string(),
                name: s.name,
                query_text: s.query_text,
            })
            .collect(),
    ))
}

/// # Errors
/// `401` se non autenticato.
#[utoipa::path(
    post,
    path = "/api/v1/saved-searches",
    tag = "search",
    operation_id = "saved_searches_create",
    summary = "Create a saved search",
    security(("session_cookie" = [])),
    request_body = SavedSearchRequest,
    responses(
        (status = 201, description = "Ricerca salvata", body = SavedSearchView),
        (status = 401, description = "Non autenticato", body = Problem)
    )
)]
pub async fn create_saved(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Json(body): Json<SavedSearchRequest>,
) -> Result<(StatusCode, Json<SavedSearchView>), Problem> {
    let saved = SearchRepo::new(&state.db)
        .create_saved(&ctx, &body.name, &body.query_text)
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(SavedSearchView {
            id: saved.id.to_string(),
            name: saved.name,
            query_text: saved.query_text,
        }),
    ))
}

fn parse_cursor(raw: &str) -> Result<(DateTime<Utc>, AssetId), Problem> {
    let (time, id) = raw.split_once('|').ok_or_else(|| {
        Problem::bad_request("invalid-query", "Invalid search cursor").with_detail(raw)
    })?;
    let taken_at = DateTime::parse_from_rfc3339(time)
        .map(|t| t.with_timezone(&Utc))
        .map_err(|_| {
            Problem::bad_request("invalid-query", "Invalid search cursor").with_detail(raw)
        })?;
    let asset_id = id.parse::<AssetId>().map_err(|_| {
        Problem::bad_request("invalid-query", "Invalid search cursor").with_detail(raw)
    })?;
    Ok((taken_at, asset_id))
}
