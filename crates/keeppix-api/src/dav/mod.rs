//! `WebDAV` — dispatcher and authentication. Authenticates with an
//! app-password via Basic Auth (`401` with no redirect for missing or
//! wrong credentials), then dispatches `PROPFIND`/`GET`, `PUT`/`MKCOL`/`MOVE`
//! and `DELETE`/`LOCK`/`UNLOCK`; `COPY` stays `501` — see `write.rs` for why
//! it isn't implemented.
//!
//! Never a session cookie here: `WebDAV` clients (Finder, rclone, …) only
//! speak Basic Auth, and the only credentials accepted are app-passwords
//! (`AppPasswordRepo::verify`) — never the login password.
//!
//! **Path → resource**: navigation is by `id`, not by name —
//! `/dav/folder/{folder_id}` and `/dav/asset/{asset_id}` — instead of
//! resolving a hierarchy of names against `ltree`. Cost: Finder (which
//! navigates by human-readable name) doesn't work yet; rclone and Cyberduck
//! do, because they compare the `ETag`, not the path. The write module
//! extends the scheme with `/dav/folder/{folder_id}/{name}` — a child not
//! yet created (`PUT`/`MKCOL`) — and with the same scheme in `MOVE`'s
//! `Destination`.

pub mod delete;
pub mod lock;
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

/// Extracts `username`/`secret` from the `Authorization: Basic` header.
/// Returns `None` if the header is missing, not `Basic`, not valid base64,
/// not UTF-8, or missing the `:` separator — never a `500` for a malformed
/// header sent by a buggy or hostile client.
fn parse_basic_auth(req: &Request<Body>) -> Option<(String, String)> {
    let header = req.headers().get(axum::http::header::AUTHORIZATION)?;
    let header = header.to_str().ok()?;
    let encoded = header.strip_prefix("Basic ")?;
    let decoded = STANDARD.decode(encoded).ok()?;
    let decoded = String::from_utf8(decoded).ok()?;
    let (username, secret) = decoded.split_once(':')?;
    Some((username.to_owned(), secret.to_owned()))
}

/// `WebDAV` resource addressed by a path under `/dav/`. See the module
/// comment on the "by id, not by name" simplification.
///
/// `FolderChild` is a not-yet-created child of a folder —
/// `/dav/folder/{folder_id}/{name}` — addressed by `PUT` (the name is a
/// filename) and by `MKCOL` (the name is the new subfolder), and by
/// `MOVE`'s `Destination`.
#[derive(Debug, Clone)]
pub(crate) enum Resource {
    Folder(FolderId),
    Asset(AssetId),
    FolderChild(FolderId, String),
}

/// `None` for any path that isn't `folder/{uuid}`, `folder/{uuid}/{name}`,
/// or `asset/{uuid}` (with or without a trailing `/`) — including a
/// malformed id, an empty name or one containing separators after
/// percent-decoding, or more than one level of nesting: this responds
/// `501` as an unimplemented method, not a different error that would
/// reveal whether an id is syntactically valid.
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

/// A single path component: not empty, not `.`/`..`, and containing no
/// separator — otherwise a percent-decoded name could make the written
/// file escape the intended folder (traversal). Same rules as
/// `keeppix_domain::AssetName::parse`, replicated here because they also
/// apply to a folder name (`MKCOL`), not just a filename.
fn is_valid_path_component(name: &str) -> bool {
    !name.is_empty()
        && !name.contains('/')
        && !name.contains('\\')
        && !name.contains('\0')
        && name != "."
        && name != ".."
}

/// Decodes percent-encoding (RFC 3986 §2.1): a real `WebDAV` client (Finder,
/// davfs2) percent-encodes spaces and other non-ASCII characters in the
/// path of a filename with spaces. `None` for a non-hex `%XX` sequence or
/// for bytes that don't recompose valid UTF-8 — never a panic on hostile
/// input.
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

/// Extracts `(dst_parent_id, name)` from `MOVE`'s `Destination` header
/// (RFC 4918 §9.9.2): the client typically sends an absolute URI
/// (`http://host/dav/folder/{id}/{name}`), sometimes just the path — both
/// are accepted, stripping the scheme and authority when present.
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

/// Main `WebDAV` handler. Authenticates first, then dispatches by method
/// and resource. `COPY` isn't implemented and stays `501`.
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
    // The caller's real role isn't looked up with a separate query —
    // `SystemRole::User` here just means "don't treat as admin".
    // `FolderRepo`/`AssetRepo` still filter by `user_id`, not by role: an
    // administrator using WebDAV sees their owned libraries and shares like
    // any other owner, and any id that doesn't belong to them responds
    // `Forbidden` — never `NotFound`, because `ctx.is_admin()` is never
    // true for a WebDAV actor here. Cost if wrong: a real admin would lose
    // the "all-seeing" visibility over WebDAV that they have in the web
    // app — no security risk, just a feature not yet replicated there.
    let ctx = AuthContext::user(user_id, SystemRole::User);

    let path = req.uri().path().strip_prefix("/dav/").unwrap_or("");
    let Some(resource) = parse_resource(path) else {
        return not_implemented();
    };

    // The method and headers we need are extracted here, as owned values:
    // from this point `req` can be consumed for the body (`PUT`) without a
    // borrow conflict with `req.method()`/`req.headers()`.
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
            let content_length = headers
                .get(header::CONTENT_LENGTH)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok());
            respond(write::put(&state, &ctx, folder_id, &name, content_length, body).await)
        }
        ("MKCOL", Resource::FolderChild(parent_id, name)) => {
            respond(write::mkcol(&state, &ctx, parent_id, &name).await)
        }
        ("MOVE", Resource::Folder(src_id)) => {
            // The final name in `Destination` is not applied: `MOVE` here
            // moves between parents, it doesn't rename (`move_subtree`
            // doesn't support a rename-in-place). A client that also asks
            // for a different name sees the old name survive under the
            // new parent.
            let Some((dst_parent_id, _name)) = destination_header(&headers) else {
                return Problem::bad_request(
                    "missing-destination",
                    "Destination header must be a /dav/folder/{id}/{name} path or URI",
                )
                .into_response();
            };
            respond(write::move_folder(&state, &ctx, src_id, dst_parent_id).await)
        }
        ("DELETE", Resource::Asset(id)) => respond(delete::asset(&state, &ctx, id).await),
        ("DELETE", Resource::Folder(id)) => respond(delete::folder(&state, &ctx, id).await),
        ("LOCK", resource) => {
            let depth_header = headers.get("depth").and_then(|v| v.to_str().ok());
            let if_header = headers.get("if").and_then(|v| v.to_str().ok());
            respond(lock::lock(&state, &ctx, &resource, depth_header, if_header).await)
        }
        // `UNLOCK` doesn't look at which resource: the token in the
        // `Lock-Token` header is the only identity that matters (RFC 4918
        // §9.11) — a `Resource` that fails to parse for this path doesn't
        // block `UNLOCK`, because the dispatch doesn't even get here in
        // that case (see `parse_resource` above, which responds `501`
        // before the `match`).
        ("UNLOCK", _) => {
            let lock_token = headers.get("lock-token").and_then(|v| v.to_str().ok());
            respond(lock::unlock(&state, lock_token).await)
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
