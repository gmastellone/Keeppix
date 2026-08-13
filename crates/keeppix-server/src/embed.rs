//! Frontend incorporato nel binario: `frontend/dist` viene compilato dentro
//! l'eseguibile a tempo di compilazione (`rust-embed`), così l'immagine
//! Docker distribuisce un solo artefatto, senza un container Vite separato.

use axum::body::Body;
use axum::http::{HeaderValue, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use keeppix_api::{AppState, Problem};
use rust_embed::Embed;

#[derive(Embed)]
#[folder = "$CARGO_MANIFEST_DIR/../../frontend/dist"]
struct Assets;

/// Serve un file incorporato oppure `index.html` come fallback SPA: il
/// routing delle pagine è lato client (`vue-router`), quindi qualunque
/// percorso non riconosciuto come asset deve comunque ricevere il documento
/// che avvia l'applicazione.
///
/// I percorsi API non arrivano qui nel binario reale: sono registrati prima
/// nel router (vedi `mount`), quindi non cadono mai nel fallback. Il
/// controllo su `api/` sotto è comunque difesa in profondità, non l'unico
/// argine: se un giorno l'ordine delle rotte cambiasse, un client API non
/// deve comunque ricevere HTML al posto di un 404 JSON.
async fn serve(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');

    if path.starts_with("api/") {
        return Problem::not_found().into_response();
    }

    if let Some(file) = Assets::get(path) {
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        // I nomi dei bundle sotto `assets/` contengono l'hash del contenuto:
        // sono immutabili, e possono essere cacheati per sempre.
        let cache = if path.starts_with("assets/") {
            "public, max-age=31536000, immutable"
        } else {
            "no-cache"
        };

        return Response::builder()
            .status(StatusCode::OK)
            .header(
                header::CONTENT_TYPE,
                HeaderValue::from_str(mime.as_ref())
                    .unwrap_or(HeaderValue::from_static("application/octet-stream")),
            )
            .header(header::CACHE_CONTROL, HeaderValue::from_static(cache))
            .body(Body::from(file.data.into_owned()))
            .unwrap_or_else(|_| Problem::internal().into_response());
    }

    match Assets::get("index.html") {
        Some(index) => Response::builder()
            .status(StatusCode::OK)
            .header(
                header::CONTENT_TYPE,
                HeaderValue::from_static("text/html; charset=utf-8"),
            )
            .header(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"))
            .body(Body::from(index.data.into_owned()))
            .unwrap_or_else(|_| Problem::internal().into_response()),
        None => Problem::not_found().into_response(),
    }
}

/// Aggiunge il fallback SPA a un router con stato e vi applica gli strati
/// comuni (header di sicurezza, compressione, tracing).
///
/// L'ordine conta: il fallback va impostato **prima** di
/// `keeppix_api::with_common_layers`, non dopo. In axum 0.8 `.layer()`
/// avvolge soltanto il fallback già presente al momento in cui viene
/// chiamato — un fallback aggiunto in seguito uscirebbe senza CSP, che qui
/// vorrebbe dire servire `index.html`, il documento che carica l'intera
/// applicazione, senza `Content-Security-Policy` in produzione. Vedi il
/// commento su `with_common_layers` in `keeppix-api` per i dettagli. Non
/// riordinare.
pub fn mount(router: axum::Router<AppState>) -> axum::Router<AppState> {
    keeppix_api::with_common_layers(router.fallback(get(serve)))
}

/// Router senza stato, con lo stesso fallback SPA e gli stessi strati comuni
/// di `mount`, per i test che non toccano il database.
pub fn mount_stateless() -> axum::Router {
    keeppix_api::with_common_layers(axum::Router::new().fallback(get(serve)))
}
