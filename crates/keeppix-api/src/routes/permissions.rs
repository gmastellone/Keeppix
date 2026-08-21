//! Pannello permessi: elenco, concessione, revoca, explain.
#![allow(clippy::missing_errors_doc)]

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use keeppix_db::{NewGrant, ObjectType, PermissionRepo, SubjectType};
use keeppix_domain::ObjectRole;
use serde::{Deserialize, Serialize};
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

#[derive(Deserialize, utoipa::ToSchema)]
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

#[derive(Deserialize, utoipa::ToSchema)]
pub struct PatchPermissionRequest {
    pub role: Option<String>,
    pub inherit: Option<bool>,
}

/// Voce di `GET /permissions` — rimpiazza il `keeppix_db::PermissionGrantView`
/// servito prima direttamente: stesse chiavi JSON (`id`/`subject_id` restano
/// stringhe, non cambia nulla sul filo), ma con uno schema `utoipa` invece
/// di doverne aggiungere uno a `keeppix-db`, che non conosce l'HTTP.
#[derive(Serialize, utoipa::ToSchema)]
pub struct PermissionGrantView {
    pub id: String,
    pub subject_type: String,
    pub subject_id: String,
    #[schema(value_type = String, example = "viewer")]
    pub role: ObjectRole,
    pub inherit: bool,
    pub inherited: bool,
}

impl From<keeppix_db::PermissionGrantView> for PermissionGrantView {
    fn from(g: keeppix_db::PermissionGrantView) -> Self {
        Self {
            id: g.id.to_string(),
            subject_type: g.subject_type,
            subject_id: g.subject_id.to_string(),
            role: g.role,
            inherit: g.inherit,
            inherited: g.inherited,
        }
    }
}

/// Corpo di risposta di `POST /permissions` e `PATCH /permissions/{id}`:
/// stesse tre chiavi che prima uscivano da un `serde_json::json!(...)` ad
/// hoc, ora con un tipo che `utoipa` può descrivere.
#[derive(Serialize, utoipa::ToSchema)]
pub struct GrantSummaryView {
    pub id: Uuid,
    #[schema(value_type = String, example = "viewer")]
    pub role: ObjectRole,
    pub inherit: bool,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct ExplainView {
    pub granted: bool,
    pub chain: Vec<ExplainChainLinkView>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct ExplainChainLinkView {
    pub subject_type: String,
    pub subject_name: String,
    pub role: String,
    pub granted_on_type: String,
    pub granted_on_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inherited_in: Option<String>,
}

impl From<keeppix_db::ExplainResult> for ExplainView {
    fn from(r: keeppix_db::ExplainResult) -> Self {
        Self {
            granted: r.granted,
            chain: r
                .chain
                .into_iter()
                .map(ExplainChainLinkView::from)
                .collect(),
        }
    }
}

impl From<keeppix_db::permissions::ExplainChainLink> for ExplainChainLinkView {
    fn from(l: keeppix_db::permissions::ExplainChainLink) -> Self {
        Self {
            subject_type: l.subject_type,
            subject_name: l.subject_name,
            role: l.role,
            granted_on_type: l.granted_on_type,
            granted_on_name: l.granted_on_name,
            inherited_in: l.inherited_in,
        }
    }
}

/// Voce di `GET /shared-with-me` (spec §29, "Condivisi con me"). Stessa
/// osservazione di [`PermissionGrantView`]: solo lo schema cambia sede.
#[derive(Serialize, utoipa::ToSchema)]
pub struct SharedWithMeView {
    pub object_type: String,
    pub object_id: String,
    pub name: String,
    pub owner_name: String,
    #[schema(value_type = String, example = "viewer")]
    pub role: ObjectRole,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub via_group: Option<String>,
    pub item_count: i64,
}

impl From<keeppix_db::SharedWithMeItem> for SharedWithMeView {
    fn from(i: keeppix_db::SharedWithMeItem) -> Self {
        Self {
            object_type: i.object_type,
            object_id: i.object_id.to_string(),
            name: i.name,
            owner_name: i.owner_name,
            role: i.role,
            via_group: i.via_group,
            item_count: i.item_count,
        }
    }
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

#[utoipa::path(
    get,
    path = "/api/v1/permissions",
    tag = "permissions",
    operation_id = "permissions_list",
    summary = "List direct permission grants on an object",
    security(("session_cookie" = [])),
    params(
        ("object_type" = String, Query, description = "folder, album o asset"),
        ("object_id" = String, Query, description = "Id dell'oggetto")
    ),
    responses(
        (status = 200, description = "Permessi diretti sull'oggetto", body = Vec<PermissionGrantView>),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Non owner/admin dell'oggetto", body = Problem)
    )
)]
pub async fn list(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<PermissionGrantView>>, Problem> {
    let object = parse_object(&q.object_type)?;
    let rows = PermissionRepo::new(&state.db)
        .list_direct(&ctx, object, q.object_id)
        .await?;
    Ok(Json(
        rows.into_iter().map(PermissionGrantView::from).collect(),
    ))
}

#[utoipa::path(
    post,
    path = "/api/v1/permissions",
    tag = "permissions",
    operation_id = "permissions_grant",
    summary = "Grant a permission",
    security(("session_cookie" = [])),
    request_body = GrantRequest,
    responses(
        (status = 201, description = "Permesso concesso", body = GrantSummaryView),
        (status = 400, description = "Tipo oggetto/soggetto o ruolo non valido", body = Problem),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Non owner/admin dell'oggetto", body = Problem)
    )
)]
pub async fn grant(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Json(body): Json<GrantRequest>,
) -> Result<(StatusCode, Json<GrantSummaryView>), Problem> {
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
        Json(GrantSummaryView {
            id: perm.id,
            role: perm.role,
            inherit: perm.inherit,
        }),
    ))
}

