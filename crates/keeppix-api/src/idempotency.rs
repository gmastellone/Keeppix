use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::body::{Body, Bytes, to_bytes};
use axum::extract::{Request, State};
use axum::http::{HeaderValue, Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum_extra::extract::CookieJar;
use keeppix_db::{
    CachedIdempotencyResponse, IdempotencyLookupResult, IdempotencyRepo,
    IdempotencyRequestFingerprint, SessionRepo,
};
use keeppix_domain::{SessionToken, UserId};

use crate::extract::{SESSION_COOKIE, session_problem};
use crate::problem::Problem;
use crate::state::AppState;

pub const IDEMPOTENCY_HEADER: &str = "idempotency-key";
const BODY_LIMIT: usize = 1024 * 1024;

#[derive(Clone, Default)]
pub struct IdempotencyLockStore {
    inner: Arc<Mutex<HashMap<String, std::sync::Weak<tokio::sync::Mutex<()>>>>>,
}

impl IdempotencyLockStore {
    #[must_use]
    pub fn lock_for(&self, key: &str) -> Arc<tokio::sync::Mutex<()>> {
        let Ok(mut guard) = self.inner.lock() else {
            return Arc::new(tokio::sync::Mutex::new(()));
        };
        guard.retain(|_, slot| slot.strong_count() > 0);
        if let Some(existing) = guard.get(key).and_then(std::sync::Weak::upgrade) {
            return existing;
        }
        let lock = Arc::new(tokio::sync::Mutex::new(()));
        guard.insert(key.to_owned(), Arc::downgrade(&lock));
        lock
    }
}

pub async fn apply(State(state): State<AppState>, mut req: Request, next: Next) -> Response {
    if !matches!(
        *req.method(),
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    ) {
        return next.run(req).await;
    }

    let Some(key) = req
        .headers()
        .get(IDEMPOTENCY_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
    else {
        return next.run(req).await;
    };

    let path = req.uri().path().to_owned();
    let body = match extract_body(&mut req).await {
        Ok(Some(bytes)) => bytes,
        Ok(None) => return next.run(req).await,
        Err(problem) => return problem.into_response(),
    };
    let fingerprint = IdempotencyRequestFingerprint {
        method: req.method().as_str().to_owned(),
        path,
        body_hash: blake3::hash(&body).to_hex().to_string(),
    };

    let lock = state.idempotency_locks.lock_for(&key);
    let _guard = lock.lock().await;
    let repo = IdempotencyRepo::new(&state.db);

    // Resolve auth *after* we have the key+fingerprint so a refresh retry with
    // a consumed cookie can still replay the cached Set-Cookie.
    let user_id = match session_user_id(&state, req.headers()).await {
        Ok(Some(user_id)) => user_id,
        Ok(None) => {
            // No cookie at all — leave unauthenticated routes alone.
            *req.body_mut() = Body::from(body);
            return next.run(req).await;
        }
        Err(auth_problem) => {
            return match repo.lookup_by_key(&key, &fingerprint).await {
                Ok(IdempotencyLookupResult::Replay { status, response }) => {
                    replay(status, response)
                }
                Ok(IdempotencyLookupResult::Conflict) => Problem::new(
                    StatusCode::CONFLICT,
                    "idempotency-key-conflict",
                    "Idempotency-Key was already used for a different request",
                )
                .into_response(),
                Ok(IdempotencyLookupResult::Missing) => auth_problem.into_response(),
                Err(error) => Problem::from(error).into_response(),
            };
        }
    };

    match repo.lookup(user_id, &key, &fingerprint).await {
        Ok(IdempotencyLookupResult::Replay { status, response }) => {
            return replay(status, response);
        }
        Ok(IdempotencyLookupResult::Conflict) => {
            return Problem::new(
                StatusCode::CONFLICT,
                "idempotency-key-conflict",
                "Idempotency-Key was already used for a different request",
            )
            .into_response();
        }
        Ok(IdempotencyLookupResult::Missing) => {}
        Err(error) => return Problem::from(error).into_response(),
    }

    *req.body_mut() = Body::from(body);
    let response = next.run(req).await;
    let (response, cached) = match cacheable_response(response).await {
        Ok(pair) => pair,
        Err(problem) => return problem.into_response(),
    };

    if response.status().is_server_error() {
        return response;
    }

    if let Err(error) = repo
        .store(
            user_id,
            &key,
            fingerprint,
            response.status().as_u16(),
            cached,
        )
        .await
    {
        return Problem::from(error).into_response();
    }

    response
}

async fn session_user_id(
    state: &AppState,
    headers: &axum::http::HeaderMap,
) -> Result<Option<UserId>, Problem> {
    let jar = CookieJar::from_headers(headers);
    let Some(cookie) = jar.get(SESSION_COOKIE) else {
        return Ok(None);
    };
    let token = SessionToken::from_string(cookie.value().to_owned());
    if let Some(ctx) = state.sessions.get(&token) {
        return Ok(ctx.user_id());
    }
    let ctx = SessionRepo::new(&state.db)
        .authenticate(&token)
        .await
        .map_err(session_problem)?;
    state.sessions.put(&token, ctx.clone());
    Ok(ctx.user_id())
}

async fn extract_body(req: &mut Request) -> Result<Option<Bytes>, Problem> {
    let body = std::mem::replace(req.body_mut(), Body::empty());
    let bytes = to_bytes(body, BODY_LIMIT).await.map_err(|_| {
        Problem::bad_request("idempotency-body-too-large", "Request body is too large")
    })?;
    let supported = bytes.is_empty()
        || req
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.starts_with("application/json"));
    if !supported {
        *req.body_mut() = Body::from(bytes);
        return Ok(None);
    }
    Ok(Some(bytes))
}

async fn cacheable_response(
    response: Response,
) -> Result<(Response, CachedIdempotencyResponse), Problem> {
    let set_cookie = response
        .headers()
        .get(header::SET_COOKIE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let status = response.status();
    let (parts, body) = response.into_parts();
    let bytes = to_bytes(body, BODY_LIMIT)
        .await
        .map_err(|_| Problem::internal())?;
    let cached = if bytes.is_empty() {
        CachedIdempotencyResponse {
            body: None,
            set_cookie,
        }
    } else {
        CachedIdempotencyResponse {
            body: Some(serde_json::from_slice(&bytes).map_err(|_| Problem::internal())?),
            set_cookie,
        }
    };
    let rebuilt = Response::from_parts(parts, Body::from(bytes));
    debug_assert_eq!(rebuilt.status(), status);
    Ok((rebuilt, cached))
}

fn replay(status: u16, response: CachedIdempotencyResponse) -> Response {
    let status = StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let has_body = response.body.is_some();
    let mut replay = match response.body {
        Some(body) => (status, crate::Json(body)).into_response(),
        None => status.into_response(),
    };
    if let Some(value) = response.set_cookie.as_deref().and_then(header_value) {
        replay.headers_mut().insert(header::SET_COOKIE, value);
    }
    if has_body {
        let content_type = if status.is_client_error() || status.is_server_error() {
            "application/problem+json"
        } else {
            "application/json"
        };
        replay
            .headers_mut()
            .insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    }
    replay
}

fn header_value(value: &str) -> Option<HeaderValue> {
    HeaderValue::from_str(value).ok()
}
