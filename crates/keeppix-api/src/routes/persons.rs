//! Persone e gruppi di persone (Fase 8 Task 6/7). Distinti dai `groups`
//! della Fase 3 (permessi utenti) — vedi `keeppix_db::PersonGroupRepo`.
#![allow(clippy::missing_errors_doc)]

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use keeppix_db::{NewPersonGroup, PersonGroupRepo, PersonRepo, PersonSummary};
use keeppix_domain::{Person, PersonGroup, PersonGroupId, PersonId, PersonName};
use serde::{Deserialize, Serialize};

use crate::extract::Auth;
use crate::json::Json;
use crate::problem::Problem;
use crate::state::AppState;

#[derive(Serialize, utoipa::ToSchema)]
pub struct PersonView {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cover_face_id: Option<String>,
    pub hidden: bool,
    /// Presente per `GET /persons` (elenco); assente per le risposte di
    /// singola persona, che non pagano un secondo giro di query per il
    /// conteggio.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub face_count: Option<i64>,
    pub created_at: String,
}

impl PersonView {
    fn from_person(p: &Person) -> Self {
        Self {
            id: p.id.to_string(),
            name: p.name.clone(),
            cover_face_id: p.cover_face_id.map(|id| id.to_string()),
            hidden: p.is_hidden(),
            face_count: None,
            created_at: p.created_at.to_rfc3339(),
        }
    }

    fn from_summary(s: &PersonSummary) -> Self {
        Self {
            face_count: Some(s.face_count),
            ..Self::from_person(&s.person)
        }
    }
}

fn parse_person_name(raw: &str) -> Result<Option<PersonName>, Problem> {
    if raw.is_empty() {
        return Ok(None);
    }
    PersonName::parse(raw).map(Some).map_err(|_| {
        Problem::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid-person-name",
            "Person name cannot be blank",
        )
    })
}

#[derive(Deserialize)]
pub struct ListPersonsQuery {
    #[serde(default)]
    pub include_hidden: bool,
}

/// # Errors
/// `401` senza utente autenticato.
#[utoipa::path(
    get,
    path = "/api/v1/persons",
    tag = "persons",
    operation_id = "persons_list",
    summary = "List persons visible to the caller",
    security(("session_cookie" = [])),
    params(("include_hidden" = Option<bool>, Query, description = "Include persone nascoste")),
    responses(
        (status = 200, description = "Persone visibili, con conteggio volti", body = Vec<PersonView>),
        (status = 401, description = "Non autenticato", body = Problem)
    )
)]
pub async fn list(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Query(q): Query<ListPersonsQuery>,
) -> Result<Json<Vec<PersonView>>, Problem> {
    let summaries = PersonRepo::new(&state.db)
        .list_visible(&ctx, q.include_hidden)
        .await?;
    Ok(Json(
        summaries.iter().map(PersonView::from_summary).collect(),
    ))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreatePersonRequest {
    /// Vuoto o assente: persona senza nome («Persona 4»). Non vuoto: nome
    /// validato, non può essere solo spazi.
    #[serde(default)]
    pub name: String,
}

/// # Errors
/// `401` senza utente autenticato (solo per uniformità con le altre rotte:
/// il repository non richiede visibilità per creare). `409` se il nome è
/// già in uso. `422` se il nome non è vuoto ma è solo spazi.
#[utoipa::path(
    post,
    path = "/api/v1/persons",
    tag = "persons",
    operation_id = "persons_create",
    summary = "Create a person (used for the review queue's «new person» outcome)",
    security(("session_cookie" = [])),
    request_body = CreatePersonRequest,
    responses(
        (status = 201, description = "Persona creata", body = PersonView),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 409, description = "Nome già in uso", body = Problem),
        (status = 422, description = "Nome non valido", body = Problem)
    )
)]
pub async fn create(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Json(body): Json<CreatePersonRequest>,
) -> Result<(StatusCode, Json<PersonView>), Problem> {
    if ctx.user_id().is_none() {
        return Err(Problem::unauthenticated());
    }
    let name = parse_person_name(&body.name)?;
    let person = PersonRepo::new(&state.db).create(name).await?;
    Ok((StatusCode::CREATED, Json(PersonView::from_person(&person))))
}

