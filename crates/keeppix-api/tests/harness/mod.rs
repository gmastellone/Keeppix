//! Server reale su porta effimera con Postgres reale in container.
//! I test parlano HTTP come un browser, cookie inclusi: è l'unico modo di
//! verificare davvero il comportamento dei cookie di sessione.

use keeppix_db::Db;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::{ContainerAsync, ImageExt};

pub struct TestServer {
    _container: ContainerAsync<Postgres>,
    pub base_url: String,
    pub client: reqwest::Client,
}

impl TestServer {
    /// # Panics
    /// Se Docker non è disponibile o il server non si avvia.
    #[allow(clippy::expect_used)]
    pub async fn start() -> Self {
        let container = Postgres::default()
            .with_tag("17-3.5")
            .with_name("postgis/postgis")
            .start()
            .await
            .expect("container Postgres");
        let port = container.get_host_port_ipv4(5432).await.expect("porta");
        let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");

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
