//! Link pubblici e accesso tramite token.
#![allow(clippy::missing_errors_doc, clippy::struct_excessive_bools)]

use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum_extra::extract::CookieJar;
use keeppix_db::{
    AlbumRepo, AssetRepo, AuditRepo, FolderRepo, GuestUploadRepo, HomeRepo, JobRepo, NewShareLink,
    ShareLinkRepo,
};
use keeppix_domain::{
    Actor, AlbumId, AssetId, AssetName, AssetStatus, FolderId, JobKind, JobPriority, NewAsset,
    Password, PasswordHash, ShareToken, hash_password, verify_password,
};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt as _;

use crate::cookie::{share_link_cookie, share_unlock_cookie};
use crate::extract::{Auth, ShareAuth};
use crate::json::Json;
use crate::problem::Problem;
use crate::routes::metadata::GeoPointView;
use crate::routes::timeline::AssetView;
use crate::state::AppState;

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreateLinkRequest {
    pub object_type: String,
    #[schema(value_type = String)]
    pub object_id: uuid::Uuid,
    pub password: Option<String>,
    #[schema(value_type = Option<String>)]
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub max_views: Option<i32>,
    #[serde(default = "default_true")]
    pub allow_download: bool,
    #[serde(default)]
    pub allow_original: bool,
    #[serde(default)]
    pub allow_upload: bool,
    #[serde(default)]
    pub allow_cdn_cache: bool,
    /// When true, the public asset list omits capture dates and coordinates.
    /// The field name is kept because `/api/v1` is frozen.
    #[serde(default = "default_true")]
    pub hide_metadata: bool,
    pub upload_quota_bytes: Option<i64>,
}

const fn default_true() -> bool {
    true
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct CreateLinkResponse {
    pub id: String,
    pub token: String,
}

/// # Errors
/// `401` se non autenticato; `403` se non può condividere l'oggetto;
/// `422` se `password` non è valida.
#[utoipa::path(
    post,
    path = "/api/v1/share/links",
    tag = "share",
    operation_id = "share_links_create",
    summary = "Create a public share link",
    security(("session_cookie" = [])),
    request_body = CreateLinkRequest,
    responses(
        (status = 201, description = "Link creato", body = CreateLinkResponse),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Non owner/admin dell'oggetto", body = Problem),
        (status = 422, description = "Password non valida", body = Problem)
    )
)]
pub async fn create_link(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Json(req): Json<CreateLinkRequest>,
) -> Result<impl IntoResponse, Problem> {
    let password_hash = if let Some(pw) = req.password {
        let parsed = Password::parse_owned(pw).map_err(|_| {
            Problem::new(
                StatusCode::UNPROCESSABLE_ENTITY,
                "invalid-password",
                "Invalid password",
            )
        })?;
        Some(
            hash_password(&parsed)
                .map_err(|_| Problem::internal())?
                .as_str()
                .to_owned(),
        )
    } else {
        None
    };

    let link = NewShareLink {
        object_type: req.object_type,
        object_id: req.object_id,
        password_hash,
        expires_at: req.expires_at,
        max_views: req.max_views,
        allow_download: req.allow_download,
        allow_original: req.allow_original,
        allow_upload: req.allow_upload,
        allow_cdn_cache: req.allow_cdn_cache,
        hide_metadata: req.hide_metadata,
        upload_quota_bytes: req.upload_quota_bytes,
    };

    let (id, token) = ShareLinkRepo::new(&state.db).create(&ctx, link).await?;

    let _ = AuditRepo::new(&state.db)
        .append(
            ctx.user_id().map(|u| u.as_uuid()),
            "user",
            "share_link.create",
            Some("share_link"),
            Some(id),
            None,
            None,
        )
        .await;

    Ok((
        StatusCode::CREATED,
        Json(CreateLinkResponse {
            id: id.to_string(),
            token: token.as_str().to_owned(),
        }),
    ))
}