/// # Errors
/// `403` se nessun volto della persona è visibile al chiamante. `404` se non esiste.
#[utoipa::path(
    get,
    path = "/api/v1/persons/{id}",
    tag = "persons",
    operation_id = "persons_get",
    summary = "Get a person",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Id persona")),
    responses(
        (status = 200, description = "Persona", body = PersonView),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Nessun volto visibile al chiamante", body = Problem),
        (status = 404, description = "Persona inesistente", body = Problem)
    )
)]
pub async fn get(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Path(id): Path<PersonId>,
) -> Result<Json<PersonView>, Problem> {
    let person = PersonRepo::new(&state.db).find_by_id(&ctx, id).await?;
    Ok(Json(PersonView::from_person(&person)))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct PatchPersonRequest {
    /// Assente: nome invariato. Stringa vuota: cancella il nome. Stringa
    /// non vuota: nuovo nome (rifiutato se solo spazi).
    #[serde(default)]
    pub name: Option<String>,
    pub hidden: Option<bool>,
    #[schema(value_type = Option<String>)]
    pub cover_face_id: Option<keeppix_domain::FaceId>,
}

/// # Errors
/// Come [`get`]. `409` se il nuovo nome è già in uso, o se `cover_face_id`
/// non appartiene a questa persona.
#[utoipa::path(
    patch,
    path = "/api/v1/persons/{id}",
    tag = "persons",
    operation_id = "persons_patch",
    summary = "Rename, hide/show, or set the cover of a person",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Id persona")),
    request_body = PatchPersonRequest,
    responses(
        (status = 200, description = "Persona aggiornata", body = PersonView),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Nessun volto visibile al chiamante", body = Problem),
        (status = 404, description = "Persona inesistente", body = Problem),
        (status = 409, description = "Nome già in uso, o copertina non di questa persona", body = Problem)
    )
)]
pub async fn patch(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Path(id): Path<PersonId>,
    Json(body): Json<PatchPersonRequest>,
) -> Result<Json<PersonView>, Problem> {
    let repo = PersonRepo::new(&state.db);
    let mut person = repo.find_by_id(&ctx, id).await?;
    if let Some(raw) = &body.name {
        let name = parse_person_name(raw)?;
        person = repo.rename(&ctx, id, name).await?;
    }
    if let Some(hidden) = body.hidden {
        person = repo.set_hidden(&ctx, id, hidden).await?;
    }
    if let Some(face_id) = body.cover_face_id {
        person = repo.set_cover(&ctx, id, face_id).await?;
    }
    Ok(Json(PersonView::from_person(&person)))
}

