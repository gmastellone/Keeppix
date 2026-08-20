//! Server reale su porta effimera con Postgres reale.
//! I test parlano HTTP come un browser, cookie inclusi: è l'unico modo di
//! verificare davvero il comportamento dei cookie di sessione.

// Il modulo è incluso da più binari di test (`auth.rs`, `openapi.rs`) e ognuno
// ne usa una parte: ciò che serve a uno è codice morto nell'altro. Senza questo
// `allow`, `stop_database()` — usata solo da `auth.rs` — farebbe fallire la
// compilazione di `openapi.rs` con `-D warnings`.
#![allow(dead_code, unused_imports)]

use std::time::Duration;

use keeppix_db::Db;
use sqlx::{Connection as _, PgConnection};
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::{ContainerAsync, ImageExt};
use tokio::sync::OnceCell;

static SHARED: OnceCell<(ContainerAsync<Postgres>, String)> = OnceCell::const_new();

pub struct TestServer {
    // `Some` solo sul percorso stoppable (il test 503). Il container
    // condiviso vive nello `OnceCell` e non si ferma.
    container: Option<ContainerAsync<Postgres>>,
    pub db: Db,
    pub database_url: String,
    pub data_dir: std::path::PathBuf,
    /// Radice allowlist per `KEEPPIX_LIBRARY_ROOTS` nei test (sotto `data_dir`).
    pub photos_root: std::path::PathBuf,
    pub auth_pings: std::sync::Arc<std::sync::atomic::AtomicU64>,
    pub base_url: String,
    pub client: reqwest::Client,
}

impl TestServer {
    /// # Panics
    /// Se il database non è raggiungibile o il server non si avvia.
    #[allow(clippy::expect_used)]
    pub async fn start() -> Self {
        boot(None, provision().await, |state| state).await
    }

    /// Come [`Self::start`], con uno stato già modificato (demosaic finto,
    /// tetto cache, TTL). I test di `/media/full` sui RAW lo usano per non
    /// dipendere da `dcraw_emu` in CI.
    #[allow(clippy::expect_used)]
    pub async fn start_with(
        configure: impl FnOnce(keeppix_api::AppState) -> keeppix_api::AppState,
    ) -> Self {
        boot(None, provision().await, configure).await
    }

    /// Container dedicato, così `stop_database` non spegne il Postgres degli
    /// altri test dello stesso binario.
    #[allow(clippy::expect_used)]
    pub async fn start_stoppable() -> Self {
        let (container, url) = provision_dedicated().await;
        boot(container, url, |state| state).await
    }

    pub fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base_url)
    }

    /// Spegne il Postgres sotto il server, per osservare come si comporta la
    /// superficie HTTP quando il database non risponde. È l'unico modo di
    /// provare davvero quella proprietà: nessun mock sta fra gli handler e il
    /// pool.
    ///
    /// Restituisce `false` sul percorso `KEEPPIX_TEST_DATABASE_URL` (vedi R9),
    /// dove il server Postgres è condiviso con gli altri test e fermarlo li
    /// romperebbe: chi chiama salta il test.
    ///
    /// # Panics
    /// Se il container esiste ma non si riesce a fermarlo.
    #[allow(clippy::expect_used)]
    pub async fn stop_database(&self) -> bool {
        match self.container.as_ref() {
            Some(container) => {
                container.stop().await.expect("stop del container Postgres");
                true
            }
            None => false,
        }
    }
}

