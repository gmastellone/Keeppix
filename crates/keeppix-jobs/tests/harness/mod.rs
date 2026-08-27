//! Real Postgres for integration tests.
//! Every `TestDb` is isolated: a fresh database, migrations applied.
//! The Postgres container is one per process: the boot cost is paid once,
//! isolation stays a `CREATE DATABASE` per test.

#![allow(dead_code)]

use std::time::Duration;

use keeppix_db::Db;
use sqlx::{Connection as _, PgConnection};
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::{ContainerAsync, ImageExt};
use tokio::sync::OnceCell;

pub struct TestDb {
    db: Db,
    #[allow(dead_code)]
    database_url: String,
    admin_url: String,
    db_name: String,
}

static SHARED: OnceCell<(ContainerAsync<Postgres>, String)> = OnceCell::const_new();
static SHARED_VECTOR: OnceCell<(ContainerAsync<Postgres>, String)> = OnceCell::const_new();

impl TestDb {
    /// # Panics
    /// If the database isn't reachable or migrations fail: in a test that's
    /// the desired behavior.
    #[allow(clippy::expect_used)]
    pub async fn start() -> Self {
        let provisioned = provision().await;

        let db = Db::connect(&provisioned.url, 5).await.expect("connection");
        db.migrate().await.expect("migrations");

        Self {
            db,
            database_url: provisioned.url,
            admin_url: provisioned.admin_url,
            db_name: provisioned.name,
        }
    }

    /// Postgres with pgvector (`keeppix-db:dev`) for AI tests.
    ///
    /// # Panics
    /// If the image fails to start or migrations fail.
    #[allow(clippy::expect_used, dead_code)]
    pub async fn start_with_vector() -> Self {
        let provisioned = provision_with_vector().await;
        let db = Db::connect(&provisioned.url, 5).await.expect("connection");
        db.migrate().await.expect("migrations");
        Self {
            db,
            database_url: provisioned.url,
            admin_url: provisioned.admin_url,
            db_name: provisioned.name,
        }
    }

    #[must_use]
    pub const fn db(&self) -> &Db {
        &self.db
    }

    /// Connection string for tools that cannot use the pool (`pg_dump`/`pg_restore`).
    #[must_use]
    #[allow(dead_code)]
    pub fn database_url(&self) -> &str {
        &self.database_url
    }
}

impl Drop for TestDb {
    fn drop(&mut self) {
        drop_test_database(&self.admin_url, &self.db_name);
    }
}

/// Creates an admin and returns their id. Every test that needs one needs
/// an owner for libraries.
///
/// # Panics
/// If creation fails: in a test that's the desired behavior.
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
        .expect("admin creation")
        .id
}

/// Creates a non-admin user. Needed by every test that checks permissions.
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
        .expect("user creation")
        .id
}

/// Provisions a fresh database.
///
/// Default path: **one** `postgis/postgis:17-3.5` container per process,
/// and a `CREATE DATABASE` per test. Booting the container is the slow
/// part; sharing it is the performance win.
///
/// The tests in this crate stay on `PostGIS` without pgvector. The AI
/// schema (`0043`) is a no-op if `vector` isn't installable, so migrate
/// doesn't fail. The bundled compose setup builds `Dockerfile.db`
/// (`PostGIS` + pgvector); the AI schema tests live in `keeppix-db`.
///
/// Alternate path, active **only** if `KEEPPIX_TEST_DATABASE_URL` is set:
/// uses the server already listening, same `CREATE DATABASE`.
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
                .expect("Postgres container startup");
            let port = mapped_port(&container).await;
            let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
            (container, url)
        })
        .await;

    named_database(admin_url).await
}

/// Like [`provision`], but on `keeppix-db:dev` (`PostGIS` + pgvector) for
/// tests that write `asset_embeddings`.
#[allow(clippy::expect_used)]
async fn provision_with_vector() -> ProvisionedDb {
    if let Ok(server_url) = std::env::var("KEEPPIX_TEST_DATABASE_URL")
        && server_offers_vector(&server_url).await
    {
        return named_database(&server_url).await;
    }

    let (_container, admin_url) = SHARED_VECTOR
        .get_or_init(|| async {
            let container = Postgres::default()
                .with_tag("dev")
                .with_name("keeppix-db")
                .start()
                .await
                .expect(
                    "Postgres container startup (keeppix-db:dev); \
                     build it with: docker build -f Dockerfile.db -t keeppix-db:dev .",
                );
            let port = mapped_port(&container).await;
            let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
            (container, url)
        })
        .await;

    named_database(admin_url).await
}

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
/// Without a retry, `PortNotExposed` randomly fails 1-3 tests locally.
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
            Err(err) => panic!("port mapped after {attempt} attempts: {err}"),
        }
    }
    unreachable!("the loop above always terminates")
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
        .expect("test database creation");
    admin.close().await.ok();
    ProvisionedDb {
        url: with_database(server_url, &name),
        admin_url: server_url.to_owned(),
        name,
    }
}

fn drop_test_database(admin_url: &str, name: &str) {
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
    // scheme there's no authority to skip.
    let authority_start = base.find("://").map_or(0, |i| i + 3);
    let without_db = base[authority_start..]
        .find('/')
        .map_or(base, |i| &base[..authority_start + i]);

    match query {
        Some(query) => format!("{without_db}/{name}?{query}"),
        None => format!("{without_db}/{name}"),
    }
}

/// These tests run in every integration binary in this crate: the
/// `harness` module is included by each one. This is string surgery,
/// exactly the kind of place where a typo goes unnoticed until it points
/// the tests at the wrong database.
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
