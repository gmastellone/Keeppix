use axum::Json;
use axum::extract::rejection::JsonRejection;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use keeppix_db::DbError;
use serde::Serialize;

/// RFC 9457-formatted error. The `type` field is a stable code clients can
/// branch on; `title` is in English and is meant for debugging, not the
/// end user — translation happens in the frontend.
///
/// It's also a schema in the `OpenAPI` document: it's the shape the mobile
/// client branches on, so it needs to be described, not just served.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct Problem {
    #[schema(example = "keeppix/unauthenticated")]
    #[serde(rename = "type")]
    pub type_slug: String,
    pub title: String,
    pub status: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(skip)]
    status_code: StatusCode,
    /// Seconds to put in `Retry-After`. Not part of the body: it's a
    /// header, and RFC 9457 doesn't include it among the document's
    /// members.
    #[serde(skip)]
    retry_after_secs: Option<u32>,
}

impl Problem {
    #[must_use]
    pub fn new(status: StatusCode, type_slug: &str, title: &str) -> Self {
        Self {
            type_slug: format!("keeppix/{type_slug}"),
            title: title.to_owned(),
            status: status.as_u16(),
            detail: None,
            status_code: status,
            retry_after_secs: None,
        }
    }

    #[must_use]
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// Adds `Retry-After`: tells the client the error is transient and when
    /// it's worth retrying, instead of leaving it to guess.
    #[must_use]
    pub const fn with_retry_after(mut self, secs: u32) -> Self {
        self.retry_after_secs = Some(secs);
        self
    }

    #[must_use]
    pub fn not_found() -> Self {
        Self::new(StatusCode::NOT_FOUND, "not-found", "Resource not found")
    }

    #[must_use]
    pub fn unauthenticated() -> Self {
        Self::new(
            StatusCode::UNAUTHORIZED,
            "unauthenticated",
            "Authentication required",
        )
    }

    #[must_use]
    pub fn forbidden() -> Self {
        Self::new(StatusCode::FORBIDDEN, "forbidden", "Not allowed")
    }

    #[must_use]
    pub fn bad_request(type_slug: &str, title: &str) -> Self {
        Self::new(StatusCode::BAD_REQUEST, type_slug, title)
    }

    #[must_use]
    pub fn internal() -> Self {
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal-error",
            "Unexpected server error",
        )
    }

    /// Transient error: the database isn't responding. **Must not** be used
    /// for an invalid session — a `401` tells the frontend "log in again",
    /// and sending it during a Postgres restart would disconnect everyone.
    #[must_use]
    pub fn service_unavailable() -> Self {
        Self::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "service-unavailable",
            "Service temporarily unavailable",
        )
        .with_retry_after(RETRY_AFTER_SECS)
    }

    /// Method not allowed on a path that exists. Brings axum's would-be
    /// empty-body `405` inside the RFC 9457 contract.
    #[must_use]
    pub fn method_not_allowed() -> Self {
        Self::new(
            StatusCode::METHOD_NOT_ALLOWED,
            "method-not-allowed",
            "Method not allowed on this path",
        )
    }

    #[must_use]
    pub fn too_many_requests() -> Self {
        Self::new(
            StatusCode::TOO_MANY_REQUESTS,
            "rate-limited",
            "Too many requests, please slow down",
        )
        .with_retry_after(60)
    }

    /// Mutation missing the required custom header (see `crate::csrf`).
    #[must_use]
    pub fn csrf_check_failed() -> Self {
        Self::new(
            StatusCode::FORBIDDEN,
            "csrf-check-failed",
            "Missing required client header",
        )
    }

    #[must_use]
    pub fn payload_too_large() -> Self {
        Self::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "payload-too-large",
            "Upload exceeds the share link quota",
        )
    }

    /// Not enough free space on the library filesystem for
    /// `expected_size`: rejected at session creation, not discovered
    /// halfway through the upload.
    #[must_use]
    pub fn insufficient_storage() -> Self {
        Self::new(
            StatusCode::INSUFFICIENT_STORAGE,
            "insufficient-storage",
            "Not enough free space on the library filesystem",
        )
    }

    /// The upload session has expired and has already been cleaned up:
    /// distinct from `404`, because the caller didn't get the id wrong —
    /// it had seen it.
    #[must_use]
    pub fn gone() -> Self {
        Self::new(
            StatusCode::GONE,
            "upload-session-expired",
            "The upload session has expired",
        )
    }

    /// `HEAD`/`PATCH` with `Upload-Offset` different from the actual
    /// `received_bytes` ("the truth always lives on the server"): never a
    /// silent acceptance that corrupts the file.
    #[must_use]
    pub fn offset_mismatch(expected: i64) -> Self {
        Self::new(
            StatusCode::CONFLICT,
            "upload-offset-mismatch",
            "Upload-Offset does not match the session's received bytes",
        )
        .with_detail(format!("expected {expected}"))
    }

    /// `WebDAV LOCK` with `If:` on an expired or nonexistent token: the
    /// client asked to renew a lock that isn't (or is no longer) theirs —
    /// a silent `200` would make it believe it still held the lock.
    #[must_use]
    pub fn precondition_failed() -> Self {
        Self::new(
            StatusCode::PRECONDITION_FAILED,
            "dav-lock-precondition-failed",
            "The lock token in the If header is missing or expired",
        )
    }

    /// `WebDAV LOCK` with no `If:` on a resource already locked by
    /// another active token.
    #[must_use]
    pub fn locked() -> Self {
        Self::new(
            StatusCode::LOCKED,
            "dav-resource-locked",
            "The resource already has an active lock",
        )
    }

    /// `460`, custom: the chunk's checksum doesn't match. Not an IANA
    /// code — Nginx uses it for a client-closed request, but it's free
    /// in our tus-style protocol — the chunk is **not** written, the
    /// client can resend it without losing its previous offset.
    #[must_use]
    pub fn chunk_checksum_mismatch() -> Self {
        Self::new(
            StatusCode::from_u16(460).unwrap_or(StatusCode::CONFLICT),
            "chunk-checksum-mismatch",
            "Upload-Checksum does not match the chunk body",
        )
    }
}

