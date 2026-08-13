use std::time::Duration;

use axum_extra::extract::cookie::{Cookie, SameSite};
use keeppix_domain::SessionToken;

use crate::extract::SESSION_COOKIE;

/// Cookie di sessione. `__Host-` impone `Secure` e `Path=/`, e vieta `Domain`:
/// il cookie non può essere piazzato da un sottodominio compromesso.
///
/// `secure` è parametrico perché in test si parla in chiaro su 127.0.0.1, dove
/// un client conforme scarterebbe un cookie `Secure`.
#[must_use]
pub fn session_cookie(token: &SessionToken, ttl: Duration, secure: bool) -> Cookie<'static> {
    let mut cookie = Cookie::new(SESSION_COOKIE, token.as_str().to_owned());
    cookie.set_http_only(true);
    cookie.set_secure(secure);
    cookie.set_same_site(SameSite::Lax);
    cookie.set_path("/");
    cookie.set_max_age(Some(
        time::Duration::try_from(ttl).unwrap_or(time::Duration::days(30)),
    ));
    cookie
}

#[must_use]
pub fn clearing_cookie() -> Cookie<'static> {
    let mut cookie = Cookie::new(SESSION_COOKIE, "");
    cookie.set_http_only(true);
    cookie.set_path("/");
    cookie.set_max_age(Some(time::Duration::ZERO));
    cookie
}

/// Vero quando la richiesta non arriva da localhost: in produzione si sta
/// dietro HTTPS, quindi il cookie deve essere `Secure`.
#[must_use]
pub fn should_be_secure(host: Option<&str>) -> bool {
    !matches!(host, Some(h) if h.starts_with("127.0.0.1") || h.starts_with("localhost"))
}
