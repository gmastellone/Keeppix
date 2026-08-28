//! `LOCK`/`UNLOCK` — `WebDAV` Class 2, required by Finder and Windows
//! Explorer before writing a file: without a response to these two
//! methods, native clients refuse to save, not just "go slower".
//!
//! No real concurrent-write arbitration happens here: the lock is a
//! contract with the client (no second writer is actually coordinated by
//! this code), it only serves to satisfy the protocol that Finder/Explorer
//! expect before a `PUT`.

use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use keeppix_db::{AssetRepo, DavLockRepo, FolderRepo};
use keeppix_domain::AuthContext;
use uuid::Uuid;

use super::Resource;
use crate::problem::Problem;
use crate::state::AppState;

/// Must stay identical to the TTL set by `DavLockRepo::create`/`refresh`
/// (`crates/keeppix-db/src/dav_locks.rs`) — this is only the value shown in
/// the response's XML body; the source of truth for expiry remains
/// `dav_locks.timeout_at` in the database.
const LOCK_TIMEOUT_SECONDS: u64 = 3600;

/// Opaque path used as the key in `dav_locks.resource_path` — the same
/// `/dav/...` path the client uses to address the resource, never a
/// filesystem path (same principle as the rest of `dav::mod`).
fn resource_key(resource: &Resource) -> String {
    match resource {
        Resource::Folder(id) => format!("/dav/folder/{id}"),
        Resource::Asset(id) => format!("/dav/asset/{id}"),
        Resource::FolderChild(parent_id, name) => format!("/dav/folder/{parent_id}/{name}"),
    }
}

/// The caller must at least be able to see the resource (or, for a
/// not-yet-created child, the parent folder) — a lock must not reveal to a
/// stranger even the existence of a resource that isn't theirs.
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

/// Extracts the token from an `If: (<token>)` header (RFC 4918 §10.4) —
/// only the single-token form, without `And`/`Or` lists: no real client in
/// use here (Finder, Windows Explorer) sends anything else for a plain
/// renewal.
fn parse_if_token(raw: &str) -> Option<String> {
    let inner = raw.trim().trim_start_matches('(').trim_end_matches(')');
    strip_angle_brackets(inner)
}

/// Extracts the token from a `Lock-Token: <token>` header (`UNLOCK`) or
/// from the value returned by `LOCK` — both carry the token between
/// `<`/`>`.
fn strip_angle_brackets(raw: &str) -> Option<String> {
    let inner = raw.trim().trim_start_matches('<').trim_end_matches('>');
    (!inner.is_empty()).then(|| inner.to_owned())
}

fn lockdiscovery_xml(token: &str, depth: &str) -> String {
    // No trailing `\` line continuations: after a `\` followed by a
    // newline, Rust also strips the leading whitespace of the next line —
    // literal spaces are needed here in the XML body, so each line is an
    // explicit `\n` on a single line of code.
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

/// `LOCK` — without `If:` creates a new exclusive lock (`423 Locked` if the
/// resource already has one active); with `If: (<token>)` attempts a
/// renewal (`412 Precondition Failed` if the token doesn't exist or has
/// already expired — never a `200` that would make the client believe it
/// still holds the lock).
///
/// # Errors
/// `403` if the caller can't see the resource (or, for a not-yet-created
/// child, the parent folder). `412` for a renewal on an expired/nonexistent
/// token. `423` for a new request on a resource already locked by another
/// active token.
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

/// `UNLOCK` — requires the `Lock-Token: <token>` header (RFC 4918 §9.11).
///
/// Uses `DavLockRepo::refresh` as a test-and-set: if the token exists and
/// is still active it renews it (a harmless side effect, since the row is
/// deleted right afterward) and returns `true`, so the only other case — a
/// token that's expired or never existed — is indistinguishable from here
/// on, and both become `404`. This avoids adding a fifth repository method
/// just for this check.
///
/// # Errors
/// `400` if the header is missing or doesn't carry a readable token. `404`
/// if the token doesn't exist or has already expired.
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
