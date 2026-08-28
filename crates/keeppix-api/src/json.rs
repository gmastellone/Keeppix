//! JSON extractor and response that stay inside the RFC 9457 contract.
//!
//! `axum::Json` rejects malformed requests on its own, but with a
//! `text/plain` response and no stable `type`: a missing `Content-Type`
//! would give a plain-text `415`, a broken body a plain-text `400`. The
//! frozen contract says *every* error is `application/problem+json` with a
//! `type` the client can branch on, so routes use this wrapper instead of
//! `axum::Json`.
//!
//! Wraps both directions — extraction and response — so a route imports a
//! single type and can't accidentally extract with `axum::Json` and
//! respond with this one, or vice versa.

use axum::extract::{FromRequest, Request};
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::problem::Problem;

/// Like `axum::Json`, but the rejection is a `Problem`.
pub struct Json<T>(pub T);

impl<T, S> FromRequest<S> for Json<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = Problem;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        // The rejection translation lives in `From<JsonRejection> for Problem`
        // (`problem.rs`): that's where all the stable `type`s live.
        let axum::Json(value) = axum::Json::<T>::from_request(req, state).await?;
        Ok(Self(value))
    }
}

impl<T: Serialize> IntoResponse for Json<T> {
    fn into_response(self) -> Response {
        axum::Json(self.0).into_response()
    }
}
