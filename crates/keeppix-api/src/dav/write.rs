//! `PUT`, `MKCOL`, `MOVE` (Task 7, Fase 5) — le operazioni di scrittura
//! `WebDAV`, riusando gli stessi repository del resto dell'API: nessuna
//! query SQL qui, solo `FolderRepo`/`AssetRepo`/`JobRepo` come ovunque nel
//! resto del crate.
//!
//! **`COPY` non è implementata** (resta `501` nel dispatch di
//! `dav::handler`): copiare un intero sottoalbero — id nuovi per ogni
//! cartella e ogni asset, eventualmente attraverso librerie diverse (il
//! brief chiede di verificare lo spazio libero sulla libreria di
//! *destinazione*, che può differire da quella di partenza) — è
//! esplicitamente segnalata dal brief come "se troppo complessa,
//! implementarla come stub 501". Vedi il ledger del Task 7 per il
//! ragionamento completo.
//!
//! **Mai una sovrascrittura silenziosa su `PUT`** (invariante di
//! `AGENTS.md`): stesso nome e stesso hash è un duplicato, saltato
//! (`204`); stesso nome e hash diverso prende un suffisso numerico (`201`,
//! nome finale diverso da quello richiesto) — la stessa regola di
//! `UploadSessionRepo::finalize` (Task 1), applicata qui da
//! `AssetRepo::ingest_direct`.

use std::path::Path;

use axum::body::Body;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use chrono::Utc;
use keeppix_db::{AssetRepo, FolderRepo, UPLOAD_TMP_DIR_NAME};
use keeppix_domain::{AssetId, AuthContext, CollisionOutcome, FolderId};

use crate::problem::Problem;
use crate::routes::share::peek_header;
use crate::routes::upload::enqueue_indexing;
use crate::state::AppState;

/// `PUT /dav/folder/{folder_id}/{filename}` — stesso percorso del `tus`
/// upload (Task 1), senza sessione: il corpo arriva già intero in streaming,
/// si scrive su un temporaneo in `.keeppix-tmp/` dentro la libreria, poi un
/// `rename()` atomico lo sposta al posto finale sullo stesso filesystem.
///
/// Un nome che inizia con `.` (`.DS_Store`, `._foto.jpg`, i sidecar
/// nascosti di macOS) viene **accettato e salvato su disco** — un client
/// `WebDAV` non deve ricevere un errore inaspettato per un file che non ha
/// chiesto lui di caricare — ma **non indicizzato**: nessuna riga `assets`,
/// nessun job (vedi [`put_dotfile`]).
///
/// # Errors
/// `400` se il nome non è un componente di percorso valido, o se il corpo
/// è vuoto. `403` se il chiamante non è editor sulla cartella. `500` per un
/// errore di I/O.
pub async fn put(
    state: &AppState,
    ctx: &AuthContext,
    folder_id: FolderId,
    filename: &str,
    body: Body,
) -> Result<Response, Problem> {
    let (_, library) = FolderRepo::new(&state.db)
        .assert_editor(ctx, folder_id)
        .await?;

    let tmp_dir = library.root_path.join(UPLOAD_TMP_DIR_NAME);
    tokio::fs::create_dir_all(&tmp_dir).await.map_err(|err| {
        tracing::error!(error = %err, "webdav PUT: could not create the temp dir");
        Problem::internal()
    })?;
    let temp_path = tmp_dir.join(format!("{}_{filename}", uuid::Uuid::now_v7()));

    let written = write_body_to_file(&temp_path, body).await?;
    if written == 0 {
        let _ = tokio::fs::remove_file(&temp_path).await;
        return Err(Problem::bad_request("empty-body", "PUT body was empty"));
    }

    if filename.starts_with('.') {
        return put_dotfile(&state.db, ctx, folder_id, filename, &temp_path).await;
    }

    let header = peek_header(&temp_path).await?;
    let kind = keeppix_media::detect_kind(&header);
    let hash = hash_temp_file(&temp_path).await?;
    let size_bytes = i64::try_from(written).unwrap_or(i64::MAX);

    let outcome = AssetRepo::new(&state.db)
        .ingest_direct(
            ctx,
            folder_id,
            &temp_path,
            filename,
            hash,
            size_bytes,
            Utc::now(),
            kind,
        )
        .await?;

    enqueue_indexing(&state.db, outcome.asset_id, &outcome.collision).await;

    Ok(put_response(outcome.asset_id, &outcome.collision))
}

