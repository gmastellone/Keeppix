//! `WebDAV` — dispatcher e autenticazione. Autentica con app-password via
//! Basic Auth (`401` senza redirect per credenziali assenti o sbagliate),
//! poi dispatcha `PROPFIND`/`GET` (Task 6) e `PUT`/`MKCOL`/`MOVE` (Task 7);
//! `COPY`, `DELETE`, `LOCK`, `UNLOCK` restano `501` — vedi il ledger del
//! Task 7 sul perché `COPY` non è implementata.
//!
//! Mai cookie di sessione qui: i client `WebDAV` (Finder, rclone, …) parlano
//! solo Basic Auth, e le uniche credenziali accettate sono le app-password
//! (`AppPasswordRepo::verify`) — mai la password di login.
//!
//! **Path → risorsa (Task 6, semplificazione documentata nel ledger)**: si
//! naviga per `id`, non per nome — `/dav/folder/{folder_id}` e
//! `/dav/asset/{asset_id}` — invece di risolvere una gerarchia di nomi
//! contro `ltree`. Costo: Finder (che naviga per nome umano) non funziona
//! ancora; rclone e Cyberduck sì, perché confrontano l'`ETag`, non il path.
//! Task 7 estende lo schema con `/dav/folder/{folder_id}/{name}` — un
//! figlio non ancora creato (`PUT`/`MKCOL`) — e con lo stesso schema nella
//! `Destination` di `MOVE`.

pub mod propfind;
pub mod write;

use std::str::FromStr;

use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, Request, StatusCode, header};
use axum::response::{IntoResponse, Response};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use keeppix_db::{AppPasswordRepo, AssetRepo, FolderRepo};
use keeppix_domain::{AssetId, AuthContext, FolderId, SystemRole};

use crate::problem::Problem;
use crate::routes::media::{mime_for_name, stream_file};
use crate::state::AppState;

/// Estrae `username`/`secret` dall'header `Authorization: Basic`.
/// Restituisce `None` se l'header è assente, non `Basic`, non valido
/// base64, non UTF-8, o privo del separatore `:` — mai un `500` per un
/// header malformato inviato da un client bacato o ostile.
fn parse_basic_auth(req: &Request<Body>) -> Option<(String, String)> {
    let header = req.headers().get(axum::http::header::AUTHORIZATION)?;
    let header = header.to_str().ok()?;
    let encoded = header.strip_prefix("Basic ")?;
    let decoded = STANDARD.decode(encoded).ok()?;
    let decoded = String::from_utf8(decoded).ok()?;
    let (username, secret) = decoded.split_once(':')?;
    Some((username.to_owned(), secret.to_owned()))
}

/// Risorsa `WebDAV` indirizzata da un path sotto `/dav/`. Vedi il commento
/// di modulo sulla semplificazione "per id, non per nome" del Task 6.
///
/// `FolderChild` (Task 7) è un figlio non ancora creato di una cartella —
/// `/dav/folder/{folder_id}/{name}` — indirizzato da `PUT` (il nome è un
/// filename) e da `MKCOL` (il nome è la nuova sotto-cartella), e dalla
/// `Destination` di `MOVE`.
#[derive(Debug, Clone)]
enum Resource {
    Folder(FolderId),
    Asset(AssetId),
    FolderChild(FolderId, String),
}

/// `None` per qualunque path che non sia `folder/{uuid}`, `folder/{uuid}/{name}`
/// o `asset/{uuid}` (con o senza `/` finale) — compreso un id malformato, un
/// nome vuoto/con separatori dopo la decodifica percentuale, o più di un
/// livello di annidamento: risponde `501` come un metodo non implementato,
/// non un errore diverso che rivelerebbe se un id è sintatticamente valido.
fn parse_resource(path: &str) -> Option<Resource> {
    let trimmed = path.trim_matches('/');
    let (kind, rest) = trimmed.split_once('/')?;
    match kind {
        "folder" => parse_folder_resource(rest),
        "asset" => AssetId::from_str(rest).ok().map(Resource::Asset),
        _ => None,
    }
}

fn parse_folder_resource(rest: &str) -> Option<Resource> {
    match rest.split_once('/') {
        None => FolderId::from_str(rest).ok().map(Resource::Folder),
        Some((id, name)) => {
            let folder_id = FolderId::from_str(id).ok()?;
            let name = percent_decode(name)?;
            is_valid_path_component(&name).then_some(Resource::FolderChild(folder_id, name))
        }
    }
}

