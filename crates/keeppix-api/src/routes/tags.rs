//! CRUD del vocabolario condiviso di tag e categorie (Fase 7 Task 7).
//!
//! Creare o modificare un **tag** (non una categoria) ricalcola solo
//! l'embedding testuale di quel tag — le foto non si toccano. Se l'embedding
//! è presente, Task 8 abbina subito le foto già indicizzate (`propose_for_tag`).
//! Cambiare solo soglia/colore/parent non rivaluta.
#![allow(clippy::missing_errors_doc)]

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use keeppix_db::{AssetTagRepo, NewTag, TagPatch, TagRepo};
use keeppix_domain::{AssetId, TagId, TagKind};
use keeppix_media::{MODEL_VERSION, MobileClip, first_complete_model_dir};
use serde::{Deserialize, Serialize};

use crate::bulk::BulkOutcome;
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

/// Una proposta di abbinamento IA in attesa di revisione umana (Fase 7
/// Task 9). Arricchita con nome tag e nome file: la coda di revisione non
/// deve fare un secondo giro per riga.
#[derive(Serialize, utoipa::ToSchema)]
pub struct ProposalView {
    pub asset_id: String,
    pub tag_id: String,
    pub tag_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<f32>,
    pub filename: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub taken_at_utc: Option<String>,
}

impl From<&keeppix_db::ProposalView> for ProposalView {
    fn from(p: &keeppix_db::ProposalView) -> Self {
        Self {
            asset_id: p.asset_id.to_string(),
            tag_id: p.tag_id.to_string(),
            tag_name: p.tag_name.clone(),
            score: p.score,
            filename: p.filename.clone(),
            taken_at_utc: p.taken_at_utc.map(|t| t.to_rfc3339()),
        }
    }
}

#[derive(Deserialize)]
pub struct ListProposalsQuery {
    #[serde(default)]
    pub tag_id: Option<uuid::Uuid>,
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

// ---------------------------------------------------------------------------
// Fase 7 Task 9: la coda di revisione delle proposte IA.
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/v1/tags/proposals",
    tag = "tags",
    operation_id = "tags_list_proposals",
    summary = "List pending AI tag proposals (review queue)",
    security(("session_cookie" = [])),
    params(
        ("tag_id" = Option<String>, Query, description = "Filtra su un solo tag")
    ),
    responses(
        (status = 200, description = "Proposte in attesa, visibili al chiamante, punteggio decrescente", body = [ProposalView]),
        (status = 401, description = "Non autenticato", body = Problem)
    )
)]
pub async fn list_proposals(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Query(query): Query<ListProposalsQuery>,
) -> Result<Json<Vec<ProposalView>>, Problem> {
    let tag_id = query.tag_id.map(TagId::from_uuid);
    let proposals = AssetTagRepo::new(&state.db)
        .list_proposed(&ctx, tag_id)
        .await?;
    Ok(Json(proposals.iter().map(ProposalView::from).collect()))
}

#[utoipa::path(
    post,
    path = "/api/v1/tags/{id}/assets/{asset_id}/confirm",
    tag = "tags",
    operation_id = "tags_confirm_proposal",
    summary = "Confirm a single AI tag proposal",
    security(("session_cookie" = [])),
    params(
        ("id" = String, Path, description = "Id tag"),
        ("asset_id" = String, Path, description = "Id asset")
    ),
    responses(
        (status = 204, description = "Confermato (idempotente se già confermato)"),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Asset non visibile al chiamante", body = Problem),
        (status = 404, description = "Nessuna proposta per questa coppia tag/asset", body = Problem),
        (status = 409, description = "Già rifiutato: la decisione è permanente", body = Problem)
    )
)]
pub async fn confirm_proposal(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Path((id, asset_id)): Path<(TagId, AssetId)>,
) -> Result<StatusCode, Problem> {
    AssetTagRepo::new(&state.db)
        .confirm(&ctx, id, asset_id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/api/v1/tags/{id}/assets/{asset_id}/reject",
    tag = "tags",
    operation_id = "tags_reject_proposal",
    summary = "Reject a single AI tag proposal (permanent)",
    security(("session_cookie" = [])),
    params(
        ("id" = String, Path, description = "Id tag"),
        ("asset_id" = String, Path, description = "Id asset")
    ),
    responses(
        (status = 204, description = "Rifiutato (idempotente se già rifiutato)"),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Asset non visibile al chiamante", body = Problem),
        (status = 404, description = "Nessuna proposta per questa coppia tag/asset", body = Problem),
        (status = 409, description = "Già confermato: la decisione è permanente", body = Problem)
    )
)]
pub async fn reject_proposal(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Path((id, asset_id)): Path<(TagId, AssetId)>,
) -> Result<StatusCode, Problem> {
    AssetTagRepo::new(&state.db)
        .reject(&ctx, id, asset_id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/api/v1/tags/{id}/proposals/confirm",
    tag = "tags",
    operation_id = "tags_confirm_all_proposals",
    summary = "Confirm all pending proposals for a tag (bulk, «Conferma tutte»)",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Id tag")),
    responses(
        (status = 200, description = "Esito: confermati gli asset visibili in attesa per questo tag", body = BulkOutcome),
        (status = 401, description = "Non autenticato", body = Problem)
    )
)]
pub async fn confirm_all_proposals(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Path(id): Path<TagId>,
) -> Result<Json<BulkOutcome>, Problem> {
    let confirmed = AssetTagRepo::new(&state.db)
        .confirm_all_for_tag(&ctx, id)
        .await?;
    Ok(Json(BulkOutcome::from_partition(confirmed, &[], None)))
}

