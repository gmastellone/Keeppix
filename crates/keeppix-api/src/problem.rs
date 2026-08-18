use axum::Json;
use axum::extract::rejection::JsonRejection;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use keeppix_db::DbError;
use serde::Serialize;

/// Errore in formato RFC 9457. Il campo `type` è un codice stabile su cui i
/// client possono ramificare; `title` è in inglese e serve al debug, non
/// all'utente finale — la traduzione avviene nel frontend.
///
/// È anche uno schema del documento `OpenAPI`: è la forma su cui il client
/// mobile ramifica, quindi va descritta, non solo servita.
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
    /// Secondi da mettere in `Retry-After`. Non fa parte del corpo: è un
    /// header, e RFC 9457 non lo prevede fra i membri del documento.
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

    /// Aggiunge `Retry-After`: dice al client che l'errore è transitorio e
    /// quando conviene riprovare, invece di lasciarglielo indovinare.
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

    /// Errore transitorio: il database non risponde. **Non** va usato per una
    /// sessione non valida — un `401` dice al frontend «rifai il login», e
    /// mandarlo durante un riavvio di Postgres disconnetterebbe tutti.
    #[must_use]
    pub fn service_unavailable() -> Self {
        Self::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "service-unavailable",
            "Service temporarily unavailable",
        )
        .with_retry_after(RETRY_AFTER_SECS)
    }

    /// Metodo non ammesso su un percorso che esiste. Serve a portare dentro il
    /// contratto RFC 9457 il `405` che axum genererebbe con il corpo vuoto.
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

    /// Mutazione priva dell'header custom richiesto (vedi `crate::csrf`).
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
}

/// Attesa suggerita su `503`: abbastanza per un riavvio breve di Postgres,
/// abbastanza poco perché un'app mobile non sembri bloccata.
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

/// Le rejection integrate di axum escono con un corpo `text/plain` e nessun
/// `type` stabile: un `Content-Type` sbagliato o un corpo malformato darebbero
/// una risposta d'errore fuori dal contratto RFC 9457, che il client mobile non
/// sa interpretare. La conversione vive qui, non negli handler, perché è
/// l'extractor `crate::json::Json` a usarla per tutte le rotte, comprese quelle
/// che la Fase 1 aggiungerà.
impl From<JsonRejection> for Problem {
    fn from(rejection: JsonRejection) -> Self {
        let problem = match &rejection {
            JsonRejection::MissingJsonContentType(_) => Self::new(
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "unsupported-media-type",
                "Expected Content-Type: application/json",
            ),
            // Sintassi rotta (`400`), forma inattesa o campo mancante (`422`),
            // corpo illeggibile: per il client sono lo stesso problema — «il
            // corpo che hai mandato non è il JSON che questa rotta accetta» —
            // e lo status di axum distingue già i casi.
            _ => Self::new(rejection.status(), "invalid-json", "Invalid JSON body"),
        };
        // `body_text()` è il messaggio di axum, in inglese e pensato per lo
        // sviluppatore («missing field `username`»): esattamente ciò che
        // `detail` deve contenere. Descrive il corpo della *richiesta*, quindi
        // non rivela nulla dello stato del server.
        problem.with_detail(rejection.body_text())
    }
}

impl From<DbError> for Problem {
    fn from(err: DbError) -> Self {
        match err {
            DbError::NotFound => Self::not_found(),
            DbError::Forbidden => Self::forbidden(),
            DbError::Conflict(msg) => {
                Self::new(StatusCode::CONFLICT, "conflict", "Conflict").with_detail(msg)
            }
            DbError::Connection(e) => {
                tracing::error!(error = %e, "database unavailable");
                Self::service_unavailable()
            }
            // I dettagli interni restano nei log, non nella risposta.
            other => {
                tracing::error!(error = %other, "database error");
                Self::internal()
            }
        }
    }
}