/// # Errors
/// `401` se non autenticato; `403` se non owner/admin del link.
#[utoipa::path(
    delete,
    path = "/api/v1/share/links/{id}",
    tag = "share",
    operation_id = "share_links_revoke",
    summary = "Revoke a public share link",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Id del link")),
    responses(
        (status = 204, description = "Link revocato"),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Non owner/admin del link", body = Problem)
    )
)]
pub async fn revoke_link(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Path(id): Path<uuid::Uuid>,
) -> Result<StatusCode, Problem> {
    ShareLinkRepo::new(&state.db).revoke(&ctx, id).await?;
    let _ = AuditRepo::new(&state.db)
        .append(
            ctx.user_id().map(|u| u.as_uuid()),
            "user",
            "share_link.revoke",
            Some("share_link"),
            Some(id),
            None,
            None,
        )
        .await;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct LinkView {
    pub id: String,
    pub object_type: String,
    pub object_id: String,
    pub has_password: bool,
    pub expires_at: Option<String>,
    pub max_views: Option<i32>,
    /// Accessi al link, non «elementi condivisi» — quello è `item_count`.
    pub view_count: i32,
    /// Elementi condivisi da questo link (§29: `"246 elementi"`). Task 11
    /// lo aveva tolto come inutilizzato ovunque; qui torna perché la scheda
    /// "Le mie condivisioni" lo chiede esplicitamente per i link pubblici.
    pub item_count: i64,
    pub allow_download: bool,
    pub allow_original: bool,
    pub allow_upload: bool,
    pub hide_metadata: bool,
    pub revoked_at: Option<String>,
    pub last_accessed_at: Option<String>,
    pub created_at: String,
}

/// # Errors
/// `401` se non autenticato.
#[utoipa::path(
    get,
    path = "/api/v1/share/links",
    tag = "share",
    operation_id = "share_links_list",
    summary = "List share links created by the caller",
    security(("session_cookie" = [])),
    responses(
        (status = 200, description = "Link creati dal chiamante", body = Vec<LinkView>),
        (status = 401, description = "Non autenticato", body = Problem)
    )
)]
pub async fn list_links(
    State(state): State<AppState>,
    Auth(ctx): Auth,
) -> Result<Json<Vec<LinkView>>, Problem> {
    let repo = ShareLinkRepo::new(&state.db);
    let rows = repo.list_by_creator(&ctx, 100, 0).await?;
    let item_counts = repo.item_counts(&rows).await?;

    let views = rows
        .into_iter()
        .map(|r| {
            let item_count = item_counts.get(&r.object_id).copied().unwrap_or(0);
            LinkView {
                id: r.id.to_string(),
                object_type: r.object_type,
                object_id: r.object_id.to_string(),
                has_password: r.password_hash.is_some(),
                expires_at: r.expires_at.map(|t| t.to_rfc3339()),
                max_views: r.max_views,
                view_count: r.view_count,
                item_count,
                allow_download: r.allow_download,
                allow_original: r.allow_original,
                allow_upload: r.allow_upload,
                hide_metadata: r.hide_metadata,
                revoked_at: r.revoked_at.map(|t| t.to_rfc3339()),
                last_accessed_at: r.last_accessed_at.map(|t| t.to_rfc3339()),
                created_at: r.created_at.to_rfc3339(),
            }
        })
        .collect();
    Ok(Json(views))
}

