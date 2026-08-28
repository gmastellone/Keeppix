use axum::extract::{Query, State};
use axum::http::HeaderMap;
use keeppix_db::{ComposedProblem, ProblemAction, ProblemLanguage, ProblemsRepo};
use serde::{Deserialize, Serialize};

use crate::extract::Auth;
use crate::json::Json;
use crate::problem::Problem;
use crate::state::AppState;

#[derive(Serialize, utoipa::ToSchema)]
pub struct OfflineLibraryView {
    id: String,
    name: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct FailedJobView {
    id: i64,
    kind: String,
    last_error: Option<String>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct ErrorAssetView {
    id: String,
    filename: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct ProblemActionView {
    action: String,
    label: String,
}

impl From<ProblemAction> for ProblemActionView {
    fn from(a: ProblemAction) -> Self {
        Self {
            action: a.action,
            label: a.label,
        }
    }
}

/// A row of the composed flat list: unlike the raw buckets below, it
/// arrives already in natural language in the request's language, with
/// the proposed action ready for a UI button.
#[derive(Serialize, utoipa::ToSchema)]
pub struct ProblemView {
    id: String,
    severity: String,
    title: String,
    description: String,
    library_id: Option<String>,
    library_name: Option<String>,
    folder_id: Option<String>,
    folder_name: Option<String>,
    actions: Vec<ProblemActionView>,
}

impl From<ComposedProblem> for ProblemView {
    fn from(p: ComposedProblem) -> Self {
        Self {
            id: p.id,
            severity: p.severity.as_str().to_owned(),
            title: p.title,
            description: p.description,
            library_id: p.library_id.map(|id| id.to_string()),
            library_name: p.library_name,
            folder_id: p.folder_id.map(|id| id.to_string()),
            folder_name: p.folder_name,
            actions: p.actions.into_iter().map(ProblemActionView::from).collect(),
        }
    }
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct ProblemsView {
    offline_libraries: Vec<OfflineLibraryView>,
    failed_jobs: Vec<FailedJobView>,
    error_assets: Vec<ErrorAssetView>,
    /// Composed flat list. Additive: the three raw buckets above stay to
    /// avoid breaking a client that was already reading them.
    problems: Vec<ProblemView>,
}

#[derive(Deserialize)]
pub struct LangQuery {
    lang: Option<String>,
}

/// The language comes from the request, not from a preference saved on the
/// server: first the `?lang=` parameter, then `Accept-Language`, otherwise
/// Italian — the same default already in effect for
/// `UserPreferences::language`.
fn resolve_language(query: &LangQuery, headers: &HeaderMap) -> ProblemLanguage {
    if let Some(lang) = query.lang.as_deref() {
        return ProblemLanguage::parse(lang);
    }
    let accept_language = headers
        .get(axum::http::header::ACCEPT_LANGUAGE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let first = accept_language.split(',').next().unwrap_or("").trim();
    ProblemLanguage::parse(first)
}

/// # Errors
/// `401` if not authenticated.
#[utoipa::path(
    get,
    path = "/api/v1/problems",
    tag = "library",
    operation_id = "problems_list",
    summary = "List media problems",
    params(
        ("lang" = Option<String>, Query, description = "Language of composed descriptions (\"it\" or \"en\"); defaults from the Accept-Language header, then Italian")
    ),
    security(("session_cookie" = [])),
    responses(
        (status = 200, description = "Visible problems", body = ProblemsView),
        (status = 401, description = "Not authenticated", body = Problem)
    )
)]
pub async fn list(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Query(query): Query<LangQuery>,
    headers: HeaderMap,
) -> Result<Json<ProblemsView>, Problem> {
    let lang = resolve_language(&query, &headers);
    let repo = ProblemsRepo::new(&state.db);
    let set = repo.list(&ctx).await?;
    let composed = repo.compose(&set, lang).await?;
    Ok(Json(ProblemsView {
        offline_libraries: set
            .offline_libraries
            .into_iter()
            .map(|l| OfflineLibraryView {
                id: l.id.to_string(),
                name: l.name,
            })
            .collect(),
        failed_jobs: set
            .failed_jobs
            .into_iter()
            .map(|j| FailedJobView {
                id: j.id,
                kind: j.kind,
                last_error: j.last_error,
            })
            .collect(),
        error_assets: set
            .error_assets
            .into_iter()
            .map(|a| ErrorAssetView {
                id: a.id.to_string(),
                filename: a.filename,
            })
            .collect(),
        problems: composed.into_iter().map(ProblemView::from).collect(),
    }))
}
