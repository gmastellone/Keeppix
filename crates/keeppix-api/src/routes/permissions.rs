//! Pannello permessi: elenco, concessione, revoca, explain.
#![allow(clippy::missing_errors_doc)]

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use keeppix_db::{NewGrant, ObjectType, PermissionRepo, SubjectType};
use keeppix_domain::ObjectRole;
use serde::Deserialize;
use uuid::Uuid;

use crate::extract::Auth;
use crate::json::Json;
use crate::problem::Problem;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct ListQuery {
    pub object_type: String,
    pub object_id: Uuid,
}

#[derive(Deserialize)]
pub struct ExplainQuery {
    pub object_type: String,
    pub object_id: Uuid,
    pub user_id: Uuid,
}

#[derive(Deserialize)]
pub struct GrantRequest {
    pub subject_type: String,
    pub subject_id: Uuid,
    pub object_type: String,
    pub object_id: Uuid,
    pub role: String,
    #[serde(default = "default_inherit")]
    pub inherit: bool,
}

const fn default_inherit() -> bool {
    true
}

#[derive(Deserialize)]
pub struct PatchPermissionRequest {
    pub role: Option<String>,
    pub inherit: Option<bool>,
}

fn parse_object(raw: &str) -> Result<ObjectType, Problem> {
    match raw {
        "folder" => Ok(ObjectType::Folder),
        "album" => Ok(ObjectType::Album),
        "asset" => Ok(ObjectType::Asset),
        _ => Err(Problem::bad_request(
            "invalid-object-type",
            "Invalid object type",
        )),
    }
}

fn parse_subject(raw: &str) -> Result<SubjectType, Problem> {
    match raw {
        "user" => Ok(SubjectType::User),
        "group" => Ok(SubjectType::Group),
        _ => Err(Problem::bad_request(
            "invalid-subject-type",
            "Invalid subject type",
        )),
    }
}

fn parse_role(raw: &str) -> Result<ObjectRole, Problem> {
    ObjectRole::parse(raw)
        .ok_or_else(|| Problem::bad_request("invalid-role", "Role must be viewer or editor"))
}

pub async fn list(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<keeppix_db::PermissionGrantView>>, Problem> {
    let object = parse_object(&q.object_type)?;
    let rows = PermissionRepo::new(&state.db)
        .list_direct(&ctx, object, q.object_id)
        .await?;
    Ok(Json(rows))
}

pub async fn grant(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Json(body): Json<GrantRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), Problem> {
    let perm = PermissionRepo::new(&state.db)
        .grant(
            &ctx,
            NewGrant {
                subject: parse_subject(&body.subject_type)?,
                subject_id: body.subject_id,
                object: parse_object(&body.object_type)?,
                object_id: body.object_id,
                role: parse_role(&body.role)?,
                inherit: body.inherit,
            },
        )
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "id": perm.id,
            "role": perm.role.as_str(),
            "inherit": perm.inherit,
        })),
    ))
}

pub async fn patch(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Path(id): Path<Uuid>,
    Json(body): Json<PatchPermissionRequest>,
) -> Result<Json<serde_json::Value>, Problem> {
    let role = body.role.as_deref().map(parse_role).transpose()?;
    let perm = PermissionRepo::new(&state.db)
        .patch(&ctx, id, role, body.inherit)
        .await?;
    Ok(Json(serde_json::json!({
        "id": perm.id,
        "role": perm.role.as_str(),
        "inherit": perm.inherit,
    })))
}

pub async fn revoke(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, Problem> {
    PermissionRepo::new(&state.db).revoke(&ctx, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn explain(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Query(q): Query<ExplainQuery>,
) -> Result<Json<keeppix_db::ExplainResult>, Problem> {
    let object = parse_object(&q.object_type)?;
    let result = PermissionRepo::new(&state.db)
        .explain(&ctx, object, q.object_id, q.user_id)
        .await?;
    Ok(Json(result))
}

/// L'inverso di `list` (interrogabile solo per oggetto): tutto ciò che è
/// stato condiviso **con** l'utente corrente (§29 scheda "Condivisi con me").
pub async fn shared_with_me(
    State(state): State<AppState>,
    Auth(ctx): Auth,
) -> Result<Json<Vec<keeppix_db::SharedWithMeItem>>, Problem> {
    let items = PermissionRepo::new(&state.db)
        .list_shared_with_me(&ctx)
        .await?;
    Ok(Json(items))
}
