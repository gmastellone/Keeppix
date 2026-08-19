//! Sessioni di upload riprendibili in stile tus (spec §1). Il protocollo è
//! nostro, non un crate tus: pre-check per hash, creazione con verifica di
//! spazio e permessi, chunk con checksum, finalizzazione con verifica
//! end-to-end e risoluzione di collisione sul nome. Vedi
//! `keeppix_db::UploadSessionRepo` per la logica che qui si limita a
//! tradurre in HTTP.

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Utc};
use keeppix_db::{AssetRepo, NewUploadSession, UploadSessionRepo};
use keeppix_domain::{
    AssetKind, AssetName, AuthContext, ChunkChecksum, CollisionOutcome, FolderId, UploadSession,
    UploadSessionId,
};
use serde::{Deserialize, Serialize};

use crate::extract::SessionOrShare;
use crate::json::Json;
use crate::problem::Problem;
use crate::routes::share::peek_header;
use crate::state::AppState;

/// Limite server-side su un singolo chunk (spec §1.2 non lo fissa, ma un
/// client che dichiara `remaining` a più gigabyte non deve poter far
/// bufferizzare tutto in RAM in un colpo): un chunk più grande va spezzato
/// dal client, non accettato qui.
const MAX_CHUNK_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Deserialize)]
pub struct CheckRequest {
    /// blake3 esadecimali (64 caratteri) calcolati localmente dal client.
    pub hashes: Vec<String>,
}

#[derive(Serialize)]
pub struct CheckResponse {
    /// Il sottoinsieme di `hashes` che il server **non** ha già: sono gli
    /// unici che vale la pena caricare (spec §1.2, «questi 47 li ho già,
    /// caricami gli altri 12»).
    pub unknown_hashes: Vec<String>,
}

/// `POST /api/v1/upload/check` — pre-check batch prima di aprire sessioni.
///
/// # Errors
/// `400` se un hash non è esadecimale a 64 caratteri. `401`/`403` se il
/// chiamante non ha un contesto valido.
pub async fn check(
    State(state): State<AppState>,
    SessionOrShare(ctx): SessionOrShare,
    Json(req): Json<CheckRequest>,
) -> Result<Json<CheckResponse>, Problem> {
    let mut parsed = Vec::with_capacity(req.hashes.len());
    for hex in &req.hashes {
        parsed.push(decode_hex32(hex)?);
    }

    let known = AssetRepo::new(&state.db)
        .known_hashes(&ctx, &parsed)
        .await?;

    let unknown_hashes = req
        .hashes
        .into_iter()
        .zip(parsed)
        .filter(|(_, bytes)| !known.contains(bytes))
        .map(|(hex, _)| hex)
        .collect();

    Ok(Json(CheckResponse { unknown_hashes }))
}

#[derive(Deserialize)]
pub struct CreateSessionRequest {
    pub target_folder_id: uuid::Uuid,
    pub filename: String,
    pub expected_size: i64,
    /// blake3 esadecimale del file intero, se il client lo conosce già.
    pub expected_hash: Option<String>,
    /// `mtime` originale sul dispositivo del client (spec §1.3): preservato
    /// come fallback di `taken_at` quando l'EXIF non ce l'ha.
    pub client_mtime: Option<DateTime<Utc>>,
}

#[derive(Serialize)]
pub struct CreateSessionResponse {
    pub id: String,
}

/// `POST /api/v1/upload` — apre una sessione. `201` con `Location:
/// /api/v1/upload/{id}`.
///
/// # Errors
/// `400` se `filename` o `expected_hash` non sono validi. `403` se il
/// chiamante non può scrivere nella cartella destinazione, o se un link
/// condiviso non ha `allow_upload` sull'oggetto esatto. `507` se lo spazio
/// libero sul filesystem della libreria è sotto `expected_size`.
pub async fn create(
    State(state): State<AppState>,
    SessionOrShare(ctx): SessionOrShare,
    Json(req): Json<CreateSessionRequest>,
) -> Result<Response, Problem> {
    let filename = AssetName::parse(&req.filename)
        .map_err(|_| Problem::bad_request("invalid-filename", "Invalid filename"))?;
    let expected_hash = match req.expected_hash {
        Some(hex) => Some(decode_hex32(&hex)?),
        None => None,
    };

    let session = UploadSessionRepo::new(&state.db)
        .create(
            &ctx,
            NewUploadSession {
                target_folder_id: FolderId::from_uuid(req.target_folder_id),
                filename: filename.as_str().to_owned(),
                expected_size: req.expected_size,
                expected_hash,
                client_mtime: req.client_mtime,
            },
        )
        .await?;

    let mut headers = HeaderMap::new();
    if let Ok(value) = HeaderValue::from_str(&format!("/api/v1/upload/{}", session.id)) {
        headers.insert(header::LOCATION, value);
    }

    Ok((
        StatusCode::CREATED,
        headers,
        Json(CreateSessionResponse {
            id: session.id.to_string(),
        }),
    )
        .into_response())
}

