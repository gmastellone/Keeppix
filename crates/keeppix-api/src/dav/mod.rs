//! `WebDAV` — dispatcher e autenticazione. Task 5 (scaffolding): monta il
//! router, autentica con app-password via Basic Auth, restituisce `401`
//! senza redirect per credenziali assenti o sbagliate e `501` per metodi
//! non ancora implementati.
//!
//! Mai cookie di sessione qui: i client `WebDAV` (Finder, rclone, …) parlano
//! solo Basic Auth, e le uniche credenziali accettate sono le app-password
//! (`AppPasswordRepo::verify`) — mai la password di login.

use axum::body::Body;
use axum::extract::State;
use axum::http::{Request, StatusCode};
use axum::response::{IntoResponse, Response};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use keeppix_db::AppPasswordRepo;

use crate::problem::Problem;
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

/// Handler `WebDAV` principale. Autentica prima, poi dispatcha per metodo.
/// In questa fase (Task 5) tutti i metodi restituiscono `501` dopo
/// l'autenticazione: i task 6-8 aggiungeranno il dispatch reale
/// (PROPFIND/GET/PUT/…) usando l'`AuthContext` costruito dalla `user_id`
/// verificata qui.
pub async fn handler(State(state): State<AppState>, req: Request<Body>) -> Response {
    let Some((username, secret)) = parse_basic_auth(&req) else {
        return unauthorized();
    };

    match AppPasswordRepo::new(&state.db)
        .verify(&username, &secret)
        .await
    {
        Err(_) => Problem::internal().into_response(),
        Ok(None) => unauthorized(),
        Ok(Some(_user_id)) => not_implemented(),
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