/// Suggested wait on `503`: enough for a brief Postgres restart, short
/// enough that a mobile app doesn't appear to hang.
const RETRY_AFTER_SECS: u32 = 5;

impl IntoResponse for Problem {
    fn into_response(self) -> Response {
        let status = self.status_code;
        let retry_after = self.retry_after_secs;
        let mut response = (status, Json(self)).into_response();
        response.headers_mut().insert(
            axum::http::header::CONTENT_TYPE,
            axum::http::HeaderValue::from_static("application/problem+json"),
        );
        if let Some(secs) = retry_after
            && let Ok(value) = axum::http::HeaderValue::from_str(&secs.to_string())
        {
            response
                .headers_mut()
                .insert(axum::http::header::RETRY_AFTER, value);
        }
        response
    }
}

/// axum's built-in rejections come out with a `text/plain` body and no
/// stable `type`: a wrong `Content-Type` or a malformed body would produce
/// an error response outside the RFC 9457 contract, which the mobile
/// client can't interpret. The conversion lives here, not in the handlers,
/// because the `crate::json::Json` extractor uses it for every route.
impl From<JsonRejection> for Problem {
    fn from(rejection: JsonRejection) -> Self {
        let problem = match &rejection {
            JsonRejection::MissingJsonContentType(_) => Self::new(
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "unsupported-media-type",
                "Expected Content-Type: application/json",
            ),
            // Broken syntax (`400`), unexpected shape or missing field
            // (`422`), unreadable body: to the client these are the same
            // problem — "the body you sent isn't the JSON this route
            // accepts" — and axum's status already distinguishes the cases.
            _ => Self::new(rejection.status(), "invalid-json", "Invalid JSON body"),
        };
        // `body_text()` is axum's message, in English and meant for the
        // developer ("missing field `username`"): exactly what `detail`
        // should contain. It describes the *request* body, so it reveals
        // nothing about the server's state.
        problem.with_detail(rejection.body_text())
    }
}

impl From<DbError> for Problem {
    fn from(err: DbError) -> Self {
        match err {
            DbError::NotFound => Self::not_found(),
            DbError::Forbidden => Self::forbidden(),
            DbError::Conflict(msg) | DbError::Collision(msg) => {
                Self::new(StatusCode::CONFLICT, "conflict", "Conflict").with_detail(msg)
            }
            DbError::Connection(e) => {
                tracing::error!(error = %e, "database unavailable");
                Self::service_unavailable()
            }
            DbError::InsufficientStorage => Self::insufficient_storage(),
            DbError::Gone => Self::gone(),
            // Internal details stay in the logs, not in the response.
            other => {
                tracing::error!(error = %other, "database error");
                Self::internal()
            }
        }
    }
}
