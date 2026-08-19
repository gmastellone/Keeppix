//! `WebDAV` — dispatcher e autenticazione. Autentica con app-password via
//! Basic Auth (`401` senza redirect per credenziali assenti o sbagliate),
//! poi dispatcha `PROPFIND` e `GET` (Task 6); ogni altro metodo resta
//! `501` fino ai Task 7-8.
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

pub mod propfind;

use std::str::FromStr;

use axum::body::Body;
use axum::extract::State;
use axum::http::{Request, StatusCode, header};
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
#[derive(Debug, Clone, Copy)]
enum Resource {
    Folder(FolderId),
    Asset(AssetId),
}

/// `None` per qualunque path che non sia `folder/{uuid}` o `asset/{uuid}`
/// (con o senza `/` finale) — compreso un id malformato: risponde `501`
/// come un metodo non implementato, non un errore diverso che rivelerebbe
/// se un id è sintatticamente valido.
fn parse_resource(path: &str) -> Option<Resource> {
    let trimmed = path.trim_matches('/');
    let (kind, id) = trimmed.split_once('/')?;
    match kind {
        "folder" => FolderId::from_str(id).ok().map(Resource::Folder),
        "asset" => AssetId::from_str(id).ok().map(Resource::Asset),
        _ => None,
    }
}

/// Handler `WebDAV` principale. Autentica prima, poi dispatcha per metodo
/// e risorsa. Metodi/risorse non ancora supportati (Task 7-8: PUT, MKCOL,
/// MOVE, COPY, DELETE, LOCK, UNLOCK) restano `501`.
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

    match (req.method().as_str(), resource) {
        ("PROPFIND", Resource::Folder(id)) => {
            let depth_header = req.headers().get("depth").and_then(|v| v.to_str().ok());
            respond(propfind_folder(&state, &ctx, id, depth_header).await)
        }
        ("PROPFIND", Resource::Asset(id)) => respond(propfind::asset(&state, &ctx, id).await),
        ("GET", Resource::Asset(id)) => {
            let range = req
                .headers()
                .get(header::RANGE)
                .and_then(|v| v.to_str().ok());
            respond(get_asset(&state, &ctx, id, range).await)
        }
        _ => not_implemented(),
    }
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