#[utoipa::path(
    post,
    path = "/api/v1/tags/{id}/proposals/reject",
    tag = "tags",
    operation_id = "tags_reject_all_proposals",
    summary = "Reject all pending proposals for a tag (bulk, «Rifiuta tutte»)",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Id tag")),
    responses(
        (status = 200, description = "Esito: rifiutati gli asset visibili in attesa per questo tag", body = BulkOutcome),
        (status = 401, description = "Non autenticato", body = Problem)
    )
)]
pub async fn reject_all_proposals(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Path(id): Path<TagId>,
) -> Result<Json<BulkOutcome>, Problem> {
    let rejected = AssetTagRepo::new(&state.db)
        .reject_all_for_tag(&ctx, id)
        .await?;
    Ok(Json(BulkOutcome::from_partition(rejected, &[], None)))
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct BatchAssignRequest {
    #[schema(value_type = Vec<String>)]
    pub asset_ids: Vec<AssetId>,
}

/// Fase 11 Task 7 (§13.3 campo 5, "Aggiungi tag…"): un'aggiunta manuale è
/// già una conferma, non passa dalla coda di revisione (SP-12) — stessa
/// forma di [`confirm_all_proposals`], ma sull'insieme di asset che il
/// chiamante sceglie (la selezione corrente), non su "tutte le proposte in
/// attesa per questo tag". [`keeppix_db::AssetTagRepo::assign`] scrive
/// sempre `state='confirmed', source='user'`, anche sopra un rifiuto
/// precedente — a differenza di `confirm_proposal`.
#[utoipa::path(
    post,
    path = "/api/v1/tags/{id}/assets/batch",
    tag = "tags",
    operation_id = "tags_assign_batch",
    summary = "Assign a tag to multiple assets directly (source=user, bulk)",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Id tag")),
    request_body = BatchAssignRequest,
    responses(
        (status = 200, description = "Esito per asset (riuscita parziale ammessa)", body = BulkOutcome),
        (status = 400, description = "batch troppo grande", body = Problem),
        (status = 401, description = "Non autenticato", body = Problem)
    )
)]
pub async fn assign_batch(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Path(id): Path<TagId>,
    Json(body): Json<BatchAssignRequest>,
) -> Result<Json<BulkOutcome>, Problem> {
    crate::batch::reject_oversized_batch(&body.asset_ids)?;
    let repo = AssetTagRepo::new(&state.db);
    let mut succeeded = Vec::new();
    let mut failed = Vec::new();
    for asset_id in &body.asset_ids {
        match repo.assign(&ctx, id, *asset_id).await {
            Ok(()) => succeeded.push(*asset_id),
            Err(error) => failed.push((*asset_id, error)),
        }
    }
    Ok(Json(BulkOutcome::from_partition(succeeded, &failed, None)))
}