/// `HEAD /api/v1/upload/{id}` — l'offset vero, mai quello che il client
/// crede di avere (spec §1.3, «la verità sta sempre sul server»).
///
/// # Errors
/// `403` se il chiamante non è il proprietario della sessione. `410` se è
/// scaduta.
pub async fn head(
    State(state): State<AppState>,
    SessionOrShare(ctx): SessionOrShare,
    Path(id): Path<uuid::Uuid>,
) -> Result<Response, Problem> {
    let session = UploadSessionRepo::new(&state.db)
        .load_owned(&ctx, UploadSessionId::from_uuid(id))
        .await?;
    Ok((StatusCode::OK, offset_header(session.received_bytes)).into_response())
}

#[derive(Serialize)]
pub struct UploadCompleteResponse {
    pub asset_id: String,
    pub filename: String,
    /// `created`, `skipped_duplicate` o `renamed` (spec §1.4): mai una
    /// sovrascrittura silenziosa, sempre segnalata al client.
    pub collision: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub existing_asset_id: Option<String>,
}

/// `PATCH /api/v1/upload/{id}` — appende un chunk, verifica il suo checksum,
/// e finalizza la sessione quando l'offset raggiunge `expected_size`.
///
/// # Errors
/// `403`/`410` come [`head`]. `409` se `Upload-Offset` non combacia con
/// `received_bytes` reale — mai un'accettazione silenziosa che corrompe il
/// file. `400` se manca un header richiesto o il corpo è vuoto. `413` se il
/// chunk supera `MAX_CHUNK_BYTES` o i byte rimanenti della sessione: il
/// corpo è letto in streaming, mai bufferizzato per intero in RAM. `460` se
/// il checksum del chunk non combacia: il chunk non viene scritto, il
/// client può rispedirlo senza perdere l'offset precedente. `422` a file
/// completo, se l'hash end-to-end non combacia con `expected_hash` o il
/// file non è decodificabile — mai finito in libreria, temporaneo
/// eliminato, sessione marcata fallita.
pub async fn patch(
    State(state): State<AppState>,
    SessionOrShare(ctx): SessionOrShare,
    Path(id): Path<uuid::Uuid>,
    headers: HeaderMap,
    body: Body,
) -> Result<Response, Problem> {
    let session_id = UploadSessionId::from_uuid(id);
    let repo = UploadSessionRepo::new(&state.db);
    let session = repo.load_owned(&ctx, session_id).await?;

    let offset = parse_offset_header(&headers)?;
    if offset != session.received_bytes {
        return Err(Problem::offset_mismatch(session.received_bytes));
    }

    let remaining = session.expected_size - session.received_bytes;
    if remaining <= 0 {
        return Err(Problem::bad_request(
            "upload-already-complete",
            "This upload session has no bytes left to receive",
        ));
    }
    let max_len = u64::try_from(remaining)
        .unwrap_or(u64::MAX)
        .min(MAX_CHUNK_BYTES);

    let checksum = parse_checksum_header(&headers)?;
    let written = write_chunk_checked(&session.temp_path, body, checksum, max_len).await?;

    let written_i64 = i64::try_from(written).unwrap_or(i64::MAX);
    let new_offset = session.received_bytes.saturating_add(written_i64);
    repo.advance(&ctx, session_id, new_offset).await?;

    if new_offset < session.expected_size {
        return Ok((StatusCode::NO_CONTENT, offset_header(new_offset)).into_response());
    }

    let response = finalize_upload(&repo, &ctx, session_id, &session).await?;
    Ok((StatusCode::CREATED, Json(response)).into_response())
}

