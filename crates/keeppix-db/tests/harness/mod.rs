//! Postgres reale per i test di integrazione.
//! Ogni `TestDb` è isolato: database vergine, migrazioni applicate.
//! Il container Postgres è uno per processo: il costo del boot si paga una
//! volta, l'isolamento resta un `CREATE DATABASE` per test.

#![allow(dead_code, clippy::expect_used)]
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use keeppix_db::Db;
use sqlx::ConnectOptions;
use sqlx::postgres::PgConnectOptions;
use sqlx::{Connection as _, PgConnection};
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::{ContainerAsync, ImageExt};
use tokio::sync::OnceCell;
use tracing::Subscriber;
use tracing_subscriber::Layer;
use tracing_subscriber::prelude::*;

pub struct TestDb {
    db: Db,
    /// Admin URL of the shared Postgres (used to DROP this test DB on Drop).
    admin_url: String,
    /// Name created by [`named_database`] — dropped with `WITH (FORCE)`.
    db_name: String,
}

static SHARED: OnceCell<(ContainerAsync<Postgres>, String)> = OnceCell::const_new();

impl TestDb {
    /// # Panics
    /// Se il database non è raggiungibile o le migrazioni falliscono: in un
    /// test è il comportamento voluto.
    #[allow(clippy::expect_used)]
    pub async fn start() -> Self {
        let provisioned = provision().await;

        let db = Db::connect(&provisioned.url, 5).await.expect("connessione");
        db.migrate().await.expect("migrazioni");

        Self {
            db,
            admin_url: provisioned.admin_url,
            db_name: provisioned.name,
        }
    }

    #[must_use]
    pub const fn db(&self) -> &Db {
        &self.db
    }

    /// Database con `log_statements` attivo e un layer che cattura le query
    /// emesse da sqlx — per i test che devono asserire l'assenza di aggregati.
    ///
    /// # Panics
    /// Come [`Self::start`].
    #[allow(clippy::expect_used)]
    pub async fn start_traced() -> (
        Self,
        Arc<Mutex<Vec<String>>>,
        tracing::subscriber::DefaultGuard,
    ) {
        let provisioned = provision().await;
        let captured = Arc::new(Mutex::new(Vec::new()));
        let layer = SqlCapture(captured.clone());
        let subscriber = tracing_subscriber::registry()
            .with(tracing_subscriber::EnvFilter::new("sqlx=debug"))
            .with(layer);
        let guard = tracing::subscriber::set_default(subscriber);

        let options = PgConnectOptions::from_str(&provisioned.url)
            .expect("url")
            .log_statements(tracing::log::LevelFilter::Debug);
        let db = Db::connect_with(options, 5)
            .await
            .expect("connessione tracciata");
        db.migrate().await.expect("migrazioni");

        (
            Self {
                db,
                admin_url: provisioned.admin_url,
                db_name: provisioned.name,
            },
            captured,
            guard,
        )
    }
}

struct SqlCapture(Arc<Mutex<Vec<String>>>);

impl<S> Layer<S> for SqlCapture
where
    S: Subscriber,
{
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        if !event.metadata().target().starts_with("sqlx::query") {
            return;
        }
        let mut visitor = SqlFieldVisitor(String::new());
        event.record(&mut visitor);
        if !visitor.0.is_empty() {
            self.0.lock().expect("lock").push(visitor.0);
        }
    }
}

struct SqlFieldVisitor(String);

impl tracing::field::Visit for SqlFieldVisitor {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "db.statement" || field.name() == "summary" {
            if !self.0.is_empty() {
                self.0.push(' ');
            }
            self.0.push_str(value);
        }
    }

    fn record_debug(&mut self, _field: &tracing::field::Field, _value: &dyn std::fmt::Debug) {}
}

impl Drop for TestDb {
    fn drop(&mut self) {
        drop_test_database(&self.admin_url, &self.db_name);
    }
}

/// Crea un amministratore e ne restituisce l'id. Ogni test di questa fase
/// ha bisogno di un proprietario per le librerie.
///
/// # Panics
/// Se la creazione fallisce: in un test è il comportamento voluto.
#[allow(clippy::expect_used, dead_code)]
pub async fn seed_admin(test: &TestDb) -> keeppix_domain::UserId {
    use keeppix_domain::{NewUser, Password, SystemRole, Username, hash_password};

    let password = Password::parse("correct horse battery staple").expect("password valida");
    keeppix_db::UserRepo::new(test.db())
        .create_bootstrap_admin(NewUser {
            username: Username::parse("giovanni").expect("username valido"),
            email: None,
            display_name: "Giovanni".to_owned(),
            password_hash: hash_password(&password).expect("hash").as_str().to_owned(),
            role: SystemRole::Admin,
        })
        .await
        .expect("creazione admin")
        .id
}