/// Cancella la persona: i suoi volti restano, pronti per un nuovo
/// raggruppamento (`person_id` torna `NULL`) — **non** cancella i dati dei
/// volti (quella è l'interruttore di libreria, Task 10).
///
/// # Errors
/// Come [`get`].
#[utoipa::path(
    delete,
    path = "/api/v1/persons/{id}",
    tag = "persons",
    operation_id = "persons_delete",
    summary = "Delete a person (their faces stay, ready to be re-grouped)",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Id persona")),
    responses(
        (status = 204, description = "Cancellata"),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Nessun volto visibile al chiamante", body = Problem),
        (status = 404, description = "Persona inesistente", body = Problem)
    )
)]
pub async fn delete(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Path(id): Path<PersonId>,
) -> Result<StatusCode, Problem> {
    PersonRepo::new(&state.db).delete(&ctx, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct MergePersonsRequest {
    #[schema(value_type = Vec<String>)]
    pub absorbed: Vec<PersonId>,
}

/// Unisce `absorbed` in `id`: tutti i volti passano alla persona
/// sopravvissuta, le assorbite spariscono. Il nome sopravvissuto è quello
/// di `id` se presente, altrimenti il primo nome trovato fra gli assorbiti.
///
/// # Errors
/// Come [`get`], su `id` e su ogni persona assorbita.
#[utoipa::path(
    post,
    path = "/api/v1/persons/{id}/merge",
    tag = "persons",
    operation_id = "persons_merge",
    summary = "Merge other persons into this one",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Id persona sopravvissuta")),
    request_body = MergePersonsRequest,
    responses(
        (status = 200, description = "Persona risultante", body = PersonView),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Nessun volto visibile al chiamante", body = Problem),
        (status = 404, description = "Persona inesistente", body = Problem)
    )
)]
pub async fn merge(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Path(id): Path<PersonId>,
    Json(body): Json<MergePersonsRequest>,
) -> Result<Json<PersonView>, Problem> {
    let person = PersonRepo::new(&state.db)
        .merge(&ctx, id, &body.absorbed)
        .await?;
    Ok(Json(PersonView::from_person(&person)))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct SeparatePersonRequest {
    #[schema(value_type = Vec<String>)]
    pub face_ids: Vec<keeppix_domain::FaceId>,
    #[serde(default)]
    pub name: String,
}

/// Separa: i volti indicati lasciano `id` e formano una persona nuova.
/// **Non ripristina uno stato precedente** — l'utente non deve aspettarsi
/// un annullamento (documento funzionale, domanda aperta n.5).
///
/// # Errors
/// Come [`get`] su `id`. `409` se `face_ids` è vuoto o un volto non
/// appartiene a `id`. `422` se `name` non è vuoto ma è solo spazi.
#[utoipa::path(
    post,
    path = "/api/v1/persons/{id}/separate",
    tag = "persons",
    operation_id = "persons_separate",
    summary = "Split faces off into a new person (not undoable)",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Id persona sorgente")),
    request_body = SeparatePersonRequest,
    responses(
        (status = 201, description = "Persona nuova", body = PersonView),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Nessun volto visibile al chiamante", body = Problem),
        (status = 404, description = "Persona inesistente", body = Problem),
        (status = 409, description = "Nessun volto selezionato, o non appartiene alla sorgente", body = Problem),
        (status = 422, description = "Nome non valido", body = Problem)
    )
)]
pub async fn separate(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Path(id): Path<PersonId>,
    Json(body): Json<SeparatePersonRequest>,
) -> Result<(StatusCode, Json<PersonView>), Problem> {
    let name = parse_person_name(&body.name)?;
    let person = PersonRepo::new(&state.db)
        .separate(&ctx, id, &body.face_ids, name)
        .await?;
    Ok((StatusCode::CREATED, Json(PersonView::from_person(&person))))
}

// ---- Gruppi di persone (spec §5.1) ----

#[derive(Serialize, utoipa::ToSchema)]
pub struct PersonGroupView {
    pub id: String,
    pub name: String,
    pub created_by: String,
    pub created_at: String,
}

impl From<&PersonGroup> for PersonGroupView {
    fn from(g: &PersonGroup) -> Self {
        Self {
            id: g.id.to_string(),
            name: g.name.clone(),
            created_by: g.created_by.to_string(),
            created_at: g.created_at.to_rfc3339(),
        }
    }
}

/// # Errors
/// `401` senza utente autenticato.
#[utoipa::path(
    get,
    path = "/api/v1/person-groups",
    tag = "persons",
    operation_id = "person_groups_list",
    summary = "List person groups",
    security(("session_cookie" = [])),
    responses(
        (status = 200, description = "Gruppi", body = Vec<PersonGroupView>),
        (status = 401, description = "Non autenticato", body = Problem)
    )
)]
pub async fn list_groups(
    State(state): State<AppState>,
    Auth(ctx): Auth,
) -> Result<Json<Vec<PersonGroupView>>, Problem> {
    let groups = PersonGroupRepo::new(&state.db).list(&ctx).await?;
    Ok(Json(groups.iter().map(PersonGroupView::from).collect()))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreatePersonGroupRequest {
    pub name: String,
}

/// # Errors
/// `401` senza utente autenticato. `409` se il nome è già in uso.
#[utoipa::path(
    post,
    path = "/api/v1/person-groups",
    tag = "persons",
    operation_id = "person_groups_create",
    summary = "Create a person group",
    security(("session_cookie" = [])),
    request_body = CreatePersonGroupRequest,
    responses(
        (status = 201, description = "Gruppo creato", body = PersonGroupView),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 409, description = "Nome già in uso", body = Problem)
    )
)]
pub async fn create_group(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Json(body): Json<CreatePersonGroupRequest>,
) -> Result<(StatusCode, Json<PersonGroupView>), Problem> {
    let group = PersonGroupRepo::new(&state.db)
        .create(&ctx, NewPersonGroup { name: body.name })
        .await?;
    Ok((StatusCode::CREATED, Json(PersonGroupView::from(&group))))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct RenamePersonGroupRequest {
    pub name: String,
}

/// # Errors
/// `401` senza utente autenticato. `404` se il gruppo non esiste. `409` se
/// il nome è già in uso.
#[utoipa::path(
    patch,
    path = "/api/v1/person-groups/{id}",
    tag = "persons",
    operation_id = "person_groups_patch",
    summary = "Rename a person group",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Id gruppo")),
    request_body = RenamePersonGroupRequest,
    responses(
        (status = 200, description = "Gruppo aggiornato", body = PersonGroupView),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 404, description = "Gruppo inesistente", body = Problem),
        (status = 409, description = "Nome già in uso", body = Problem)
    )
)]
pub async fn patch_group(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Path(id): Path<PersonGroupId>,
    Json(body): Json<RenamePersonGroupRequest>,
) -> Result<Json<PersonGroupView>, Problem> {
    let group = PersonGroupRepo::new(&state.db)
        .rename(&ctx, id, &body.name)
        .await?;
    Ok(Json(PersonGroupView::from(&group)))
}