async fn finalize_upload(
    repo: &UploadSessionRepo<'_>,
    ctx: &AuthContext,
    id: UploadSessionId,
    session: &UploadSession,
) -> Result<UploadCompleteResponse, Problem> {
    let path = session.temp_path.clone();
    let computed = tokio::task::spawn_blocking(move || keeppix_media::hash_file(&path))
        .await
        .map_err(|_| Problem::internal())?
        .map_err(|err| {
            tracing::error!(error = %err, "upload finalize: could not hash the temp file");
            Problem::internal()
        })?;

    if let Some(expected) = session.expected_hash
        && expected != computed
    {
        repo.fail(ctx, id).await?;
        return Err(Problem::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "upload-hash-mismatch",
            "The uploaded file's hash does not match the declared expected_hash",
        ));
    }

    let header = peek_header(&session.temp_path).await?;
    let kind = keeppix_media::detect_kind(&header);
    if kind == AssetKind::Unknown {
        repo.fail(ctx, id).await?;
        return Err(Problem::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "upload-undecodable",
            "The uploaded file could not be verified as decodable",
        ));
    }

    let outcome = repo.finalize(ctx, id, kind, computed).await?;
    Ok(match outcome.collision {
        CollisionOutcome::Created => UploadCompleteResponse {
            asset_id: outcome.asset_id.to_string(),
            filename: outcome.filename,
            collision: "created".to_owned(),
            existing_asset_id: None,
        },
        CollisionOutcome::SkippedDuplicate { existing_asset_id } => UploadCompleteResponse {
            asset_id: outcome.asset_id.to_string(),
            filename: outcome.filename,
            collision: "skipped_duplicate".to_owned(),
            existing_asset_id: Some(existing_asset_id.to_string()),
        },
        CollisionOutcome::RenamedTo(_) => UploadCompleteResponse {
            asset_id: outcome.asset_id.to_string(),
            filename: outcome.filename,
            collision: "renamed".to_owned(),
            existing_asset_id: None,
        },
    })
}

/// Scrive un chunk sul temporaneo in streaming — mai bufferizzato per
/// intero in memoria, a differenza di un `axum::body::to_bytes(body, cap)`
/// con `cap` fino a `expected_size` — calcolando l'hash blake3
/// incrementalmente mentre i byte arrivano, sullo stile di
/// `crate::routes::share::write_body_capped`.
///
/// Il checksum si verifica solo a corpo esaurito, ma il chunk **non**
/// sopravvive a un checksum sbagliato: si tronca il file di append alla sua
/// lunghezza originale (`set_len`), disfacendo esattamente i byte appena
/// scritti — il client può rispedirlo senza perdere l'offset già accettato.
///
/// # Errors
/// `400`/`413` se il corpo è vuoto, illeggibile, o supera `max_len` (limite
/// per-chunk, mai tutto il resto del file in un colpo). `460` se il
/// checksum non combacia. `500` per un errore di I/O sul temporaneo.
async fn write_chunk_checked(
    path: &std::path::Path,
    body: Body,
    checksum: ChunkChecksum,
    max_len: u64,
) -> Result<u64, Problem> {
    use http_body::Body as _;
    use tokio::io::AsyncWriteExt as _;

    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .await
        .map_err(|err| {
            tracing::error!(error = %err, "upload chunk: could not open temp file");
            Problem::internal()
        })?;
    let original_len = file
        .metadata()
        .await
        .map_err(|err| {
            tracing::error!(error = %err, "upload chunk: could not stat temp file");
            Problem::internal()
        })?
        .len();

    let mut hasher = blake3::Hasher::new();
    let mut written = 0_u64;
    let mut body = std::pin::pin!(body);
    loop {
        let frame = std::future::poll_fn(|cx| body.as_mut().poll_frame(cx)).await;
        match frame {
            None => break,
            Some(Err(_)) => {
                let _ = file.set_len(original_len).await;
                return Err(Problem::bad_request(
                    "invalid-body",
                    "Could not read the chunk",
                ));
            }
            Some(Ok(frame)) => {
                let Ok(data) = frame.into_data() else {
                    continue;
                };
                let len = u64::try_from(data.len()).unwrap_or(u64::MAX);
                if written.saturating_add(len) > max_len {
                    // Non si scrive oltre il limite: si disfa quanto già
                    // accettato per questo chunk e si segnala 413, non un
                    // avanzamento parziale silenzioso.
                    let _ = file.set_len(original_len).await;
                    return Err(Problem::payload_too_large());
                }
                hasher.update(&data);
                if let Err(err) = file.write_all(&data).await {
                    tracing::error!(error = %err, "upload chunk: write failed");
                    let _ = file.set_len(original_len).await;
                    return Err(Problem::internal());
                }
                written = written.saturating_add(len);
            }
        }
    }

    if written == 0 {
        return Err(Problem::bad_request("empty-chunk", "Chunk body was empty"));
    }

    let computed = hasher.finalize();
    if !checksum.matches(computed.as_bytes()) {
        // Il chunk non va lasciato sul temporaneo: il client lo rispedisce
        // senza perdere l'offset che il server ha già accettato prima di
        // questo chunk.
        file.set_len(original_len).await.map_err(|err| {
            tracing::error!(error = %err, "upload chunk: truncate after checksum mismatch failed");
            Problem::internal()
        })?;
        return Err(Problem::chunk_checksum_mismatch());
    }

    // Un fsync per chunk, non ogni 16 MB come suggerisce la spec §1.3: più
    // sicuro con chunk adattivi fino a `MAX_CHUNK_BYTES`, il costo
    // aggiuntivo di un fsync su un file locale è trascurabile rispetto alla
    // rete.
    file.sync_all().await.map_err(|err| {
        tracing::error!(error = %err, "upload chunk: fsync failed");
        Problem::internal()
    })?;
    Ok(written)
}

