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

/// Una riga dell'elenco piatto composto (spec §47, Task 13): a differenza dei
/// secchi grezzi qui sotto, arriva già in linguaggio naturale nella lingua
/// della richiesta, con l'azione proposta pronta per un bottone della UI.
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
    /// Elenco piatto composto (Task 13). Additivo: i tre secchi grezzi sopra
    /// restano per non rompere un client che li leggeva già.
    problems: Vec<ProblemView>,
}

#[derive(Deserialize)]
pub struct LangQuery {
    lang: Option<String>,
}

/// La lingua viene dalla richiesta, non da una preferenza salvata sul server
/// (spec §47, Ruling Task 13): prima il parametro `?lang=`, poi
/// `Accept-Language`, altrimenti italiano — lo stesso default già in vigore
/// per `UserPreferences::language`.
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
/// `401` se non autenticato.
#[utoipa::path(
    get,
    path = "/api/v1/problems",
    tag = "library",
    operation_id = "problems_list",
    summary = "List media problems",
    params(
        ("lang" = Option<String>, Query, description = "Lingua delle descrizioni composte (\"it\" o \"en\"); default dall'header Accept-Language, poi italiano")
    ),
    security(("session_cookie" = [])),
    responses(
        (status = 200, description = "Problemi visibili", body = ProblemsView),
        (status = 401, description = "Non autenticato", body = Problem)
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
