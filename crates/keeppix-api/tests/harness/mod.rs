//! Server reale su porta effimera con Postgres reale.
//! I test parlano HTTP come un browser, cookie inclusi: è l'unico modo di
//! verificare davvero il comportamento dei cookie di sessione.

use keeppix_db::Db;
use sqlx::{Connection as _, PgConnection};
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::{ContainerAsync, ImageExt};

pub struct TestServer {
    // `None` quando i test girano contro un server Postgres già esistente.
    _container: Option<ContainerAsync<Postgres>>,
    pub base_url: String,
    pub client: reqwest::Client,
}

impl TestServer {
    /// # Panics
    /// Se il database non è raggiungibile o il server non si avvia.
    #[allow(clippy::expect_used)]
    pub async fn start() -> Self {
        let (container, url) = provision().await;

        let db = Db::connect(&url, 5).await.expect("connessione");
        db.migrate().await.expect("migrazioni");

        let state = keeppix_api::AppState::new(db, 3600);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let addr = listener.local_addr().expect("indirizzo");

        tokio::spawn(async move {
            axum::serve(listener, keeppix_api::router(state)).await.ok();
        });

        let client = reqwest::Client::builder()
            .cookie_store(true)
            .build()
            .expect("client http");

        Self {
            _container: container,
            base_url: format!("http://{addr}"),
            client,
        }
    }

    pub fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base_url)
    }
}

/// Asserzioni condivise sui quattro header di sicurezza, per i test che
/// passano da `TestServer` (`keeppix_api::router(state)`, il router *con*
/// stato). Copia di `assert_security_headers` in `tests/health.rs` e
/// `tests/openapi.rs` — quei due file non toccano `TestServer` e non possono
/// vedere questo modulo allo stesso modo, quindi restano copie separate (i
/// crate/binari di test non condividono codice fra loro senza un modulo
/// comune) — ma qui vive in un unico posto per tutti i file che già
/// dichiarano `mod harness;` (`auth.rs`, `openapi.rs`), evitandone una quarta
/// copia testuale.
#[allow(clippy::unwrap_used)]
pub fn assert_security_headers(headers: &reqwest::header::HeaderMap) {
    assert_eq!(headers.get("x-content-type-options").unwrap(), "nosniff");
    assert_eq!(headers.get("referrer-policy").unwrap(), "no-referrer");
    assert!(headers.get("content-security-policy").is_some());
    assert_eq!(
        headers.get("permissions-policy").unwrap(),
        "camera=(), microphone=(), geolocation=()"
    );
}

/// Procura un database vergine e restituisce l'eventuale container che lo
/// ospita, da tenere vivo per la durata del test.
///
/// Percorso predefinito: un container usa-e-getta `postgis/postgis:17-3.5`.
/// Se `KEEPPIX_TEST_DATABASE_URL` è impostata si usa invece il server già in
/// ascolto a quell'indirizzo, creando un database vergine per ogni test —
/// necessario dove il registry delle immagini non è raggiungibile. I database
/// così creati non vengono eliminati: quell'indirizzo deve puntare a
/// un'istanza di scarto.
///
/// Copia di `crates/keeppix-db/tests/harness/mod.rs`, dove la documentazione
/// è per esteso e dove `with_database` ha i suoi unit test. I due crate non
/// condividono codice di test: le due copie vanno tenute allineate a mano.
#[allow(clippy::expect_used)]
async fn provision() -> (Option<ContainerAsync<Postgres>>, String) {
    let Ok(server_url) = std::env::var("KEEPPIX_TEST_DATABASE_URL") else {
        let container = Postgres::default()
            .with_tag("17-3.5")
            .with_name("postgis/postgis")
            .start()
            .await
            .expect("container Postgres");
        let port = container.get_host_port_ipv4(5432).await.expect("porta");
        let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");

        return (Some(container), url);
    };

    // Il nome deriva da un UUID: solo esadecimali e underscore, nessun input
    // esterno in un identificatore che non sarebbe parametrizzabile.
    let name = format!("keeppix_test_{}", uuid::Uuid::now_v7().simple());

    let mut admin = PgConnection::connect(&server_url)
        .await
        .expect("connessione al server Postgres esistente");
    sqlx::query(&format!("CREATE DATABASE \"{name}\""))
        .execute(&mut admin)
        .await
        .expect("creazione del database di test");
    admin.close().await.ok();

    (None, with_database(&server_url, &name))
}

/// Sostituisce il nome del database in un URL di connessione, conservando
/// credenziali, host, porta e parametri di query.
fn with_database(server_url: &str, name: &str) -> String {
    let (base, query) = match server_url.split_once('?') {
        Some((base, query)) => (base, Some(query)),
        None => (server_url, None),
    };

    let authority_start = base.find("://").map_or(0, |i| i + 3);
    let without_db = base[authority_start..]
        .find('/')
        .map_or(base, |i| &base[..authority_start + i]);

    match query {
        Some(query) => format!("{without_db}/{name}?{query}"),
        None => format!("{without_db}/{name}"),
    }
}