fn offset_header(offset: i64) -> HeaderMap {
    let mut headers = HeaderMap::new();
    if let Ok(value) = HeaderValue::from_str(&offset.to_string()) {
        headers.insert(HeaderName::from_static("upload-offset"), value);
    }
    headers
}

fn parse_offset_header(headers: &HeaderMap) -> Result<i64, Problem> {
    let value = headers.get("upload-offset").ok_or_else(|| {
        Problem::bad_request("missing-upload-offset", "Upload-Offset header is required")
    })?;
    let text = value.to_str().map_err(|_| {
        Problem::bad_request(
            "invalid-upload-offset",
            "Upload-Offset must be a non-negative integer",
        )
    })?;
    text.parse::<i64>().map_err(|_| {
        Problem::bad_request(
            "invalid-upload-offset",
            "Upload-Offset must be a non-negative integer",
        )
    })
}

/// `Upload-Checksum: blake3 <hex a 64 caratteri>` — stesso stile
/// dell'estensione checksum di tus 1.0 (algoritmo seguito dal digest), hex
/// invece di base64 per restare coerenti con `content_hash` in tutta
/// l'API (`crate::routes::timeline::hex_hash`).
fn parse_checksum_header(headers: &HeaderMap) -> Result<ChunkChecksum, Problem> {
    let value = headers.get("upload-checksum").ok_or_else(|| {
        Problem::bad_request(
            "missing-upload-checksum",
            "Upload-Checksum header is required",
        )
    })?;
    let text = value.to_str().map_err(|_| bad_checksum_header())?;
    let (algo, hex) = text.split_once(' ').ok_or_else(bad_checksum_header)?;
    if algo != "blake3" {
        return Err(bad_checksum_header());
    }
    Ok(ChunkChecksum(decode_hex32(hex)?))
}

fn bad_checksum_header() -> Problem {
    Problem::bad_request(
        "invalid-upload-checksum",
        "Upload-Checksum must be `blake3 <64 hex characters>`",
    )
}

fn decode_hex32(hex: &str) -> Result<[u8; 32], Problem> {
    if hex.len() != 64 {
        return Err(bad_hash());
    }
    let mut out = [0_u8; 32];
    for (i, chunk) in hex.as_bytes().chunks(2).enumerate() {
        let piece = core::str::from_utf8(chunk).map_err(|_| bad_hash())?;
        out[i] = u8::from_str_radix(piece, 16).map_err(|_| bad_hash())?;
    }
    Ok(out)
}

fn bad_hash() -> Problem {
    Problem::bad_request("invalid-hash", "Expected 64 hex characters")
}