/// Fase 11 Task 7 (§13.3 campo 5, "Aggiungi tag…" — verificato sul
/// prototipo reale, `openTagPickerDialog` in `docs/ui/keeppix-mockup.html`:
/// lo stesso pulsante attiva/disattiva, aggiunge **o toglie**): la freccia
/// opposta di [`assign_batch`], stessa forma esatta. [`keeppix_db::
/// AssetTagRepo::unassign`] cancella la riga invece di deciderla
/// `'rejected'` — quello stato è la coda di revisione IA, permanente per
/// costruzione, semantica sbagliata per un tag manuale su cui si è
/// ripensato.
#[utoipa::path(
    post,
    path = "/api/v1/tags/{id}/assets/batch/remove",
    tag = "tags",
    operation_id = "tags_unassign_batch",
    summary = "Remove a tag from multiple assets directly (bulk)",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Id tag")),
    request_body = BatchAssignRequest,
    responses(
        (status = 200, description = "Esito per asset (riuscita parziale ammessa)", body = BulkOutcome),
        (status = 400, description = "batch troppo grande", body = Problem),
        (status = 401, description = "Non autenticato", body = Problem)
    )
)]
pub async fn unassign_batch(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Path(id): Path<TagId>,
    Json(body): Json<BatchAssignRequest>,
) -> Result<Json<BulkOutcome>, Problem> {
    crate::batch::reject_oversized_batch(&body.asset_ids)?;
    let repo = AssetTagRepo::new(&state.db);
    let mut succeeded = Vec::new();
    let mut failed = Vec::new();
    for asset_id in &body.asset_ids {
        match repo.unassign(&ctx, id, *asset_id).await {
            Ok(()) => succeeded.push(*asset_id),
            Err(error) => failed.push((*asset_id, error)),
        }
    }
    Ok(Json(BulkOutcome::from_partition(succeeded, &failed, None)))
}

/// Un tag come lo mostra il pannello informazioni del lightbox (Fase 11
/// Task 8, §19.2 campi 14-17) — `state`/`source` grezzi, la vista sceglie
/// la resa (piena / `.ai-applied` / tratteggiata), non questa risposta.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct AssetTagDetailView {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category_id: Option<String>,
    pub state: String,
    pub source: String,
}

impl From<keeppix_db::AssetTagDetail> for AssetTagDetailView {
    fn from(t: keeppix_db::AssetTagDetail) -> Self {
        Self {
            id: t.tag_id.to_string(),
            name: t.name,
            color: t.color,
            category_id: t.category_id.map(|id| id.to_string()),
            state: t.state,
            source: t.source,
        }
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/assets/{id}/tags",
    tag = "tags",
    operation_id = "assets_list_tags",
    summary = "List an asset's tags — confirmed and pending, never rejected",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Id asset")),
    responses(
        (status = 200, description = "Tag confermati e in attesa, ordinati per nome", body = Vec<AssetTagDetailView>),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Asset non visibile al chiamante", body = Problem)
    )
)]
pub async fn list_tags_for_asset(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Path(asset_id): Path<AssetId>,
) -> Result<Json<Vec<AssetTagDetailView>>, Problem> {
    let tags = AssetTagRepo::new(&state.db)
        .for_asset(&ctx, asset_id)
        .await?;
    Ok(Json(
        tags.into_iter().map(AssetTagDetailView::from).collect(),
    ))
}

/// Fase 11 Task 8 (§19.3, la `×` sui chip confermati): rimuove un tag già
/// confermato, permanentemente — non una `DELETE` come [`unassign_batch`]
/// (che serve solo l'aggiunta manuale di Modifica in blocco). Vedi
/// [`keeppix_db::AssetTagRepo::remove_confirmed`] per il perché della
/// transizione a `'rejected'` invece della cancellazione della riga.
#[utoipa::path(
    post,
    path = "/api/v1/tags/{id}/assets/{asset_id}/remove",
    tag = "tags",
    operation_id = "tags_remove_confirmed",
    summary = "Permanently remove an already-confirmed tag from an asset",
    security(("session_cookie" = [])),
    params(
        ("id" = String, Path, description = "Id tag"),
        ("asset_id" = String, Path, description = "Id asset")
    ),
    responses(
        (status = 204, description = "Rimosso (idempotente se già rimosso)"),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Asset non visibile al chiamante", body = Problem),
        (status = 404, description = "Il tag non è mai stato assegnato a questo asset", body = Problem),
        (status = 409, description = "È ancora in attesa di conferma: va deciso dalla coda, non rimosso", body = Problem)
    )
)]
pub async fn remove_confirmed_tag(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Path((id, asset_id)): Path<(TagId, AssetId)>,
) -> Result<StatusCode, Problem> {
    AssetTagRepo::new(&state.db)
        .remove_confirmed(&ctx, id, asset_id)
        .await?;
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
