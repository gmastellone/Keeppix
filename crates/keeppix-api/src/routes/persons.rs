//! Persons and person groups. Distinct from `groups` (user permissions) —
//! see `keeppix_db::PersonGroupRepo`.
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
    /// Present for `GET /persons` (list); absent for single-person
    /// responses, which don't pay for a second query round trip to count.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub face_count: Option<i64>,
    /// A representative photo's `content_hash`/`thumbhash` — present only
    /// on the list response, same reasoning as `face_count`: computed in
    /// the same query as everything else in `PersonSummary`, not a
    /// per-person search the client would otherwise have to run itself
    /// (`PeopleView.vue`'s old one-search-per-card `loadCover`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cover_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cover_thumbhash: Option<String>,
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
            cover_hash: None,
            cover_thumbhash: None,
            created_at: p.created_at.to_rfc3339(),
        }
    }

    fn from_summary(s: &PersonSummary) -> Self {
        Self {
            face_count: Some(s.face_count),
            cover_hash: s.cover_hash.as_deref().map(super::timeline::hex_bytes),
            cover_thumbhash: s.cover_thumbhash.as_deref().map(super::timeline::hex_bytes),
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
/// `401` without an authenticated user.
#[utoipa::path(
    get,
    path = "/api/v1/persons",
    tag = "persons",
    operation_id = "persons_list",
    summary = "List persons visible to the caller",
    security(("session_cookie" = [])),
    params(("include_hidden" = Option<bool>, Query, description = "Include hidden persons")),
    responses(
        (status = 200, description = "Visible persons, with face count", body = Vec<PersonView>),
        (status = 401, description = "Not authenticated", body = Problem)
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
    /// Empty or absent: unnamed person ("Person 4"). Non-empty: validated
    /// name, cannot be whitespace only.
    #[serde(default)]
    pub name: String,
}

/// # Errors
/// `401` without an authenticated user (only for consistency with the other
/// routes: the repository does not require visibility to create). `409` if
/// the name is already in use. `422` if the name is non-empty but
/// whitespace only.
#[utoipa::path(
    post,
    path = "/api/v1/persons",
    tag = "persons",
    operation_id = "persons_create",
    summary = "Create a person (used for the review queue's «new person» outcome)",
    security(("session_cookie" = [])),
    request_body = CreatePersonRequest,
    responses(
        (status = 201, description = "Person created", body = PersonView),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 409, description = "Name already in use", body = Problem),
        (status = 422, description = "Invalid name", body = Problem)
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
/// `403` if no face of the person is visible to the caller. `404` if it
/// does not exist.
#[utoipa::path(
    get,
    path = "/api/v1/persons/{id}",
    tag = "persons",
    operation_id = "persons_get",
    summary = "Get a person",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Person id")),
    responses(
        (status = 200, description = "Person", body = PersonView),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "No face visible to the caller", body = Problem),
        (status = 404, description = "Person does not exist", body = Problem)
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
    /// Absent: name unchanged. Empty string: clears the name. Non-empty
    /// string: new name (rejected if whitespace only).
    #[serde(default)]
    pub name: Option<String>,
    pub hidden: Option<bool>,
    #[schema(value_type = Option<String>)]
    pub cover_face_id: Option<keeppix_domain::FaceId>,
}

/// # Errors
/// Same as [`get`]. `409` if the new name is already in use, or if
/// `cover_face_id` does not belong to this person.
#[utoipa::path(
    patch,
    path = "/api/v1/persons/{id}",
    tag = "persons",
    operation_id = "persons_patch",
    summary = "Rename, hide/show, or set the cover of a person",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Person id")),
    request_body = PatchPersonRequest,
    responses(
        (status = 200, description = "Person updated", body = PersonView),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "No face visible to the caller", body = Problem),
        (status = 404, description = "Person does not exist", body = Problem),
        (status = 409, description = "Name already in use, or cover face not of this person", body = Problem)
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

/// Deletes the person: their faces stay, ready to be re-grouped
/// (`person_id` goes back to `NULL`) — **does not** delete face data (that
/// is the per-library toggle).
///
/// # Errors
/// Same as [`get`].
#[utoipa::path(
    delete,
    path = "/api/v1/persons/{id}",
    tag = "persons",
    operation_id = "persons_delete",
    summary = "Delete a person (their faces stay, ready to be re-grouped)",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Person id")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "No face visible to the caller", body = Problem),
        (status = 404, description = "Person does not exist", body = Problem)
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

/// Merges `absorbed` into `id`: all their faces move to the surviving
/// person, and the absorbed ones disappear. The surviving name is `id`'s if
/// present, otherwise the first name found among the absorbed persons.
///
/// # Errors
/// Same as [`get`], on `id` and on every absorbed person.
#[utoipa::path(
    post,
    path = "/api/v1/persons/{id}/merge",
    tag = "persons",
    operation_id = "persons_merge",
    summary = "Merge other persons into this one",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Surviving person id")),
    request_body = MergePersonsRequest,
    responses(
        (status = 200, description = "Resulting person", body = PersonView),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "No face visible to the caller", body = Problem),
        (status = 404, description = "Person does not exist", body = Problem)
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

/// Splits: the given faces leave `id` and form a new person. **Does not
/// restore a previous state** — the user should not expect an undo.
///
/// # Errors
/// Same as [`get`] on `id`. `409` if `face_ids` is empty or a face does not
/// belong to `id`. `422` if `name` is non-empty but whitespace only.
#[utoipa::path(
    post,
    path = "/api/v1/persons/{id}/separate",
    tag = "persons",
    operation_id = "persons_separate",
    summary = "Split faces off into a new person (not undoable)",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Source person id")),
    request_body = SeparatePersonRequest,
    responses(
        (status = 201, description = "New person", body = PersonView),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "No face visible to the caller", body = Problem),
        (status = 404, description = "Person does not exist", body = Problem),
        (status = 409, description = "No face selected, or it does not belong to the source", body = Problem),
        (status = 422, description = "Invalid name", body = Problem)
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

// ---- Person groups ----

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
/// `401` without an authenticated user.
#[utoipa::path(
    get,
    path = "/api/v1/person-groups",
    tag = "persons",
    operation_id = "person_groups_list",
    summary = "List person groups",
    security(("session_cookie" = [])),
    responses(
        (status = 200, description = "Groups", body = Vec<PersonGroupView>),
        (status = 401, description = "Not authenticated", body = Problem)
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
/// `401` without an authenticated user. `409` if the name is already in
/// use.
#[utoipa::path(
    post,
    path = "/api/v1/person-groups",
    tag = "persons",
    operation_id = "person_groups_create",
    summary = "Create a person group",
    security(("session_cookie" = [])),
    request_body = CreatePersonGroupRequest,
    responses(
        (status = 201, description = "Group created", body = PersonGroupView),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 409, description = "Name already in use", body = Problem)
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
/// `401` without an authenticated user. `404` if the group does not exist.
/// `409` if the name is already in use.
#[utoipa::path(
    patch,
    path = "/api/v1/person-groups/{id}",
    tag = "persons",
    operation_id = "person_groups_patch",
    summary = "Rename a person group",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Group id")),
    request_body = RenamePersonGroupRequest,
    responses(
        (status = 200, description = "Group updated", body = PersonGroupView),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 404, description = "Group does not exist", body = Problem),
        (status = 409, description = "Name already in use", body = Problem)
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
/// `401` without an authenticated user.
#[utoipa::path(
    delete,
    path = "/api/v1/person-groups/{id}",
    tag = "persons",
    operation_id = "person_groups_delete",
    summary = "Delete a person group",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Group id")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 401, description = "Not authenticated", body = Problem)
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
/// `401` without an authenticated user.
#[utoipa::path(
    get,
    path = "/api/v1/person-groups/{id}/members",
    tag = "persons",
    operation_id = "person_groups_list_members",
    summary = "List a group's members visible to the caller",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Group id")),
    responses(
        (status = 200, description = "Ids of the persons in the group", body = Vec<String>),
        (status = 401, description = "Not authenticated", body = Problem)
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
/// Same as [`get`] on the person.
#[utoipa::path(
    post,
    path = "/api/v1/person-groups/{id}/members/{person_id}",
    tag = "persons",
    operation_id = "person_groups_add_member",
    summary = "Add a person to a group",
    security(("session_cookie" = [])),
    params(
        ("id" = String, Path, description = "Group id"),
        ("person_id" = String, Path, description = "Person id")
    ),
    responses(
        (status = 204, description = "Added"),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "Person not visible to the caller", body = Problem),
        (status = 404, description = "Person does not exist", body = Problem)
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
/// `401` without an authenticated user.
#[utoipa::path(
    delete,
    path = "/api/v1/person-groups/{id}/members/{person_id}",
    tag = "persons",
    operation_id = "person_groups_remove_member",
    summary = "Remove a person from a group",
    security(("session_cookie" = [])),
    params(
        ("id" = String, Path, description = "Group id"),
        ("person_id" = String, Path, description = "Person id")
    ),
    responses(
        (status = 204, description = "Removed"),
        (status = 401, description = "Not authenticated", body = Problem)
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
