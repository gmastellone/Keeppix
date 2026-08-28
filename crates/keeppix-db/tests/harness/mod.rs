//! A real Postgres for integration tests.
//! Each `TestDb` is isolated: a fresh database with migrations applied.
//! The Postgres container is one per process: the boot cost is paid once,
//! isolation is just a `CREATE DATABASE` per test.

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
    /// If the database is unreachable or the migrations fail: in a test
    /// that is the intended behavior.
    #[allow(clippy::expect_used)]
    pub async fn start() -> Self {
        let provisioned = provision().await;

        let db = Db::connect(&provisioned.url, 5).await.expect("connection");
        db.migrate().await.expect("migrations");

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

    /// A database with `log_statements` enabled and a layer that captures
    /// the queries sqlx emits — for tests that need to assert the absence
    /// of aggregates.
    ///
    /// # Panics
    /// Same as [`Self::start`].
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
            .expect("tracked connection");
        db.migrate().await.expect("migrations");

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

    /// A database on `postgis/postgis:17-3.5` **without** pgvector — the
    /// degraded path (an external Postgres without the extension). Does
    /// not share the container from [`Self::start`].
    ///
    /// # Panics
    /// If the container fails to start or the migrations fail.
    #[allow(clippy::expect_used)]
    pub async fn start_postgis_only() -> Self {
        static POSTGIS_ONLY: OnceCell<(ContainerAsync<Postgres>, String)> = OnceCell::const_new();

        let (_container, admin_url) = POSTGIS_ONLY
            .get_or_init(|| async {
                let container = Postgres::default()
                    .with_tag("17-3.5")
                    .with_name("postgis/postgis")
                    .start()
                    .await
                    .expect("start the PostGIS-only container");
                let port = mapped_port(&container).await;
                let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
                (container, url)
            })
            .await;

        let provisioned = named_database(admin_url).await;
        let db = Db::connect(&provisioned.url, 5)
            .await
            .expect("PostGIS-only connection");
        db.migrate()
            .await
            .expect("migrations on PostGIS-only (must succeed without vector)");
        Self {
            db,
            admin_url: provisioned.admin_url,
            db_name: provisioned.name,
        }
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

/// Creates an administrator and returns their id. Tests need an owner for
/// libraries.
///
/// # Panics
/// If creation fails: in a test that is the intended behavior.
#[allow(clippy::expect_used, dead_code)]
pub async fn seed_admin(test: &TestDb) -> keeppix_domain::UserId {
    use keeppix_domain::{NewUser, Password, SystemRole, Username, hash_password};

    let password = Password::parse("correct horse battery staple").expect("valid password");
    keeppix_db::UserRepo::new(test.db())
        .create_bootstrap_admin(NewUser {
            username: Username::parse("giovanni").expect("valid username"),
            email: None,
            display_name: "Giovanni".to_owned(),
            password_hash: hash_password(&password).expect("hash").as_str().to_owned(),
            role: SystemRole::Admin,
        })
        .await
        .expect("create admin")
        .id
}

/// Creates a non-admin user. Used by every test that checks permissions.
///
/// # Panics
/// If creation fails.
#[allow(clippy::expect_used, dead_code)]
pub async fn seed_user(
    test: &TestDb,
    admin: keeppix_domain::UserId,
    username: &str,
) -> keeppix_domain::UserId {
    use keeppix_domain::{AuthContext, NewUser, Password, SystemRole, Username, hash_password};

    let password = Password::parse("correct horse battery staple").expect("valid password");
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    keeppix_db::UserRepo::new(test.db())
        .create(
            &ctx,
            NewUser {
                username: Username::parse(username).expect("valid username"),
                email: None,
                display_name: username.to_owned(),
                password_hash: hash_password(&password).expect("hash").as_str().to_owned(),
                role: SystemRole::User,
            },
        )
        .await
        .expect("create user")
        .id
}

/// Provisions a fresh database.
///
/// Default path: **one** `keeppix-db:dev` container (`Dockerfile.db`:
/// `PostGIS` + pgvector) per process, and a `CREATE DATABASE` per test. The
/// container boot is the slow part; sharing it was the main performance
/// win. The AI schema uses the `vector` type, so this crate's tests run on
/// the bundled image.
///
/// If `KEEPPIX_TEST_DATABASE_URL` is set **and** that server offers
/// `vector` in `pg_available_extensions`, that server is used. Otherwise it
/// falls back to `keeppix-db:dev` (a URL without pgvector would make the
/// AI schema a no-op and its column tests would lie).
///
/// The degraded path (without the pgvector extension) stays in
/// [`TestDb::start_postgis_only`].
#[allow(clippy::expect_used)]
async fn provision() -> ProvisionedDb {
    if let Ok(server_url) = std::env::var("KEEPPIX_TEST_DATABASE_URL")
        && server_offers_vector(&server_url).await
    {
        return named_database(&server_url).await;
    }

    let (_container, admin_url) = SHARED
        .get_or_init(|| async {
            let container = Postgres::default()
                .with_tag("dev")
                .with_name("keeppix-db")
                .start()
                .await
                .expect(
                    "start the Postgres container (keeppix-db:dev); \
                     build it with: docker build -f Dockerfile.db -t keeppix-db:dev .",
                );
            let port = mapped_port(&container).await;
            let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
            (container, url)
        })
        .await;

    named_database(admin_url).await
}

/// `true` if the server exposes the `vector` extension (package installed).
#[allow(clippy::expect_used)]
async fn server_offers_vector(server_url: &str) -> bool {
    let Ok(mut conn) = PgConnection::connect(server_url).await else {
        return false;
    };
    let offered = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM pg_available_extensions WHERE name = 'vector')",
    )
    .fetch_one(&mut conn)
    .await
    .unwrap_or(false);
    conn.close().await.ok();
    offered
}

/// Docker Desktop sometimes exposes the port a moment after `start()`.
/// Without retries, `PortNotExposed` makes 1-3 random tests fail locally.
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
            Err(err) => panic!("port mapping failed after {attempt} attempts: {err}"),
        }
    }
    unreachable!("the loop above always returns")
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
        .expect("connection to the Postgres server");
    sqlx::query(&format!("CREATE DATABASE \"{name}\""))
        .execute(&mut admin)
        .await
        .expect("create test database");
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

/// Replaces the database name in a connection URL, preserving credentials,
/// host, port, and query parameters.
fn with_database(server_url: &str, name: &str) -> String {
    let (base, query) = match server_url.split_once('?') {
        Some((base, query)) => (base, Some(query)),
        None => (server_url, None),
    };

    // The first `/` after `scheme://` opens the database name; with no
    // scheme there is no authority to skip.
    let authority_start = base.find("://").map_or(0, |i| i + 3);
    let without_db = base[authority_start..]
        .find('/')
        .map_or(base, |i| &base[..authority_start + i]);

    match query {
        Some(query) => format!("{without_db}/{name}?{query}"),
        None => format!("{without_db}/{name}"),
    }
}

/// These tests run in every integration binary of this crate: the
/// `harness` module is included by each of them. This is string surgery,
/// which is exactly the kind of place where a typo goes unnoticed until it
/// points the tests at the wrong database.
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