#[utoipa::path(
    patch,
    path = "/api/v1/permissions/{id}",
    tag = "permissions",
    operation_id = "permissions_patch",
    summary = "Update a permission grant",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Id del permesso")),
    request_body = PatchPermissionRequest,
    responses(
        (status = 200, description = "Permesso aggiornato", body = GrantSummaryView),
        (status = 400, description = "Ruolo non valido", body = Problem),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Non owner/admin dell'oggetto", body = Problem)
    )
)]
pub async fn patch(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Path(id): Path<Uuid>,
    Json(body): Json<PatchPermissionRequest>,
) -> Result<Json<GrantSummaryView>, Problem> {
    let role = body.role.as_deref().map(parse_role).transpose()?;
    let perm = PermissionRepo::new(&state.db)
        .patch(&ctx, id, role, body.inherit)
        .await?;
    Ok(Json(GrantSummaryView {
        id: perm.id,
        role: perm.role,
        inherit: perm.inherit,
    }))
}

#[utoipa::path(
    delete,
    path = "/api/v1/permissions/{id}",
    tag = "permissions",
    operation_id = "permissions_revoke",
    summary = "Revoke a permission grant",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Id del permesso")),
    responses(
        (status = 204, description = "Permesso revocato"),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Non owner/admin dell'oggetto", body = Problem)
    )
)]
pub async fn revoke(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, Problem> {
    PermissionRepo::new(&state.db).revoke(&ctx, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get,
    path = "/api/v1/permissions/explain",
    tag = "permissions",
    operation_id = "permissions_explain",
    summary = "Explain why a user has (or lacks) access to an object",
    security(("session_cookie" = [])),
    params(
        ("object_type" = String, Query, description = "folder, album o asset"),
        ("object_id" = String, Query, description = "Id dell'oggetto"),
        ("user_id" = String, Query, description = "Id dell'utente da spiegare")
    ),
    responses(
        (status = 200, description = "Catena di permessi che porta (o non porta) all'accesso", body = ExplainView),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Non owner/admin dell'oggetto", body = Problem)
    )
)]
pub async fn explain(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Query(q): Query<ExplainQuery>,
) -> Result<Json<ExplainView>, Problem> {
    let object = parse_object(&q.object_type)?;
    let result = PermissionRepo::new(&state.db)
        .explain(&ctx, object, q.object_id, q.user_id)
        .await?;
    Ok(Json(ExplainView::from(result)))
}

/// L'inverso di `list` (interrogabile solo per oggetto): tutto ciò che è
/// stato condiviso **con** l'utente corrente (§29 scheda "Condivisi con me").
///
/// # Errors
/// `401` se non autenticato.
#[utoipa::path(
    get,
    path = "/api/v1/shared-with-me",
    tag = "permissions",
    operation_id = "permissions_shared_with_me",
    summary = "List objects shared with the caller",
    security(("session_cookie" = [])),
    responses(
        (status = 200, description = "Oggetti condivisi con il chiamante", body = Vec<SharedWithMeView>),
        (status = 401, description = "Non autenticato", body = Problem)
    )
)]
pub async fn shared_with_me(
    State(state): State<AppState>,
    Auth(ctx): Auth,
) -> Result<Json<Vec<SharedWithMeView>>, Problem> {
    let items = PermissionRepo::new(&state.db)
        .list_shared_with_me(&ctx)
        .await?;
    Ok(Json(
        items.into_iter().map(SharedWithMeView::from).collect(),
    ))
}