/// # Errors
/// `401` senza utente autenticato.
#[utoipa::path(
    delete,
    path = "/api/v1/person-groups/{id}",
    tag = "persons",
    operation_id = "person_groups_delete",
    summary = "Delete a person group",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Id gruppo")),
    responses(
        (status = 204, description = "Cancellato"),
        (status = 401, description = "Non autenticato", body = Problem)
    )
)]
pub async fn delete_group(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Path(id): Path<PersonGroupId>,
) -> Result<StatusCode, Problem> {
    PersonGroupRepo::new(&state.db).delete(&ctx, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// # Errors
/// `401` senza utente autenticato.
#[utoipa::path(
    get,
    path = "/api/v1/person-groups/{id}/members",
    tag = "persons",
    operation_id = "person_groups_list_members",
    summary = "List a group's members visible to the caller",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Id gruppo")),
    responses(
        (status = 200, description = "Id delle persone nel gruppo", body = Vec<String>),
        (status = 401, description = "Non autenticato", body = Problem)
    )
)]
pub async fn list_group_members(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Path(id): Path<PersonGroupId>,
) -> Result<Json<Vec<String>>, Problem> {
    let members = PersonGroupRepo::new(&state.db).members(&ctx, id).await?;
    Ok(Json(members.iter().map(PersonId::to_string).collect()))
}

/// # Errors
/// Come [`get`] sulla persona.
#[utoipa::path(
    post,
    path = "/api/v1/person-groups/{id}/members/{person_id}",
    tag = "persons",
    operation_id = "person_groups_add_member",
    summary = "Add a person to a group",
    security(("session_cookie" = [])),
    params(
        ("id" = String, Path, description = "Id gruppo"),
        ("person_id" = String, Path, description = "Id persona")
    ),
    responses(
        (status = 204, description = "Aggiunta"),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Persona non visibile al chiamante", body = Problem),
        (status = 404, description = "Persona inesistente", body = Problem)
    )
)]
pub async fn add_group_member(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Path((id, person_id)): Path<(PersonGroupId, PersonId)>,
) -> Result<StatusCode, Problem> {
    PersonGroupRepo::new(&state.db)
        .add_member(&ctx, id, person_id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// # Errors
/// `401` senza utente autenticato.
#[utoipa::path(
    delete,
    path = "/api/v1/person-groups/{id}/members/{person_id}",
    tag = "persons",
    operation_id = "person_groups_remove_member",
    summary = "Remove a person from a group",
    security(("session_cookie" = [])),
    params(
        ("id" = String, Path, description = "Id gruppo"),
        ("person_id" = String, Path, description = "Id persona")
    ),
    responses(
        (status = 204, description = "Rimossa"),
        (status = 401, description = "Non autenticato", body = Problem)
    )
)]
pub async fn remove_group_member(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Path((id, person_id)): Path<(PersonGroupId, PersonId)>,
) -> Result<StatusCode, Problem> {
    PersonGroupRepo::new(&state.db)
        .remove_member(&ctx, id, person_id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