#[allow(clippy::expect_used)]
async fn boot(
    container: Option<ContainerAsync<Postgres>>,
    url: String,
    configure: impl FnOnce(keeppix_api::AppState) -> keeppix_api::AppState,
) -> TestServer {
    let db = Db::connect(&url, 5).await.expect("connessione");
    db.migrate().await.expect("migrazioni");

    let data_dir = std::env::temp_dir().join(format!("keeppix-api-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&data_dir).expect("data_dir");
    let photos_root = data_dir.join("photos");
    std::fs::create_dir_all(&photos_root).expect("photos_root");
    let auth_pings = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
    let ping = auth_pings.clone();
    let watchers =
        keeppix_jobs::watch::LibraryWatchers::new(db.clone(), std::time::Duration::from_millis(80));
    let state = configure(
        keeppix_api::AppState::new(db.clone(), 3600, data_dir.clone())
            .with_database_url(url.clone())
            .with_library_roots(vec![photos_root.clone()])
            .with_library_watchers(watchers)
            .with_on_authenticated(std::sync::Arc::new(move || {
                ping.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            })),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("indirizzo");

    tokio::spawn(async move {
        axum::serve(listener, keeppix_api::router(state)).await.ok();
    });

    let client = reqwest::Client::builder()
        .cookie_store(true)
        .default_headers(client_headers())
        .build()
        .expect("client http");

    TestServer {
        container,
        db,
        database_url: url,
        data_dir,
        photos_root,
        auth_pings,
        base_url: format!("http://{addr}"),
        client,
    }
}

/// Header custom richiesto sulle mutazioni dal layer CSRF
/// (`keeppix_api::csrf`), esattamente come lo invia `apiFetch` del frontend su
/// tutte le chiamate. Lo portano di default sia il client dell'harness sia
/// `plain_client()`, così i test parlano come il client reale; i due test che
/// verificano il layer costruiscono invece un client senza header a mano.
#[allow(clippy::expect_used)]
pub fn client_headers() -> reqwest::header::HeaderMap {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        keeppix_api::csrf::CLIENT_HEADER,
        reqwest::header::HeaderValue::from_static("test"),
    );
    headers
}

/// Client senza cookie store — per i test che ripresentano a mano un cookie
/// specifico — ma con l'header custom, cioè un client legittimo che
/// semplicemente non ricorda le sessioni.
#[allow(clippy::expect_used)]
pub fn plain_client() -> reqwest::Client {
    reqwest::Client::builder()
        .default_headers(client_headers())
        .build()
        .expect("client http")
}

/// Le asserzioni sugli header di sicurezza vivono in `keeppix-test-support`,
/// una copia sola per tutto il workspace: `keeppix-api` e `keeppix-server` non
/// possono condividere codice di test in altro modo. Ri-esportata qui perché i
/// file che dichiarano `mod harness;` continuino a importarla da un solo posto.
pub use keeppix_test_support::assert_security_headers;

/// Procura un database vergine. Un container per processo, un `CREATE
/// DATABASE` per test — allineato a `crates/keeppix-db/tests/harness/mod.rs`.
#[allow(clippy::expect_used)]
async fn provision() -> String {
    if let Ok(server_url) = std::env::var("KEEPPIX_TEST_DATABASE_URL") {
        return named_database(&server_url).await;
    }
    let (_container, admin_url) = SHARED
        .get_or_init(|| async {
            let container = Postgres::default()
                .with_tag("17-3.5")
                .with_name("postgis/postgis")
                .start()
                .await
                .expect("container Postgres");
            let port = mapped_port(&container).await;
            let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
            (container, url)
        })
        .await;
    named_database(admin_url).await
}

#[allow(clippy::expect_used)]
async fn provision_dedicated() -> (Option<ContainerAsync<Postgres>>, String) {
    if std::env::var("KEEPPIX_TEST_DATABASE_URL").is_ok() {
        return (None, provision().await);
    }
    let container = Postgres::default()
        .with_tag("17-3.5")
        .with_name("postgis/postgis")
        .start()
        .await
        .expect("container Postgres");
    let port = mapped_port(&container).await;
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    (Some(container), url)
}

/// Docker Desktop a volte espone la porta un attimo dopo `start()`. Senza
/// retry, `PortNotExposed` fa fallire 1-3 test a caso in locale.
#[allow(clippy::expect_used)]
async fn mapped_port(container: &ContainerAsync<Postgres>) -> u16 {
    let mut delay = Duration::from_millis(50);
    for attempt in 1..=12 {
        match container.get_host_port_ipv4(5432).await {
            Ok(port) => return port,
            Err(_) if attempt < 12 => {
                tokio::time::sleep(delay).await;
                delay = (delay * 2).min(Duration::from_secs(2));
            }
            Err(err) => panic!("porta mappata dopo {attempt} tentativi: {err}"),
        }
    }
    unreachable!("il ciclo sopra termina sempre")
}

#[allow(clippy::expect_used)]
async fn named_database(server_url: &str) -> String {
    let name = format!("keeppix_test_{}", uuid::Uuid::now_v7().simple());
    let mut admin = PgConnection::connect(server_url)
        .await
        .expect("connessione al server Postgres esistente");
    sqlx::query(&format!("CREATE DATABASE \"{name}\""))
        .execute(&mut admin)
        .await
        .expect("creazione del database di test");
    admin.close().await.ok();
    with_database(server_url, &name)
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
