//! Metà server-side della difesa CSRF richiesta dallo spec §9.5.
//!
//! Lo spec chiede tre cose insieme: `SameSite=Lax` sul cookie, obbligo di
//! `Content-Type: application/json` e un header custom sulle mutazioni. Le
//! prime due c'erano già (`cookie.rs` e l'extractor `crate::json::Json`, che
//! rifiuta con `415` un corpo non-JSON); questo modulo aggiunge la terza, che
//! è anche l'unica che copre le mutazioni **senza corpo** —
//! `POST /auth/refresh` e `POST /auth/logout` non passano da `Json<T>`.
//!
//! La proprietà comprata è precisa: un `<form>` HTML su un sito ostile può
//! inviare una POST cross-site con i cookie (per questo `SameSite=Lax` non
//! basta contro un attaccante *same-site*: `evil.example.com` e
//! `photos.example.com` sono lo stesso sito per `SameSite`, e il prefisso
//! `__Host-` impedisce di *impostare* il cookie per il dominio, non di
//! *inviarlo*), ma **non può impostare un header custom** — servirebbe
//! `fetch`/`XHR`, cioè il preflight CORS, che l'istanza non concede.
//!
//! Il controllo è un layer e non un controllo per handler, così le rotte che la
//! Fase 1 aggiunge dentro `api_routes()` sono coperte senza doversene
//! ricordare. Deroghe da prevedere quando arriveranno: `WebDAV` (Fase 5: i
//! client sono Finder e rclone, che non mandano header di Keeppix) e gli
//! upload **tus** (`application/offset+octet-stream`); entrambi vivranno fuori
//! da `/api/v1` e quindi fuori da questo layer, ma la decisione va presa
//! esplicitamente, non per distrazione.

use axum::extract::Request;
use axum::http::Method;
use axum::middleware::Next;
use axum::response::{IntoResponse as _, Response};

use crate::problem::Problem;

/// Header che ogni client di Keeppix invia sulle mutazioni. Il frontend lo
/// imposta in `apiFetch` (`frontend/src/api/client.ts`) su **tutte** le
/// chiamate; il valore non conta, conta che un form cross-site non possa
/// produrlo.
pub const CLIENT_HEADER: &str = "x-keeppix-client";

/// Rifiuta con `403 keeppix/csrf-check-failed` una mutazione priva di
/// `x-keeppix-client`. I metodi sicuri (`GET`, `HEAD`, `OPTIONS`) passano
/// sempre: non cambiano stato, e richiedere l'header su di essi romperebbe
/// l'apertura diretta di un URL.
pub async fn require_client_header(req: Request, next: Next) -> Response {
    // Deroga per `/dav/*` (Fase 5): i client WebDAV (Finder, rclone, …) non
    // mandano mai `x-keeppix-client`, e non c'è cookie di sessione da
    // proteggere lì — l'autenticazione è Basic Auth con app-password. La
    // deroga è per prefisso di path, non per assenza di cookie: non
    // restringe `/api/v1`, che resta coperto per intero.
    if req.uri().path().starts_with("/dav/") {
        return next.run(req).await;
    }

    let mutating = matches!(
        *req.method(),
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    );

    if mutating && !req.headers().contains_key(CLIENT_HEADER) {
        return Problem::csrf_check_failed()
            .with_detail(format!(
                "{CLIENT_HEADER} is required on state-changing requests"
            ))
            .into_response();
    }

    next.run(req).await
}