/// Un solo componente di percorso: né vuoto, né `.`/`..`, né contenente un
/// separatore — altrimenti un nome percent-decoded potrebbe far uscire il
/// file scritto dalla cartella prevista (traversal). Stesse regole di
/// `keeppix_domain::AssetName::parse`, replicate qui perché si applicano
/// anche al nome di una cartella (`MKCOL`), non solo a un filename.
fn is_valid_path_component(name: &str) -> bool {
    !name.is_empty()
        && !name.contains('/')
        && !name.contains('\\')
        && !name.contains('\0')
        && name != "."
        && name != ".."
}

/// Decodifica percent-encoding (RFC 3986 §2.1): un client `WebDAV` reale
/// (Finder, davfs2) percent-encoda gli spazi e gli altri caratteri non-ASCII
/// nel path di un nome file con spazi. `None` per una sequenza `%XX` non
/// esadecimale o per byte che non ricompongono UTF-8 valido — mai un
/// panic su un input ostile.
fn percent_decode(input: &str) -> Option<String> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = input.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// Estrae `(dst_parent_id, name)` dall'header `Destination` di `MOVE`
/// (RFC 4918 §9.9.2): il client manda tipicamente un URI assoluto
/// (`http://host/dav/folder/{id}/{name}`), a volte solo il path — entrambi
/// accettati, spogliando schema e autorità quando presenti.
fn parse_destination(value: &str) -> Option<(FolderId, String)> {
    let path = strip_origin(value);
    let path = path.strip_prefix("/dav/")?;
    match parse_resource(path)? {
        Resource::FolderChild(parent_id, name) => Some((parent_id, name)),
        _ => None,
    }
}

fn strip_origin(value: &str) -> &str {
    let Some(after_scheme) = value.split_once("://").map(|(_, rest)| rest) else {
        return value;
    };
    after_scheme.find('/').map_or("/", |i| &after_scheme[i..])
}

/// Handler `WebDAV` principale. Autentica prima, poi dispatcha per metodo
/// e risorsa. Metodi/risorse non ancora supportati (`COPY`, `DELETE`,
/// `LOCK`, `UNLOCK`) restano `501`.
pub async fn handler(State(state): State<AppState>, req: Request<Body>) -> Response {
    let Some((username, secret)) = parse_basic_auth(&req) else {
        return unauthorized();
    };

    let user_id = match AppPasswordRepo::new(&state.db)
        .verify(&username, &secret)
        .await
    {
        Err(_) => return Problem::internal().into_response(),
        Ok(None) => return unauthorized(),
        Ok(Some(user_id)) => user_id,
    };
    // Ruling (Task 6): il ruolo reale non viene interrogato con una query
    // separata — `SystemRole::User` qui significa solo "non trattare come
    // admin". `FolderRepo`/`AssetRepo` filtrano comunque per `user_id`, non
    // per ruolo: un amministratore che usa WebDAV vede le sue librerie
    // possedute e le condivisioni come qualunque proprietario, e ogni id
    // che non gli appartiene risponde `Forbidden` — mai `NotFound`, perché
    // `ctx.is_admin()` non è mai vero per un attore WebDAV in questo task.
    // Costo se sbagliato: un vero admin perderebbe la visibilità
    // "onnisciente" su WebDAV che ha nella web app — nessun rischio di
    // sicurezza, solo una funzionalità non ancora replicata lì.
    let ctx = AuthContext::user(user_id, SystemRole::User);

    let path = req.uri().path().strip_prefix("/dav/").unwrap_or("");
    let Some(resource) = parse_resource(path) else {
        return not_implemented();
    };

    // Il metodo e gli header che servono si estraggono qui, come valori
    // posseduti: da questo punto `req` può essere consumato per il corpo
    // (`PUT`) senza un conflitto di prestito con `req.method()`/`req.headers()`.
    let method = req.method().as_str().to_owned();
    let headers = req.headers().clone();
    let body = req.into_body();

    match (method.as_str(), resource) {
        ("PROPFIND", Resource::Folder(id)) => {
            let depth_header = headers.get("depth").and_then(|v| v.to_str().ok());
            respond(propfind_folder(&state, &ctx, id, depth_header).await)
        }
        ("PROPFIND", Resource::Asset(id)) => respond(propfind::asset(&state, &ctx, id).await),
        ("GET", Resource::Asset(id)) => {
            let range = headers.get(header::RANGE).and_then(|v| v.to_str().ok());
            respond(get_asset(&state, &ctx, id, range).await)
        }
        ("PUT", Resource::FolderChild(folder_id, name)) => {
            respond(write::put(&state, &ctx, folder_id, &name, body).await)
        }
        ("MKCOL", Resource::FolderChild(parent_id, name)) => {
            respond(write::mkcol(&state, &ctx, parent_id, &name).await)
        }
        ("MOVE", Resource::Folder(src_id)) => {
            // Il nome finale nella `Destination` non viene applicato: `MOVE`
            // qui sposta di genitore, non rinomina (ledger Task 7,
            // `move_subtree` non supporta la rinomina contestuale). Un
            // client che chiede anche un nome diverso vede il vecchio nome
            // sopravvivere sotto il nuovo genitore.
            let Some((dst_parent_id, _name)) = destination_header(&headers) else {
                return Problem::bad_request(
                    "missing-destination",
                    "Destination header must be a /dav/folder/{id}/{name} path or URI",
                )
                .into_response();
            };
            respond(write::move_folder(&state, &ctx, src_id, dst_parent_id).await)
        }
        _ => not_implemented(),
    }
}

