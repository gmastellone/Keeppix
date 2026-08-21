//! CRUD del vocabolario condiviso di tag e categorie (Fase 7 Task 7).
//!
//! Creare o modificare un **tag** (non una categoria) ricalcola solo
//! l'embedding testuale di quel tag — le foto non si toccano. Se l'embedding
//! è presente, Task 8 abbina subito le foto già indicizzate (`propose_for_tag`).
//! Cambiare solo soglia/colore/parent non rivaluta.
#![allow(clippy::missing_errors_doc)]

use axum::extract::{Path, State};
use axum::http::StatusCode;
use keeppix_db::{AssetTagRepo, NewTag, TagPatch, TagRepo};
use keeppix_domain::{TagId, TagKind};
use keeppix_media::{MODEL_VERSION, MobileClip, first_complete_model_dir};
use serde::{Deserialize, Serialize};

use crate::extract::Auth;
use crate::json::Json;
use crate::problem::Problem;
use crate::state::AppState;

#[derive(Serialize, utoipa::ToSchema)]
pub struct TagView {
    pub id: String,
    pub name: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    pub threshold: f32,
    pub has_embedding: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_version: Option<String>,
    pub created_by: String,
    pub created_at: String,
    /// Quante righe in `asset_tags` verrebbero cancellate con il tag.
    /// Il dialog di conferma in UI lo legge da qui prima del DELETE.
    pub assignment_count: i64,
}

impl From<&keeppix_db::TagView> for TagView {
    fn from(t: &keeppix_db::TagView) -> Self {
        Self {
            id: t.id.to_string(),
            name: t.name.clone(),
            kind: t.kind.as_str().to_owned(),
            parent_id: t.parent_id.map(|id| id.to_string()),
            prompt: t.prompt.clone(),
            color: t.color.clone(),
            threshold: t.threshold,
            has_embedding: t.has_embedding,
            model_version: t.model_version.clone(),
            created_by: t.created_by.to_string(),
            created_at: t.created_at.to_rfc3339(),
            assignment_count: t.assignment_count,
        }
    }
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreateTagRequest {
    pub name: String,
    /// `"tag"` o `"category"`.
    pub kind: String,
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub prompt: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub threshold: Option<f32>,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct PatchTagRequest {
    #[serde(default)]
    pub name: Option<String>,
    /// Assente = invariato; `null` = stacca dal parent.
    #[serde(default)]
    #[allow(clippy::option_option)]
    pub parent_id: Option<Option<String>>,
    #[serde(default)]
    #[allow(clippy::option_option)]
    pub prompt: Option<Option<String>>,
    #[serde(default)]
    #[allow(clippy::option_option)]
    pub color: Option<Option<String>>,
    #[serde(default)]
    pub threshold: Option<f32>,
}

#[utoipa::path(
    get,
    path = "/api/v1/tags",
    tag = "tags",
    operation_id = "tags_list",
    summary = "List tags and categories",
    security(("session_cookie" = [])),
    responses(
        (status = 200, description = "Vocabolario condiviso", body = [TagView]),
        (status = 401, description = "Non autenticato", body = Problem)
    )
)]
pub async fn list(
    State(state): State<AppState>,
    Auth(ctx): Auth,
) -> Result<Json<Vec<TagView>>, Problem> {
    let tags = TagRepo::new(&state.db).list(&ctx).await?;
    Ok(Json(tags.iter().map(TagView::from).collect()))
}

#[utoipa::path(
    post,
    path = "/api/v1/tags",
    tag = "tags",
    operation_id = "tags_create",
    summary = "Create a tag or category",
    security(("session_cookie" = [])),
    request_body = CreateTagRequest,
    responses(
        (status = 201, description = "Creato", body = TagView),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Parent assente", body = Problem),
        (status = 409, description = "Nome già in uso o nesting illegale", body = Problem),
        (status = 422, description = "Dati non validi", body = Problem)
    )
)]
pub async fn create(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Json(body): Json<CreateTagRequest>,
) -> Result<(StatusCode, Json<TagView>), Problem> {
    let name = body.name.trim();
    if name.is_empty() {
        return Err(Problem::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid-tag-name",
            "Tag name must not be empty",
        ));
    }
    let kind = parse_kind(&body.kind)?;
    let parent_id = parse_optional_tag_id(body.parent_id.as_deref())?;
    let prompt = body
        .prompt
        .as_ref()
        .map(|p| p.trim().to_owned())
        .filter(|p| !p.is_empty());

    let (embedding, model_version) = if kind == TagKind::Tag {
        let text = prompt.as_deref().unwrap_or(name);
        embed_tag_text(text).await?
    } else {
        (None, None)
    };

    let tag = TagRepo::new(&state.db)
        .create(
            &ctx,
            NewTag {
                name: name.to_owned(),
                kind,
                parent_id,
                prompt,
                color: body.color,
                threshold: body.threshold,
                embedding,
                model_version,
            },
        )
        .await?;
    // Task 8: un tag appena creato con embedding abbina subito le foto già
    // indicizzate (una query, nessuna re-inferenza sulle immagini).
    if tag.has_embedding {
        AssetTagRepo::new(&state.db).propose_for_tag(tag.id).await?;
    }
    Ok((StatusCode::CREATED, Json(TagView::from(&tag))))
}