/// Crea un utente non-admin. Serve a ogni test che verifichi i permessi.
///
/// # Panics
/// Se la creazione fallisce.
#[allow(clippy::expect_used, dead_code)]
pub async fn seed_user(
    test: &TestDb,
    admin: keeppix_domain::UserId,
    username: &str,
) -> keeppix_domain::UserId {
    use keeppix_domain::{AuthContext, NewUser, Password, SystemRole, Username, hash_password};

    let password = Password::parse("correct horse battery staple").expect("password valida");
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    keeppix_db::UserRepo::new(test.db())
        .create(
            &ctx,
            NewUser {
                username: Username::parse(username).expect("username valido"),
                email: None,
                display_name: username.to_owned(),
                password_hash: hash_password(&password).expect("hash").as_str().to_owned(),
                role: SystemRole::User,
            },
        )
        .await
        .expect("creazione utente")
        .id
}

/// Procura un database vergine.
///
/// Percorso predefinito: **un** container `postgis/postgis:17-3.5` per
/// processo, e un `CREATE DATABASE` per test. Il boot del container è la
/// parte lenta; condividerlo è il checkpoint prestazioni della 1a.
///
/// Fase 7 Task 3: i test restano su `PostGIS` senza pgvector finché Task 4
/// non introduce lo schema vettoriale. Il percorso degradato
/// (`probe_pgvector` → `available: false`) è coperto da `tests/pgvector.rs`
/// proprio su questa immagine. Compose bundled usa invece `Dockerfile.db`
/// (`PostGIS` + pgvector).
///
/// Percorso alternativo, attivo **solo** se `KEEPPIX_TEST_DATABASE_URL` è
/// impostata: si usa il server già in ascolto, stesso `CREATE DATABASE`.
#[allow(clippy::expect_used)]
async fn provision() -> ProvisionedDb {
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
                .expect("avvio del container Postgres");
            let port = mapped_port(&container).await;
            let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
            (container, url)
        })
        .await;

    named_database(admin_url).await
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

struct ProvisionedDb {
    url: String,
    admin_url: String,
    name: String,
}

#[allow(clippy::expect_used)]
async fn named_database(server_url: &str) -> ProvisionedDb {
    let name = format!("keeppix_test_{}", uuid::Uuid::now_v7().simple());
    let mut admin = PgConnection::connect(server_url)
        .await
        .expect("connessione al server Postgres");
    sqlx::query(&format!("CREATE DATABASE \"{name}\""))
        .execute(&mut admin)
        .await
        .expect("creazione del database di test");
    admin.close().await.ok();
    ProvisionedDb {
        url: with_database(server_url, &name),
        admin_url: server_url.to_owned(),
        name,
    }
}

/// Drop a per-test database. `WITH (FORCE)` (PG 13+) terminates leftover
/// pool connections so Drop from an async test does not deadlock. Best-effort:
/// a failed drop only leaks disk until the next suite, never fails the test.
fn drop_test_database(admin_url: &str, name: &str) {
    // Names are `keeppix_test_` + uuid; refuse anything else as a belt-and-braces
    // guard against a bad refactor interpolating untrusted input into DDL.
    if !name.starts_with("keeppix_test_")
        || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return;
    }
    let admin_url = admin_url.to_owned();
    let name = name.to_owned();
    let _ = std::thread::spawn(move || {
        let Ok(rt) = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        else {
            return;
        };
        rt.block_on(async move {
            let Ok(mut admin) = PgConnection::connect(&admin_url).await else {
                return;
            };
            let _ = sqlx::query(&format!("DROP DATABASE IF EXISTS \"{name}\" WITH (FORCE)"))
                .execute(&mut admin)
                .await;
            admin.close().await.ok();
        });
    })
    .join();
}

/// Sostituisce il nome del database in un URL di connessione, conservando
/// credenziali, host, porta e parametri di query.
fn with_database(server_url: &str, name: &str) -> String {
    let (base, query) = match server_url.split_once('?') {
        Some((base, query)) => (base, Some(query)),
        None => (server_url, None),
    };

    // Il primo `/` dopo `scheme://` apre il nome del database; senza schema
    // non c'è authority da saltare.
    let authority_start = base.find("://").map_or(0, |i| i + 3);
    let without_db = base[authority_start..]
        .find('/')
        .map_or(base, |i| &base[..authority_start + i]);

    match query {
        Some(query) => format!("{without_db}/{name}?{query}"),
        None => format!("{without_db}/{name}"),
    }
}

/// Questi test girano in ogni binario di integrazione di questo crate: il
/// modulo `harness` è incluso da ciascuno. È chirurgia su stringhe, cioè
/// esattamente il posto dove un refuso passa inosservato finché non fa
/// puntare i test al database sbagliato.
#[cfg(test)]
mod tests {
    use super::with_database;

    #[test]
    fn replaces_an_existing_database_name() {
        assert_eq!(
            with_database("postgres://u:p@127.0.0.1:5432/postgres", "kpx"),
            "postgres://u:p@127.0.0.1:5432/kpx"
        );
    }

    #[test]
    fn appends_when_the_url_has_no_database() {
        assert_eq!(
            with_database("postgres://u:p@127.0.0.1:5432", "kpx"),
            "postgres://u:p@127.0.0.1:5432/kpx"
        );
    }

    #[test]
    fn preserves_the_query_string() {
        assert_eq!(
            with_database(
                "postgres://u:p@127.0.0.1:5432/postgres?sslmode=disable",
                "kpx"
            ),
            "postgres://u:p@127.0.0.1:5432/kpx?sslmode=disable"
        );
    }
}
