//! Extractor e risposta JSON che restano dentro il contratto RFC 9457.
//!
//! `axum::Json` rifiuta da sé le richieste malformate, ma con una risposta
//! `text/plain` e senza `type` stabile: un `Content-Type` mancante darebbe un
//! `415` di testo, un corpo rotto un `400` di testo. Il contratto congelato
//! (spec §9.2) dice che *ogni* errore è `application/problem+json` con un
//! `type` su cui il client ramifica, quindi le rotte usano questo wrapper al
//! posto di `axum::Json`.
//!
//! Avvolge entrambe le direzioni — estrazione e risposta — così che una rotta
//! importi un solo tipo e non possa per distrazione estrarre con
//! `axum::Json` e rispondere con questo, o viceversa.

use axum::extract::{FromRequest, Request};
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::problem::Problem;

/// Come `axum::Json`, ma la rejection è un `Problem`.
pub struct Json<T>(pub T);

impl<T, S> FromRequest<S> for Json<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = Problem;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        // La traduzione della rejection sta in `From<JsonRejection> for Problem`
        // (`problem.rs`): è lì che vivono tutti i `type` stabili.
        let axum::Json(value) = axum::Json::<T>::from_request(req, state).await?;
        Ok(Self(value))
    }
}

impl<T: Serialize> IntoResponse for Json<T> {
    fn into_response(self) -> Response {
        axum::Json(self.0).into_response()
    }
}