/// Salva il dotfile al posto finale senza passare dal controllo di
/// collisione degli asset — vedi la doc di [`put`]. Sovrascrive un omonimo
/// già presente: è esattamente il comportamento che un file di cache del
/// sistema operativo del client si aspetta (`.DS_Store` viene riscritto in
/// continuazione da Finder), e l'invariante "mai sovrascrittura silenziosa"
/// dell'`AGENTS.md` protegge le foto dell'utente, non la sua cache
/// (Ruling, Task 7).
async fn put_dotfile(
    db: &keeppix_db::Db,
    ctx: &AuthContext,
    folder_id: FolderId,
    filename: &str,
    temp_path: &Path,
) -> Result<Response, Problem> {
    let folder_dir = FolderRepo::new(db).absolute_path(ctx, folder_id).await?;
    let target = folder_dir.join(filename);
    tokio::fs::rename(temp_path, &target).await.map_err(|err| {
        tracing::error!(error = %err, "webdav PUT: could not move the dotfile into place");
        Problem::internal()
    })?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

/// `201 Created` + `Location: /dav/asset/{id}` per un file nuovo (creato o
/// rinominato per collisione); `204 No Content` per un duplicato esatto —
/// mai una risposta che suggerisca una sovrascrittura silenziosa.
fn put_response(asset_id: AssetId, collision: &CollisionOutcome) -> Response {
    match collision {
        CollisionOutcome::SkippedDuplicate { .. } => StatusCode::NO_CONTENT.into_response(),
        CollisionOutcome::Created | CollisionOutcome::RenamedTo(_) => {
            let mut headers = HeaderMap::new();
            if let Ok(value) = HeaderValue::from_str(&format!("/dav/asset/{asset_id}")) {
                headers.insert(header::LOCATION, value);
            }
            (StatusCode::CREATED, headers).into_response()
        }
    }
}

/// `MKCOL /dav/folder/{parent_id}/{new_name}` — crea la sotto-cartella nel
/// database (`FolderRepo::ensure_child`, idempotente per costruzione) e la
/// directory sul disco.
///
/// Semplificazione (ledger Task 7): un `MKCOL` ripetuto sullo stesso nome
/// non fallisce con `405` (RFC 4918 §9.3) — restituisce di nuovo `201`
/// sulla cartella già esistente, perché `ensure_child` è idempotente per lo
/// stesso motivo per cui lo scanner lo richiama senza duplicare nulla.
/// Nessun client reale in uso qui dipende dal `405`.
///
/// # Errors
/// `403` se il chiamante non è editor sulla cartella genitore. `500` per un
/// errore di I/O nella creazione della directory.
pub async fn mkcol(
    state: &AppState,
    ctx: &AuthContext,
    parent_id: FolderId,
    new_name: &str,
) -> Result<Response, Problem> {
    let folder_repo = FolderRepo::new(&state.db);
    let (parent, _library) = folder_repo.assert_editor(ctx, parent_id).await?;
    let child = folder_repo.ensure_child(&parent, new_name).await?;

    let path = folder_repo.absolute_path(ctx, child.id).await?;
    tokio::fs::create_dir_all(&path).await.map_err(|err| {
        tracing::error!(error = %err, "webdav MKCOL: could not create the directory on disk");
        Problem::internal()
    })?;

    let mut headers = HeaderMap::new();
    if let Ok(value) = HeaderValue::from_str(&format!("/dav/folder/{}", child.id)) {
        headers.insert(header::LOCATION, value);
    }
    Ok((StatusCode::CREATED, headers).into_response())
}

/// `MOVE /dav/folder/{src_id}` con `Destination: /dav/folder/{dst_parent_id}/{name}`
/// — verifica il permesso di editor su **entrambe** le cartelle coinvolte
/// (il brief lo richiede esplicitamente: `FolderRepo::move_subtree` da sola
/// controlla solo la cartella sorgente), poi riusa `move_subtree`, che
/// conserva rating/album/descrizioni per costruzione, e infine sposta anche
/// la directory su disco.
///
/// Nota (ledger Task 7): `move_subtree` **non** chiama `rename()` — riscrive
/// solo `folders.path` nel database (il brief lo dava per scontato, ma non
/// è così: verificato leggendo il codice). Lo spostamento fisico qui sotto
/// avviene **dopo** il commit di `move_subtree`: se il `rename()` fallisse
/// (solo un errore di I/O reale, perché `move_subtree` ha già validato
/// ciclo/libreria/collisione di nome prima di arrivare qui), la cartella
/// sarebbe già spostata nel database ma non sul disco — un'inconsistenza
/// da correggere a mano, la stessa identica lacuna già presente e non
/// toccata in questo task in `PATCH /api/v1/folders/{id}` (che oggi non
/// sposta la directory per niente). Il `MOVE` `WebDAV` qui è quindi già più
/// corretto dell'endpoint REST esistente, non più fragile.
///
/// # Errors
/// `403` se il chiamante non è editor su `src_id` o su `dst_parent_id`.
/// `409` se `dst_parent_id` sta nel sottoalbero di `src_id` (compresa la
/// cartella stessa), se le due cartelle sono in librerie diverse, o se il
/// nuovo genitore ha già un figlio con lo stesso nome. `500` se il
/// `rename()` fisico fallisce dopo che il database è già stato aggiornato.
pub async fn move_folder(
    state: &AppState,
    ctx: &AuthContext,
    src_id: FolderId,
    dst_parent_id: FolderId,
) -> Result<Response, Problem> {
    let folder_repo = FolderRepo::new(&state.db);
    folder_repo.assert_editor(ctx, src_id).await?;
    folder_repo.assert_editor(ctx, dst_parent_id).await?;

    let old_path = folder_repo.absolute_path(ctx, src_id).await?;

    folder_repo.move_subtree(ctx, src_id, dst_parent_id).await?;

    let new_path = folder_repo.absolute_path(ctx, src_id).await?;
    if old_path != new_path {
        tokio::fs::rename(&old_path, &new_path).await.map_err(|err| {
            tracing::error!(
                error = %err,
                old = %old_path.display(),
                new = %new_path.display(),
                "webdav MOVE: the folder moved in the database but the directory rename failed on disk"
            );
            Problem::internal()
        })?;
    }

    Ok(StatusCode::NO_CONTENT.into_response())
}

/// Scrive il corpo della richiesta su `path` in streaming — mai
/// bufferizzato per intero in RAM, sullo stile di
/// `crate::routes::upload::write_chunk_checked`, ma senza checksum: qui non
/// c'è un `Upload-Checksum` a cui confrontarlo, l'hash si calcola a file
/// completo (vedi [`hash_temp_file`]), esattamente come
/// `crate::routes::upload::finalize_upload`.
///
/// # Errors
/// `400` se il corpo non si può leggere. `500` per un errore di I/O sul
/// temporaneo.
async fn write_body_to_file(path: &Path, body: Body) -> Result<u64, Problem> {
    use http_body::Body as _;
    use tokio::io::AsyncWriteExt as _;

    let mut file = tokio::fs::File::create(path).await.map_err(|err| {
        tracing::error!(error = %err, "webdav PUT: could not create the temp file");
        Problem::internal()
    })?;

    let mut written = 0_u64;
    let mut body = std::pin::pin!(body);
    loop {
        let frame = std::future::poll_fn(|cx| body.as_mut().poll_frame(cx)).await;
        match frame {
            None => break,
            Some(Err(_)) => {
                let _ = tokio::fs::remove_file(path).await;
                return Err(Problem::bad_request(
                    "invalid-body",
                    "Could not read the request body",
                ));
            }
            Some(Ok(frame)) => {
                let Ok(data) = frame.into_data() else {
                    continue;
                };
                if let Err(err) = file.write_all(&data).await {
                    tracing::error!(error = %err, "webdav PUT: write failed");
                    let _ = tokio::fs::remove_file(path).await;
                    return Err(Problem::internal());
                }
                written = written.saturating_add(u64::try_from(data.len()).unwrap_or(u64::MAX));
            }
        }
    }
    file.flush().await.map_err(|err| {
        tracing::error!(error = %err, "webdav PUT: flush failed");
        Problem::internal()
    })?;
    Ok(written)
}

/// Hash blake3 dell'intero temporaneo, fuori dal thread async — come
/// `crate::routes::upload::finalize_upload`.
async fn hash_temp_file(path: &Path) -> Result<[u8; 32], Problem> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || keeppix_media::hash_file(&path))
        .await
        .map_err(|_| Problem::internal())?
        .map_err(|err| {
            tracing::error!(error = %err, "webdav PUT: could not hash the temp file");
            Problem::internal()
        })
}