/// # Errors
/// `401` se non autenticato; `403` se non owner/admin della cartella di
/// destinazione dell'upload guest.
#[utoipa::path(
    post,
    path = "/api/v1/guest-uploads/{id}/approve",
    tag = "share",
    operation_id = "guest_uploads_approve",
    summary = "Approve a guest upload",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Id dell'upload guest")),
    responses(
        (status = 204, description = "Upload approvato, indicizzazione accodata"),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Non owner/admin", body = Problem)
    )
)]
pub async fn approve_guest_upload(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Path(id): Path<uuid::Uuid>,
) -> Result<StatusCode, Problem> {
    let asset_id = GuestUploadRepo::new(&state.db).approve(&ctx, id).await?;
    AssetRepo::new(&state.db)
        .clear_guest_flag(&ctx, AssetId::from_uuid(asset_id))
        .await?;
    JobRepo::new(&state.db)
        .enqueue(
            JobKind::ExtractMetadata,
            serde_json::json!({ "asset_id": asset_id.to_string() }),
            JobPriority::Background,
            Some(&format!("meta:{asset_id}")),
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

fn share_headers() -> HeaderMap {
    let mut h = HeaderMap::new();
    h.insert(
        "x-robots-tag",
        axum::http::HeaderValue::from_static("noindex, nofollow"),
    );
    h.insert(
        "referrer-policy",
        axum::http::HeaderValue::from_static("no-referrer"),
    );
    h
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct ShareInfoResponse {
    pub object_type: String,
    pub object_id: String,
    pub allow_download: bool,
    pub allow_original: bool,
    pub hide_metadata: bool,
    pub allow_upload: bool,
    pub has_password: bool,
}

/// Nessuna sicurezza a cookie di sessione: l'accesso è il token nel
/// percorso stesso — è la superficie pubblica dei link di condivisione.
///
/// # Errors
/// `403` se il token non esiste o è revocato; `429` oltre il rate limit
/// per token.
#[utoipa::path(
    get,
    path = "/api/v1/share/{token}",
    tag = "share",
    operation_id = "share_public_info",
    summary = "Get public info about a share link",
    params(("token" = String, Path, description = "Token del link pubblico")),
    responses(
        (status = 200, description = "Info pubbliche sul link", body = ShareInfoResponse),
        (status = 403, description = "Token inesistente o revocato", body = Problem),
        (status = 429, description = "Troppi accessi con questo token", body = Problem)
    )
)]
pub async fn public_info(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(token): Path<String>,
) -> Result<impl IntoResponse, Problem> {
    if !state.share_limiter.check_and_record(&token) {
        return Err(Problem::too_many_requests());
    }

    let share_token = ShareToken::from_string(token);
    let hash = share_token.digest();

    let row = ShareLinkRepo::new(&state.db)
        .lookup_by_token_hash(&hash)
        .await?
        .ok_or_else(Problem::forbidden)?;

    ShareLinkRepo::new(&state.db).record_view(row.id).await?;

    let resp = ShareInfoResponse {
        object_type: row.object_type,
        object_id: row.object_id.to_string(),
        allow_download: row.allow_download,
        allow_original: row.allow_original,
        hide_metadata: row.hide_metadata,
        allow_upload: row.allow_upload,
        has_password: row.password_hash.is_some(),
    };

    let jar = jar.add(share_link_cookie(
        &share_token,
        std::time::Duration::from_secs(60 * 60),
    ));
    Ok((share_headers(), jar, Json(resp)))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct ShareAuthRequest {
    pub password: Option<String>,
}

/// # Errors
/// `403` se il token non esiste, è revocato, o la password non combacia;
/// `429` oltre il rate limit per token.
#[utoipa::path(
    post,
    path = "/api/v1/share/{token}/auth",
    tag = "share",
    operation_id = "share_public_auth",
    summary = "Unlock a password-protected share link",
    params(("token" = String, Path, description = "Token del link pubblico")),
    request_body = ShareAuthRequest,
    responses(
        (status = 204, description = "Password corretta, cookie di sblocco impostato"),
        (status = 403, description = "Token inesistente, revocato, o password errata", body = Problem),
        (status = 429, description = "Troppi accessi con questo token", body = Problem)
    )
)]
pub async fn public_auth(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(token): Path<String>,
    Json(req): Json<ShareAuthRequest>,
) -> Result<impl IntoResponse, Problem> {
    if !state.share_limiter.check_and_record(&token) {
        return Err(Problem::too_many_requests());
    }

    let share_token = ShareToken::from_string(token);
    let hash = share_token.digest();

    let row = ShareLinkRepo::new(&state.db)
        .lookup_by_token_hash(&hash)
        .await?
        .ok_or_else(Problem::forbidden)?;

    if let Some(pw_hash) = &row.password_hash {
        let provided = req.password.unwrap_or_default();
        let parsed = Password::parse_owned(provided).map_err(|_| Problem::forbidden())?;
        let stored = PasswordHash::from_stored(pw_hash.clone());
        if !verify_password(&parsed, &stored) {
            return Err(Problem::forbidden());
        }
    }

    let unlock = state.share_unlocks.issue(row.id);
    let jar = jar
        .add(share_link_cookie(
            &share_token,
            std::time::Duration::from_secs(60 * 60),
        ))
        .add(share_unlock_cookie(
            &unlock,
            std::time::Duration::from_secs(60 * 60),
        ));
    Ok((share_headers(), jar, StatusCode::NO_CONTENT))
}

