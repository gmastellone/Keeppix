//! `LOCK`/`UNLOCK` (Task 8, Fase 5) — `WebDAV` Class 2, richiesto da Finder
//! e Windows Explorer prima di scrivere un file: senza risposta a questi
//! due metodi i client nativi si rifiutano di salvare, non solo "vanno più
//! lento".
//!
//! Nessun arbitraggio reale di scrittura concorrente avviene qui: il lock
//! è un contratto col client (nessun secondo writer coordinato da questo
//! codice), serve solo a soddisfare il protocollo che Finder/Explorer si
//! aspettano prima di un `PUT`.

use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use keeppix_db::{AssetRepo, DavLockRepo, FolderRepo};
use keeppix_domain::AuthContext;
use uuid::Uuid;

use super::Resource;
use crate::problem::Problem;
use crate::state::AppState;

/// Deve restare identico al TTL impostato da `DavLockRepo::create`/
/// `refresh` (`crates/keeppix-db/src/dav_locks.rs`) — è solo il valore
/// mostrato nel corpo XML di risposta, la fonte di verità sulla scadenza
/// resta `dav_locks.timeout_at` nel database.
const LOCK_TIMEOUT_SECONDS: u64 = 3600;

/// Percorso opaco usato come chiave in `dav_locks.resource_path` — lo
/// stesso path `/dav/...` con cui il client indirizza la risorsa, mai un
/// percorso filesystem (stesso principio del resto di `dav::mod`).
fn resource_key(resource: &Resource) -> String {
    match resource {
        Resource::Folder(id) => format!("/dav/folder/{id}"),
        Resource::Asset(id) => format!("/dav/asset/{id}"),
        Resource::FolderChild(parent_id, name) => format!("/dav/folder/{parent_id}/{name}"),
    }
}

/// Il chiamante deve almeno vedere la risorsa (o, per un figlio non ancora
/// creato, la cartella genitore) — un lock non deve rivelare a un estraneo
/// nemmeno l'esistenza di una risorsa che non gli appartiene.
async fn assert_visible(
    state: &AppState,
    ctx: &AuthContext,
    resource: &Resource,
) -> Result<(), Problem> {
    match resource {
        Resource::Folder(id) => {
            FolderRepo::new(&state.db).find_by_id(ctx, *id).await?;
        }
        Resource::Asset(id) => {
            AssetRepo::new(&state.db).find_by_id(ctx, *id).await?;
        }
        Resource::FolderChild(parent_id, _) => {
            FolderRepo::new(&state.db)
                .find_by_id(ctx, *parent_id)
                .await?;
        }
    }
    Ok(())
}

/// Estrae il token da un header `If: (<token>)` (RFC 4918 §10.4) — solo la
/// forma con un singolo token, senza liste `And`/`Or`: nessun client reale
/// in uso qui (Finder, Windows Explorer) manda altro per un semplice
/// rinnovo.
fn parse_if_token(raw: &str) -> Option<String> {
    let inner = raw.trim().trim_start_matches('(').trim_end_matches(')');
    strip_angle_brackets(inner)
}

/// Estrae il token da un header `Lock-Token: <token>` (`UNLOCK`) o dal
/// valore restituito da `LOCK` — entrambi portano il token fra `<`/`>`.
fn strip_angle_brackets(raw: &str) -> Option<String> {
    let inner = raw.trim().trim_start_matches('<').trim_end_matches('>');
    (!inner.is_empty()).then(|| inner.to_owned())
}

fn lockdiscovery_xml(token: &str, depth: &str) -> String {
    // Niente continuazioni `\` a fine riga: dopo un `\` seguito da a-capo,
    // Rust rimuove anche gli spazi di indentazione della riga successiva —
    // qui servono spazi letterali nel corpo XML, quindi ogni riga è un
    // `\n` esplicito su un'unica riga di codice.
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<D:prop xmlns:D=\"DAV:\">\n  <D:lockdiscovery>\n    <D:activelock>\n      <D:locktype><D:write/></D:locktype>\n      <D:lockscope><D:exclusive/></D:lockscope>\n      <D:depth>{depth}</D:depth>\n      <D:timeout>Second-{LOCK_TIMEOUT_SECONDS}</D:timeout>\n      <D:locktoken><D:href>{token}</D:href></D:locktoken>\n    </D:activelock>\n  </D:lockdiscovery>\n</D:prop>"
    )
}

