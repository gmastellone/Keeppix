//! Asserzioni di test condivise fra i crate del workspace.
//!
//! Esiste per una ragione meccanica: due binari di test in **crate diversi**
//! non possono condividere codice se non attraverso un crate. Le asserzioni
//! sugli header di sicurezza servivano a `keeppix-api` (rotte, 404, 405,
//! documento `OpenAPI`) e a `keeppix-server` (fallback SPA), e ne esistevano
//! **tre copie** testuali: tre posti da aggiornare quando la policy cambia,
//! con la certezza statistica che uno resti indietro.
//!
//! Il tipo degli header è `http::HeaderMap`, che è lo stesso tipo che
//! `axum::http` e `reqwest::header` ri-esportano: la stessa funzione serve sia
//! i test che parlano HTTP con `reqwest` sia quelli che chiamano un `Router`
//! con `oneshot`.

// Il codice di test asserisce fallendo: `unwrap`/`expect` qui sono lo strumento,
// non una dimenticanza.
#![allow(clippy::expect_used)]

use http::HeaderMap;

/// Policy attesa in `Content-Security-Policy`, direttiva per direttiva. Il
/// confronto è per direttiva esatta, non `contains` sull'intero header:
/// `default-src 'self' *` contiene `default-src 'self'` e non deve passare.
const REQUIRED_CSP_DIRECTIVES: [&str; 5] = [
    // Fondamento: tutto ciò che non è coperto da una direttiva specifica viene
    // dall'origine stessa.
    "default-src 'self'",
    // La metà che conta: senza script inline, `'self'` è più forte di un nonce.
    "script-src 'self'",
    // Anti-clickjacking. Sostituisce `X-Frame-Options`.
    "frame-ancestors 'none'",
    // Impedisce a un'iniezione di riscrivere la base dei percorsi relativi.
    "base-uri 'none'",
    // Un form iniettato non può inviare le credenziali a un altro host.
    "form-action 'self'",
];

/// Header di sicurezza attesi su **ogni** risposta, qualunque rotta la produca:
/// una rotta esistente, il fallback 404, il fallback `405`, il fallback SPA o il
/// documento `OpenAPI`. Sono applicati da `keeppix_api::with_common_layers`, e
/// la trappola che questa asserzione difende è l'ordine di `.fallback(...)`
/// rispetto ai `.layer(...)` (vedi il commento su quella funzione): sbagliarlo
/// fa uscire proprio `index.html` senza CSP.
///
/// # Panics
/// Se un header manca o ha un valore diverso da quello atteso.
pub fn assert_security_headers(headers: &HeaderMap) {
    assert_eq!(
        headers
            .get("x-content-type-options")
            .expect("x-content-type-options"),
        "nosniff"
    );
    assert_eq!(
        headers.get("referrer-policy").expect("referrer-policy"),
        "no-referrer"
    );
    assert_eq!(
        headers
            .get("permissions-policy")
            .expect("permissions-policy"),
        "camera=(), microphone=(), geolocation=()"
    );
    assert_content_security_policy(headers);
}

/// Verifica la **sostanza** della CSP, non la sua presenza. Le tre copie che
/// questa funzione sostituisce facevano `assert!(csp.is_some())`: sostituire
/// l'intera policy con `default-src *` lasciava la suite verde, cioè
/// l'asserzione non asseriva niente. Qui una policy indebolita fa fallire i
/// test.
///
/// # Panics
/// Se l'header manca, se una direttiva richiesta non c'è esattamente come
/// scritta, o se ricompare una deroga `unsafe-*`.
pub fn assert_content_security_policy(headers: &HeaderMap) {
    let csp = headers
        .get("content-security-policy")
        .expect("content-security-policy")
        .to_str()
        .expect("la CSP è ASCII");

    let directives: Vec<&str> = csp.split(';').map(str::trim).collect();
    for required in REQUIRED_CSP_DIRECTIVES {
        assert!(
            directives.contains(&required),
            "la CSP non contiene la direttiva `{required}`: `{csp}`"
        );
    }

    // `script-src` senza `'unsafe-inline'` è la proprietà che rende la policy
    // efficace: con essa, un'iniezione di `<script>` non esegue.
    let script_src = directives
        .iter()
        .find(|d| d.starts_with("script-src"))
        .expect("script-src dichiarata");
    assert!(
        !script_src.contains("unsafe-inline") && !script_src.contains("unsafe-eval"),
        "script-src non deve ammettere deroghe unsafe: `{script_src}`"
    );
}