/// Sottoinsieme di [`AssetView`] sicuro per un guest anonimo — **non**
/// `AssetView` stessa, apposta: quel tipo è condiviso con timeline/ricerca ed
/// è pensato per crescere di campi "additivi" (`tags`, `faces`, `full_exif`,
/// …, vedi i commenti su `AssetView`), e riusarlo qui esporrebbe ogni futura
/// aggiunta a un utente anonimo senza che nessuno se ne accorga. `face_
/// privacy.rs` lo impone strutturalmente: cammina l'intero corpo e rifiuta
/// qualunque chiave "face"/"person", quindi anche `faces:[]` (vuoto, mai
/// popolato da questa rotta) lo fa fallire — di proposito, perché un domani
/// chi copia `.with_faces(...)` da `timeline.rs` in questa rotta lo farebbe
/// fallire lo stesso, non solo il caso con dati veri.
#[derive(Serialize, utoipa::ToSchema)]
pub struct PublicAssetView {
    pub id: String,
    pub folder_id: String,
    pub filename: String,
    pub content_hash: Option<String>,
    pub size_bytes: i64,
    pub kind: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub taken_at_utc: Option<chrono::DateTime<chrono::Utc>>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbhash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<GeoPointView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub place_id: Option<i64>,
    pub stack_size: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_kind: Option<String>,
}

impl From<AssetView> for PublicAssetView {
    fn from(v: AssetView) -> Self {
        Self {
            id: v.id,
            folder_id: v.folder_id,
            filename: v.filename,
            content_hash: v.content_hash,
            size_bytes: v.size_bytes,
            kind: v.kind,
            status: v.status,
            taken_at_utc: v.taken_at_utc,
            width: v.width,
            height: v.height,
            thumbhash: v.thumbhash,
            location: v.location,
            place_id: v.place_id,
            stack_size: v.stack_size,
            raw_kind: v.raw_kind,
        }
    }
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct SharedContentResponse {
    pub object_type: String,
    pub assets: Vec<PublicAssetView>,
}

/// # Errors
/// `403` se il token/cookie di sblocco non è valido o l'oggetto condiviso
/// non è più risolvibile.
#[utoipa::path(
    get,
    path = "/api/v1/share/{token}/assets",
    tag = "share",
    operation_id = "share_public_assets",
    summary = "List assets exposed by a share link",
    params(("token" = String, Path, description = "Token del link pubblico")),
    responses(
        (status = 200, description = "Asset condivisi (metadati redatti se hide_metadata)", body = SharedContentResponse),
        (status = 403, description = "Token/sblocco non valido", body = Problem)
    )
)]
pub async fn public_assets(
    State(state): State<AppState>,
    ShareAuth(ctx): ShareAuth,
) -> Result<impl IntoResponse, Problem> {
    let Actor::ShareLink {
        object_type,
        object_id,
        hide_metadata,
        ..
    } = &ctx.actor
    else {
        return Err(Problem::forbidden());
    };

    let domain_assets: Vec<keeppix_domain::Asset> = match object_type.as_str() {
        "album" => AlbumRepo::new(&state.db)
            .list_assets(&ctx, AlbumId::from_uuid(*object_id))
            .await?
            .into_iter()
            .map(|row| row.asset)
            .collect(),
        "asset" => {
            let asset = AssetRepo::new(&state.db)
                .find_by_id(&ctx, AssetId::from_uuid(*object_id))
                .await?;
            vec![asset]
        }
        "folder" => AssetRepo::new(&state.db)
            .find_by_folder(&ctx, FolderId::from_uuid(*object_id))
            .await?
            .into_iter()
            .filter(|a| a.status == AssetStatus::Indexed)
            .collect(),
        _ => return Err(Problem::forbidden()),
    };

    let asset_ids: Vec<AssetId> = domain_assets.iter().map(|a| a.id).collect();
    let location_rows = if *hide_metadata {
        Vec::new()
    } else {
        HomeRepo::new(&state.db)
            .public_locations_for_assets(&asset_ids)
            .await?
    };
    let locations: std::collections::HashMap<AssetId, _> = location_rows
        .into_iter()
        .map(|row| (row.asset_id, row))
        .collect();

    let views = domain_assets
        .into_iter()
        .map(|asset| {
            let mut view = AssetView::from_asset(&asset);
            if *hide_metadata {
                view.taken_at_utc = None;
                return PublicAssetView::from(view);
            }
            if let Some(row) = locations.get(&asset.id)
                && !row.redact
            {
                view = view.with_location(row.location.map(GeoPointView::from), row.place_id);
            }
            PublicAssetView::from(view)
        })
        .collect();

    Ok((
        share_headers(),
        Json(SharedContentResponse {
            object_type: object_type.clone(),
            assets: views,
        }),
    ))
}