fn lock_response(token: &str, depth: &str) -> Response {
    let body = lockdiscovery_xml(token, depth);
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/xml; charset=\"utf-8\""),
    );
    if let Ok(value) = HeaderValue::from_str(&format!("<{token}>")) {
        headers.insert("lock-token", value);
    }
    (StatusCode::OK, headers, body).into_response()
}

/// `LOCK` — senza `If:` crea un nuovo lock esclusivo (`423 Locked` se la
/// risorsa ne ha già uno attivo); con `If: (<token>)` tenta un rinnovo
/// (`412 Precondition Failed` se il token non esiste o è già scaduto — mai
/// un `200` che farebbe credere al client di possedere ancora il lock).
///
/// # Errors
/// `403` se il chiamante non vede la risorsa (o, per un figlio non ancora
/// creato, la cartella genitore). `412` per un rinnovo su un token
/// scaduto/inesistente. `423` per una nuova richiesta su una risorsa già
/// bloccata da un altro token attivo.
pub(crate) async fn lock(
    state: &AppState,
    ctx: &AuthContext,
    resource: &Resource,
    depth_header: Option<&str>,
    if_header: Option<&str>,
) -> Result<Response, Problem> {
    assert_visible(state, ctx, resource).await?;
    let repo = DavLockRepo::new(&state.db);
    let depth = depth_header.unwrap_or("0").to_owned();

    if let Some(token) = if_header.and_then(parse_if_token) {
        return if repo.refresh(&token).await? {
            Ok(lock_response(&token, &depth))
        } else {
            Err(Problem::precondition_failed())
        };
    }

    let path = resource_key(resource);
    if repo.is_locked(&path).await? {
        return Err(Problem::locked());
    }

    let token = format!("opaquelocktoken:{}", Uuid::now_v7());
    repo.create(&token, &path, None, &depth).await?;
    Ok(lock_response(&token, &depth))
}

/// `UNLOCK` — richiede l'header `Lock-Token: <token>` (RFC 4918 §9.11).
///
/// Usa `DavLockRepo::refresh` come test-and-set: se il token esiste ed è
/// ancora attivo lo rinnova (effetto collaterale innocuo, la riga viene
/// cancellata immediatamente dopo) e restituisce `true`, così l'unica altra
/// via — un token scaduto o mai esistito — è indistinguibile da qui in
/// avanti, ed entrambe diventano `404`. Evita di introdurre nella spec un
/// quinto metodo di repository non richiesto dal brief solo per questo
/// controllo.
///
/// # Errors
/// `400` se l'header manca o non porta un token leggibile. `404` se il
/// token non esiste o è già scaduto.
pub async fn unlock(
    state: &AppState,
    lock_token_header: Option<&str>,
) -> Result<Response, Problem> {
    let token = lock_token_header
        .and_then(strip_angle_brackets)
        .ok_or_else(|| {
            Problem::bad_request("missing-lock-token", "UNLOCK requires a Lock-Token header")
        })?;

    let repo = DavLockRepo::new(&state.db);
    if repo.refresh(&token).await? {
        repo.delete(&token).await?;
        Ok(StatusCode::NO_CONTENT.into_response())
    } else {
        Err(Problem::not_found())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn if_header_token_is_extracted_from_parentheses_and_angle_brackets() {
        assert_eq!(
            parse_if_token("(<opaquelocktoken:abc-123>)"),
            Some("opaquelocktoken:abc-123".to_owned())
        );
    }

    #[test]
    fn lock_token_header_is_extracted_from_angle_brackets() {
        assert_eq!(
            strip_angle_brackets("<opaquelocktoken:abc-123>"),
            Some("opaquelocktoken:abc-123".to_owned())
        );
    }

    #[test]
    fn an_empty_lock_token_header_is_rejected() {
        assert_eq!(strip_angle_brackets("<>"), None);
        assert_eq!(strip_angle_brackets(""), None);
    }
}
