//! Postgres reale in container per i test di integrazione.
//! Ogni `TestDb` è isolato: container proprio, database vuoto, migrazioni applicate.

use keeppix_db::Db;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::{ContainerAsync, ImageExt};

pub struct TestDb {
    // Tenuto vivo: alla deallocazione il container viene fermato.
    _container: ContainerAsync<Postgres>,
    db: Db,
}

impl TestDb {
    /// # Panics
    /// Se Docker non è disponibile o le migrazioni falliscono: in un test è
    /// il comportamento voluto.
    #[allow(clippy::expect_used)]
    pub async fn start() -> Self {
        let container = Postgres::default()
            .with_tag("17-3.5")
            .with_name("postgis/postgis")
            .start()
            .await
            .expect("avvio del container Postgres");

        let port = container
            .get_host_port_ipv4(5432)
            .await
            .expect("porta mappata");
        let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");

        let db = Db::connect(&url, 5).await.expect("connessione");
        db.migrate().await.expect("migrazioni");

        Self {
            _container: container,
            db,
        }
    }

    #[must_use]
    pub const fn db(&self) -> &Db {
        &self.db
    }
}