/// Ceiling when the link has `allow_upload` but no explicit quota: a Pi
/// disk must not be filled by omission. Raise per-link, never here.
const DEFAULT_UPLOAD_QUOTA_BYTES: i64 = 256 * 1024 * 1024;

#[derive(Deserialize)]
pub struct GuestUploadQuery {
    pub filename: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct GuestUploadResponse {
    pub id: String,
    pub filename: String,
}

/// # Errors
/// `403` se il link non ha `allow_upload` o non è su una cartella; `400` se
/// il nome file non è valido o il corpo è vuoto; `409` in caso di collisione
/// esatta col nome finale; `413` oltre la quota residua del link.
#[utoipa::path(
    post,
    path = "/api/v1/share/{token}/uploads",
    tag = "share",
    operation_id = "share_public_upload",
    summary = "Upload a file through a share link",
    params(
        ("token" = String, Path, description = "Token del link pubblico"),
        ("filename" = String, Query, description = "Nome file proposto dal client")
    ),
    request_body(content = Vec<u8>, description = "Corpo del file", content_type = "application/octet-stream"),
    responses(
        (status = 201, description = "File caricato e in coda per l'indicizzazione", body = GuestUploadResponse),
        (status = 400, description = "Nome file non valido o corpo vuoto", body = Problem),
        (status = 403, description = "Link non abilitato all'upload", body = Problem),
        (status = 409, description = "Collisione di nome", body = Problem),
        (status = 413, description = "Quota residua del link esaurita", body = Problem)
    )
)]
pub async fn public_upload(
    State(state): State<AppState>,
    ShareAuth(ctx): ShareAuth,
    Query(query): Query<GuestUploadQuery>,
    body: Body,
) -> Result<impl IntoResponse, Problem> {
    let Actor::ShareLink {
        object_type,
        object_id,
        allow_upload,
        upload_quota_bytes,
        ..
    } = &ctx.actor
    else {
        return Err(Problem::forbidden());
    };
    if !allow_upload {
        return Err(Problem::forbidden());
    }
    if object_type != "folder" {
        return Err(Problem::forbidden());
    }

    let filename = AssetName::parse(&query.filename)
        .map_err(|_| Problem::bad_request("invalid-filename", "Invalid filename"))?;
    let folder_id = FolderId::from_uuid(*object_id);

    let existing = AssetRepo::new(&state.db)
        .find_by_folder(&ctx, folder_id)
        .await?;
    let taken: Vec<String> = existing
        .iter()
        .map(|a| a.filename.as_str().to_owned())
        .collect();
    let filename = AssetName::parse(&unique_filename(filename.as_str(), &taken))
        .map_err(|_| Problem::bad_request("invalid-filename", "Invalid filename"))?;

    let quota = upload_quota_bytes.unwrap_or(DEFAULT_UPLOAD_QUOTA_BYTES);
    let used = GuestUploadRepo::new(&state.db).bytes_used(&ctx).await?;
    let remaining = quota.saturating_sub(used);
    if remaining <= 0 {
        return Err(Problem::payload_too_large());
    }
    let cap = u64::try_from(remaining).unwrap_or(0);
    let response = write_and_queue(&state, &ctx, folder_id, filename, body, cap).await?;
    Ok((StatusCode::CREATED, share_headers(), Json(response)))
}

