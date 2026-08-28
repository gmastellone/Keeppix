//! Group management. Admin only. No SQL: only `GroupRepo`.
#![allow(clippy::missing_errors_doc)]

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use keeppix_db::GroupRepo;
use keeppix_domain::{GroupId, UserId};
use serde::{Deserialize, Serialize};

use crate::extract::AdminAuth;
use crate::json::Json;
use crate::problem::Problem;
use crate::state::AppState;

#[derive(Serialize, utoipa::ToSchema)]
pub struct GroupView {
    pub id: String,
    pub name: String,
    pub created_at: String,
}

impl From<&keeppix_db::GroupView> for GroupView {
    fn from(g: &keeppix_db::GroupView) -> Self {
        Self {
            id: g.id.to_string(),
            name: g.name.clone(),
            created_at: g.created_at.to_rfc3339(),
        }
    }
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct GroupMemberView {
    pub user_id: String,
    pub added_at: String,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreateGroupRequest {
    pub name: String,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct PatchGroupRequest {
    pub name: String,
}

#[derive(Deserialize)]
pub struct DeleteGroupQuery {
    #[serde(default)]
    pub cascade: Option<bool>,
}

#[utoipa::path(
    get,
    path = "/api/v1/groups",
    tag = "groups",
    operation_id = "groups_list",
    summary = "List groups",
    security(("session_cookie" = [])),
    responses(
        (status = 200, description = "List of groups", body = [GroupView]),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "Admin only", body = Problem)
    )
)]
pub async fn list(
    State(state): State<AppState>,
    AdminAuth(ctx): AdminAuth,
) -> Result<Json<Vec<GroupView>>, Problem> {
    let groups = GroupRepo::new(&state.db).list(&ctx).await?;
    Ok(Json(groups.iter().map(GroupView::from).collect()))
}

#[utoipa::path(
    post,
    path = "/api/v1/groups",
    tag = "groups",
    operation_id = "groups_create",
    summary = "Create a group",
    security(("session_cookie" = [])),
    request_body = CreateGroupRequest,
    responses(
        (status = 201, description = "Group created", body = GroupView),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "Admin only", body = Problem),
        (status = 409, description = "Name already in use", body = Problem),
        (status = 422, description = "Invalid data", body = Problem)
    )
)]
pub async fn create(
    State(state): State<AppState>,
    AdminAuth(ctx): AdminAuth,
    Json(body): Json<CreateGroupRequest>,
) -> Result<(StatusCode, Json<GroupView>), Problem> {
    let name = body.name.trim();
    if name.is_empty() {
        return Err(Problem::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid-group-name",
            "Group name must not be empty",
        ));
    }
    let group = GroupRepo::new(&state.db).create(&ctx, name).await?;
    Ok((StatusCode::CREATED, Json(GroupView::from(&group))))
}

#[utoipa::path(
    patch,
    path = "/api/v1/groups/{id}",
    tag = "groups",
    operation_id = "groups_patch",
    summary = "Update a group",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Group id")),
    request_body = PatchGroupRequest,
    responses(
        (status = 200, description = "Group updated", body = GroupView),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "Admin only", body = Problem),
        (status = 404, description = "Group not found", body = Problem),
        (status = 409, description = "Name already in use", body = Problem)
    )
)]
pub async fn patch(
    State(state): State<AppState>,
    AdminAuth(ctx): AdminAuth,
    Path(id): Path<GroupId>,
    Json(body): Json<PatchGroupRequest>,
) -> Result<Json<GroupView>, Problem> {
    let name = body.name.trim();
    if name.is_empty() {
        return Err(Problem::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid-group-name",
            "Group name must not be empty",
        ));
    }
    let group = GroupRepo::new(&state.db).update(&ctx, id, name).await?;
    Ok(Json(GroupView::from(&group)))
}

#[utoipa::path(
    delete,
    path = "/api/v1/groups/{id}",
    tag = "groups",
    operation_id = "groups_delete",
    summary = "Delete a group",
    security(("session_cookie" = [])),
    params(
        ("id" = String, Path, description = "Group id"),
    ),
    responses(
        (status = 204, description = "Group deleted"),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "Admin only", body = Problem),
        (status = 404, description = "Group not found", body = Problem),
        (status = 409, description = "Group has active permissions", body = Problem)
    )
)]
pub async fn delete(
    State(state): State<AppState>,
    AdminAuth(ctx): AdminAuth,
    Path(id): Path<GroupId>,
    Query(query): Query<DeleteGroupQuery>,
) -> Result<StatusCode, Problem> {
    let cascade = query.cascade.unwrap_or(false);
    GroupRepo::new(&state.db).delete(&ctx, id, cascade).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get,
    path = "/api/v1/groups/{id}/members",
    tag = "groups",
    operation_id = "groups_list_members",
    summary = "List group members",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Group id")),
    responses(
        (status = 200, description = "List of members", body = [GroupMemberView]),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "Admin only", body = Problem),
        (status = 404, description = "Group not found", body = Problem)
    )
)]
pub async fn list_members(
    State(state): State<AppState>,
    AdminAuth(ctx): AdminAuth,
    Path(id): Path<GroupId>,
) -> Result<Json<Vec<GroupMemberView>>, Problem> {
    let members = GroupRepo::new(&state.db).list_members(&ctx, id).await?;
    Ok(Json(
        members
            .iter()
            .map(|m| GroupMemberView {
                user_id: m.user_id.to_string(),
                added_at: m.added_at.to_rfc3339(),
            })
            .collect(),
    ))
}

#[utoipa::path(
    post,
    path = "/api/v1/groups/{group_id}/members/{user_id}",
    tag = "groups",
    operation_id = "groups_add_member",
    summary = "Add a group member",
    security(("session_cookie" = [])),
    params(
        ("group_id" = String, Path, description = "Group id"),
        ("user_id" = String, Path, description = "User id"),
    ),
    responses(
        (status = 204, description = "Member added"),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "Admin only", body = Problem),
        (status = 404, description = "Group not found", body = Problem)
    )
)]
pub async fn add_member(
    State(state): State<AppState>,
    AdminAuth(ctx): AdminAuth,
    Path((group_id, user_id)): Path<(GroupId, UserId)>,
) -> Result<StatusCode, Problem> {
    GroupRepo::new(&state.db)
        .add_member(&ctx, group_id, user_id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    delete,
    path = "/api/v1/groups/{group_id}/members/{user_id}",
    tag = "groups",
    operation_id = "groups_remove_member",
    summary = "Remove a group member",
    security(("session_cookie" = [])),
    params(
        ("group_id" = String, Path, description = "Group id"),
        ("user_id" = String, Path, description = "User id"),
    ),
    responses(
        (status = 204, description = "Member removed"),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "Admin only", body = Problem),
        (status = 404, description = "Group not found", body = Problem)
    )
)]
pub async fn remove_member(
    State(state): State<AppState>,
    AdminAuth(ctx): AdminAuth,
    Path((group_id, user_id)): Path<(GroupId, UserId)>,
) -> Result<StatusCode, Problem> {
    GroupRepo::new(&state.db)
        .remove_member(&ctx, group_id, user_id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