#[utoipa::path(
    get,
    path = "/api/v1/tags/{id}",
    tag = "tags",
    operation_id = "tags_get",
    summary = "Get a tag or category",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Id tag")),
    responses(
        (status = 200, description = "Trovato", body = TagView),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Assente o non accessibile", body = Problem)
    )
)]
pub async fn get(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Path(id): Path<TagId>,
) -> Result<Json<TagView>, Problem> {
    let tag = TagRepo::new(&state.db).get(&ctx, id).await?;
    Ok(Json(TagView::from(&tag)))
}

#[utoipa::path(
    patch,
    path = "/api/v1/tags/{id}",
    tag = "tags",
    operation_id = "tags_patch",
    summary = "Update a tag or category",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Id tag")),
    request_body = PatchTagRequest,
    responses(
        (status = 200, description = "Aggiornato", body = TagView),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Assente o non accessibile", body = Problem),
        (status = 409, description = "Nome già in uso o nesting illegale", body = Problem),
        (status = 422, description = "Dati non validi", body = Problem)
    )
)]
pub async fn patch(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Path(id): Path<TagId>,
    Json(body): Json<PatchTagRequest>,
) -> Result<Json<TagView>, Problem> {
    let repo = TagRepo::new(&state.db);
    let current = repo.get(&ctx, id).await?;

    let name = match &body.name {
        Some(n) => {
            let trimmed = n.trim();
            if trimmed.is_empty() {
                return Err(Problem::new(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "invalid-tag-name",
                    "Tag name must not be empty",
                ));
            }
            Some(trimmed.to_owned())
        }
        None => None,
    };

    let parent_id = match body.parent_id {
        None => None,
        Some(None) => Some(None),
        Some(Some(ref s)) => Some(Some(parse_tag_id(s)?)),
    };

    let prompt = match body.prompt {
        None => None,
        Some(None) => Some(None),
        Some(Some(ref p)) => {
            let trimmed = p.trim();
            if trimmed.is_empty() {
                Some(None)
            } else {
                Some(Some(trimmed.to_owned()))
            }
        }
    };

    let text_changed = name.is_some() || prompt.is_some();
    let (embedding, model_version) = if current.kind == TagKind::Tag && text_changed {
        let effective_name = name.as_deref().unwrap_or(current.name.as_str());
        let effective_prompt = match &prompt {
            Some(v) => v.as_deref(),
            None => current.prompt.as_deref(),
        };
        let text = effective_prompt.unwrap_or(effective_name);
        match embed_tag_text(text).await? {
            (Some(vector), Some(version)) => (Some(Some(vector)), Some(Some(version))),
            // Pesi assenti: azzera il vettore così non resta un embedding
            // allineato al prompt vecchio.
            _ => (Some(None), Some(None)),
        }
    } else {
        (None, None)
    };

    // Solo un cambio di testo (name/prompt) rivaluta: threshold/color/parent
    // da soli NON rematchano — la soglia governa le analisi future.
    let rematch = matches!(embedding, Some(Some(_)));

    let tag = repo
        .update(
            &ctx,
            id,
            TagPatch {
                name,
                parent_id,
                prompt,
                color: body.color,
                threshold: body.threshold,
                embedding,
                model_version,
            },
        )
        .await?;
    if rematch {
        AssetTagRepo::new(&state.db).propose_for_tag(tag.id).await?;
    }
    Ok(Json(TagView::from(&tag)))
}

#[utoipa::path(
    delete,
    path = "/api/v1/tags/{id}",
    tag = "tags",
    operation_id = "tags_delete",
    summary = "Delete a tag or category",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Id tag")),
    responses(
        (status = 204, description = "Cancellato (cascade su asset_tags)"),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Assente o non accessibile", body = Problem)
    )
)]
pub async fn delete(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Path(id): Path<TagId>,
) -> Result<StatusCode, Problem> {
    TagRepo::new(&state.db).delete(&ctx, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

fn parse_kind(raw: &str) -> Result<TagKind, Problem> {
    raw.parse().map_err(|_| {
        Problem::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid-tag-kind",
            "kind must be \"tag\" or \"category\"",
        )
    })
}

fn parse_tag_id(raw: &str) -> Result<TagId, Problem> {
    raw.parse().map_err(|_| {
        Problem::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid-tag-id",
            "parent_id must be a UUID",
        )
    })
}

fn parse_optional_tag_id(raw: Option<&str>) -> Result<Option<TagId>, Problem> {
    raw.map(parse_tag_id).transpose()
}

/// Inferenza testuale usa-e-getta. Se i pesi mancano, `Ok((None, None))` —
/// il tag resta creato e l'abbinamento (Task 8) lo salterà finché non c'è
/// un embedding.
async fn embed_tag_text(text: &str) -> Result<(Option<Vec<f32>>, Option<String>), Problem> {
    let Some(model_dir) = first_complete_model_dir() else {
        return Ok((None, None));
    };
    let text = text.to_owned();
    let embedding = tokio::task::spawn_blocking(move || -> Result<Vec<f32>, String> {
        let mut clip = MobileClip::load(&model_dir)?;
        clip.embed_text(&text)
    })
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "tag embed task join failed");
        Problem::internal()
    })?
    .map_err(|e| {
        tracing::error!(error = %e, "tag text embedding failed");
        Problem::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "ai-unavailable",
            "Text embedding failed",
        )
        .with_detail(e)
    })?;
    Ok((Some(embedding), Some(MODEL_VERSION.to_owned())))
}