async fn write_and_queue(
    state: &AppState,
    ctx: &keeppix_domain::AuthContext,
    folder_id: FolderId,
    filename: AssetName,
    body: Body,
    cap: u64,
) -> Result<GuestUploadResponse, Problem> {
    let dir = FolderRepo::new(&state.db)
        .absolute_path(ctx, folder_id)
        .await?;
    let path = dir.join(filename.as_str());
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .await
        .map_err(|err| {
            if err.kind() == std::io::ErrorKind::AlreadyExists {
                Problem::new(StatusCode::CONFLICT, "conflict", "File already exists")
            } else {
                tracing::error!(error = %err, "guest upload could not create file");
                Problem::internal()
            }
        })?;

    let written = match write_body_capped(body, &mut file, cap).await {
        Ok(written) => written,
        Err(err) => {
            drop(file);
            let _ = tokio::fs::remove_file(&path).await;
            return Err(err);
        }
    };
    if let Err(err) = file.flush().await {
        drop(file);
        let _ = tokio::fs::remove_file(&path).await;
        tracing::error!(error = %err, "guest upload flush failed");
        return Err(Problem::internal());
    }
    drop(file);

    if written == 0 {
        let _ = tokio::fs::remove_file(&path).await;
        return Err(Problem::bad_request(
            "empty-upload",
            "Upload body was empty",
        ));
    }

    let meta = tokio::fs::metadata(&path).await.map_err(|err| {
        tracing::error!(error = %err, "guest upload stat failed");
        Problem::internal()
    })?;
    let size_bytes = i64::try_from(written).unwrap_or(i64::MAX);
    let mtime = meta
        .modified()
        .ok()
        .map_or_else(chrono::Utc::now, chrono::DateTime::<chrono::Utc>::from);
    let inode = inode_of(&meta);
    let header = peek_header(&path).await?;
    let kind = keeppix_media::detect_kind(&header);

    match GuestUploadRepo::new(&state.db)
        .receive(
            ctx,
            NewAsset {
                folder_id,
                filename: filename.clone(),
                size_bytes,
                mtime,
                inode,
                kind,
            },
        )
        .await
    {
        Ok((upload_id, _)) => Ok(GuestUploadResponse {
            id: upload_id.to_string(),
            filename: filename.as_str().to_owned(),
        }),
        Err(err) => {
            let _ = tokio::fs::remove_file(&path).await;
            Err(err.into())
        }
    }
}

async fn write_body_capped(
    body: Body,
    dest: &mut tokio::fs::File,
    cap: u64,
) -> Result<u64, Problem> {
    use http_body::Body as _;
    let mut body = std::pin::pin!(body);
    let mut written = 0_u64;
    loop {
        let frame = std::future::poll_fn(|cx| body.as_mut().poll_frame(cx)).await;
        match frame {
            None => break,
            Some(Err(_)) => {
                return Err(Problem::bad_request(
                    "invalid-body",
                    "Could not read upload",
                ));
            }
            Some(Ok(frame)) => {
                let Ok(data) = frame.into_data() else {
                    continue;
                };
                let chunk = u64::try_from(data.len()).unwrap_or(u64::MAX);
                if written.saturating_add(chunk) > cap {
                    return Err(Problem::payload_too_large());
                }
                dest.write_all(&data).await.map_err(|err| {
                    tracing::error!(error = %err, "guest upload write failed");
                    Problem::internal()
                })?;
                written = written.saturating_add(chunk);
            }
        }
    }
    Ok(written)
}

/// Riusata da [`crate::routes::upload`] per la stessa verifica di
/// decodificabilità a fine sessione tus.
pub(crate) async fn peek_header(path: &std::path::Path) -> Result<Vec<u8>, Problem> {
    use tokio::io::AsyncReadExt as _;
    let mut file = tokio::fs::File::open(path)
        .await
        .map_err(|_| Problem::internal())?;
    let mut buf = vec![0_u8; 4096];
    let n = file.read(&mut buf).await.map_err(|_| Problem::internal())?;
    buf.truncate(n);
    Ok(buf)
}

fn unique_filename(desired: &str, taken: &[String]) -> String {
    if !taken.iter().any(|name| name == desired) {
        return desired.to_owned();
    }
    let (stem, ext) = match desired.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => (stem, format!(".{ext}")),
        _ => (desired, String::new()),
    };
    for n in 1..10_000 {
        let candidate = format!("{stem}-{n}{ext}");
        if !taken.iter().any(|name| name == &candidate) {
            return candidate;
        }
    }
    format!("{stem}-{}{ext}", uuid::Uuid::now_v7())
}

fn inode_of(meta: &std::fs::Metadata) -> Option<i64> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        i64::try_from(meta.ino()).ok()
    }
    #[cfg(not(unix))]
    {
        let _ = meta;
        None
    }
}
