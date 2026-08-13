use std::time::Duration;

use axum_extra::extract::cookie::{Cookie, SameSite};
use keeppix_domain::SessionToken;

use crate::extract::SESSION_COOKIE;

/// Cookie di sessione con prefisso `__Host-`.
///
/// Il prefisso `__Host-` (RFC 6265bis §4.1.3.2) impone `Secure`, `Path=/` e
/// vieta `Domain`: un client conforme che riceve un `Set-Cookie` con quel
/// prefisso ma privo anche di uno solo di questi requisiti scarta il cookie
/// **per intero**, non solo l'attributo mancante. Questo vale sempre,
/// indipendentemente dal trasporto della richiesta: `Secure` va impostato
/// anche quando si parla in chiaro su `127.0.0.1` in sviluppo, perché è la
/// *presenza letterale* dell'attributo nell'header a essere richiesta, non
/// l'uso effettivo di TLS.
///
/// Da non confondere con una regola diversa e più permissiva che i browser
/// applicano *a valle*: un'origine "potenzialmente affidabile" come
/// `127.0.0.1`/`localhost`/`::1` è esentata dal requisito che `Secure`
/// **funzioni** solo su una connessione cifrata, quindi un browser (e,
/// verificato empiricamente, anche `cookie_store`/`reqwest` nella versione
/// usata da questo workspace) accetta e rinvia un cookie `Secure` ricevuto in
/// chiaro su loopback. Ma questo non rende mai opzionale *impostare*
/// l'attributo lato server: se manca, il prefisso `__Host-` fa scartare il
/// cookie a prescindere da dove sia diretta la richiesta. Un'implementazione
/// precedente ometteva `Secure` su host che sembravano loopback pensando che
/// servisse a renderlo osservabile nei test: era doppiamente sbagliato, sia
/// perché rompeva ogni sessione aperta in sviluppo locale via browser reale,
/// sia perché la libreria di test già gestiva correttamente il caso.
#[must_use]
pub fn session_cookie(token: &SessionToken, ttl: Duration) -> Cookie<'static> {
    let mut cookie = Cookie::new(SESSION_COOKIE, token.as_str().to_owned());
    cookie.set_http_only(true);
    cookie.set_secure(true);
    cookie.set_same_site(SameSite::Lax);
    cookie.set_path("/");
    cookie.set_max_age(Some(
        time::Duration::try_from(ttl).unwrap_or(time::Duration::days(30)),
    ));
    cookie
}

/// Stessa forma di `session_cookie`: un `__Host-` cancellante che omettesse
/// `Secure` o `SameSite` verrebbe scartato per intero da un browser conforme
/// (RFC 6265bis §4.1.3.2), lasciando il cookie di sessione scaduto — ma
/// ancora inviato dal client — sopravvivere al logout in produzione.
#[must_use]
pub fn clearing_cookie() -> Cookie<'static> {
    let mut cookie = Cookie::new(SESSION_COOKIE, "");
    cookie.set_http_only(true);
    cookie.set_secure(true);
    cookie.set_same_site(SameSite::Lax);
    cookie.set_path("/");
    cookie.set_max_age(Some(time::Duration::ZERO));
    cookie
}