fn destination_header(headers: &HeaderMap) -> Option<(FolderId, String)> {
    let raw = headers.get("destination")?.to_str().ok()?;
    parse_destination(raw)
}

async fn propfind_folder(
    state: &AppState,
    ctx: &AuthContext,
    folder_id: FolderId,
    depth_header: Option<&str>,
) -> Result<Response, Problem> {
    let depth = propfind::parse_depth(depth_header)?;
    propfind::folder(state, ctx, folder_id, &depth).await
}

async fn get_asset(
    state: &AppState,
    ctx: &AuthContext,
    id: AssetId,
    range: Option<&str>,
) -> Result<Response, Problem> {
    let asset = AssetRepo::new(&state.db).find_by_id(ctx, id).await?;
    let folder_path = FolderRepo::new(&state.db)
        .absolute_path(ctx, asset.folder_id)
        .await?;
    let path = folder_path.join(asset.filename.as_str());
    stream_file(&path, range, mime_for_name(asset.filename.as_str()), false).await
}

fn respond(result: Result<Response, Problem>) -> Response {
    match result {
        Ok(response) => response,
        Err(problem) => problem.into_response(),
    }
}

fn unauthorized() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [("www-authenticate", r#"Basic realm="Keeppix""#)],
    )
        .into_response()
}

fn not_implemented() -> Response {
    StatusCode::NOT_IMPLEMENTED.into_response()
}

#[cfg(test)]
mod tests {
    use super::parse_basic_auth;
    use axum::body::Body;
    use axum::http::Request;
    use base64::Engine as _;
    use base64::engine::general_purpose::STANDARD;

    #[allow(clippy::unwrap_used)]
    fn request_with_auth(value: Option<&str>) -> Request<Body> {
        let mut builder = Request::builder().uri("/dav/x");
        if let Some(value) = value {
            builder = builder.header("authorization", value);
        }
        builder.body(Body::empty()).unwrap()
    }

    #[test]
    fn missing_header_is_none() {
        assert!(parse_basic_auth(&request_with_auth(None)).is_none());
    }

    #[test]
    fn non_basic_scheme_is_none() {
        assert!(parse_basic_auth(&request_with_auth(Some("Bearer abc"))).is_none());
    }

    #[test]
    fn invalid_base64_is_none() {
        assert!(parse_basic_auth(&request_with_auth(Some("Basic not-base64!"))).is_none());
    }

    #[test]
    fn missing_colon_is_none() {
        let encoded = STANDARD.encode("no-colon-here");
        assert!(parse_basic_auth(&request_with_auth(Some(&format!("Basic {encoded}")))).is_none());
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn valid_header_splits_username_and_secret() {
        let encoded = STANDARD.encode("giovanni:s3cr3t");
        let (username, secret) =
            parse_basic_auth(&request_with_auth(Some(&format!("Basic {encoded}")))).unwrap();
        assert_eq!(username, "giovanni");
        assert_eq!(secret, "s3cr3t");
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn secret_containing_a_colon_is_preserved_whole() {
        let encoded = STANDARD.encode("giovanni:pass:with:colons");
        let (username, secret) =
            parse_basic_auth(&request_with_auth(Some(&format!("Basic {encoded}")))).unwrap();
        assert_eq!(username, "giovanni");
        assert_eq!(secret, "pass:with:colons");
    }
}
