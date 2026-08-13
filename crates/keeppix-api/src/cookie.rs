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

/// Stessa forma di `session_cookie`: un `__Host-` cancellante che omettesse
/// `Secure` o `SameSite` verrebbe scartato per intero da un browser conforme
/// (RFC 6265bis §4.1.3.2), lasciando il cookie di sessione scaduto — ma
/// ancora inviato dal client — sopravvivere al logout in produzione.
#[must_use]
pub fn clearing_cookie(secure: bool) -> Cookie<'static> {
    let mut cookie = Cookie::new(SESSION_COOKIE, "");
    cookie.set_http_only(true);
    cookie.set_secure(secure);
    cookie.set_same_site(SameSite::Lax);
    cookie.set_path("/");
    cookie.set_max_age(Some(time::Duration::ZERO));
    cookie
}

/// Vero quando la richiesta non sembra arrivare da localhost: in produzione
/// si sta dietro HTTPS, quindi il cookie deve essere `Secure`. `None` (header
/// `Host` assente) mantiene il comportamento sicuro (`true`).
///
/// Confronta l'host **esattamente**, dopo aver tolto un'eventuale porta: un
/// semplice `starts_with` verrebbe ingannato da un header `Host` scelto dal
/// client come `127.0.0.1.evil.com` o `localhost.evil.com`, che inizia con
/// una stringa dell'insieme locale ma non lo è.
#[must_use]
pub fn should_be_secure(host: Option<&str>) -> bool {
    let Some(host) = host else { return true };
    !matches!(
        strip_port(host),
        "localhost" | "127.0.0.1" | "[::1]" | "::1"
    )
}

/// Rimuove l'eventuale suffisso `:porta` da un header `Host`. Un letterale
/// IPv6 può comparire fra parentesi quadre con porta (`[::1]:8080`) o, senza
/// porta, privo di parentesi (`::1`) — in quel caso i due punti fanno parte
/// dell'indirizzo, non separano una porta, quindi si toglie la porta solo
/// quando compare **un solo** `:` nella stringa.
fn strip_port(host: &str) -> &str {
    if host.starts_with('[') {
        return match host.find(']') {
            Some(end) => &host[..=end],
            None => host,
        };
    }
    if host.matches(':').count() == 1 {
        if let Some((h, _port)) = host.split_once(':') {
            return h;
        }
    }
    host
}

#[cfg(test)]
mod tests {
    use super::should_be_secure;

    #[test]
    fn localhost_variants_are_not_secure() {
        assert!(!should_be_secure(Some("127.0.0.1:8080")));
        assert!(!should_be_secure(Some("localhost:3000")));
        assert!(!should_be_secure(Some("localhost")));
        assert!(!should_be_secure(Some("127.0.0.1")));
        assert!(!should_be_secure(Some("[::1]:8080")));
        assert!(!should_be_secure(Some("::1")));
    }

    #[test]
    fn real_hosts_and_missing_host_are_secure() {
        assert!(should_be_secure(Some("photos.example.com")));
        assert!(should_be_secure(Some("photos.example.com:443")));
        assert!(should_be_secure(None));
    }

    /// Un header `Host` scelto dal client che inizia come un membro
    /// dell'insieme locale ma non lo è deve restare `Secure`.
    #[test]
    fn lookalike_hosts_are_not_treated_as_local() {
        assert!(should_be_secure(Some("127.0.0.1.evil.com")));
        assert!(should_be_secure(Some("localhost.evil.com")));
        assert!(should_be_secure(Some("127.0.0.1.evil.com:8080")));
    }
}
