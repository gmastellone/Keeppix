//! Real server on an ephemeral port with a real Postgres.
//! Tests speak HTTP like a browser, cookies included: it's the only way to
//! actually verify session cookie behavior.

// This module is included by multiple test binaries (`auth.rs`, `openapi.rs`)
// and each one uses only part of it: what one needs is dead code in the
// other. Without this `allow`, `stop_database()` — used only by `auth.rs` —
// would fail compilation of `openapi.rs` with `-D warnings`.
#![allow(dead_code, unused_imports)]

use std::time::Duration;

use keeppix_db::Db;
use sqlx::{Connection as _, PgConnection};
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::{ContainerAsync, ImageExt};
use tokio::sync::OnceCell;

static SHARED: OnceCell<(ContainerAsync<Postgres>, String)> = OnceCell::const_new();
static SHARED_VECTOR: OnceCell<(ContainerAsync<Postgres>, String)> = OnceCell::const_new();

pub struct TestServer {
    // `Some` only on the stoppable path (the 503 test). The shared container
    // lives in the `OnceCell` and never stops.
    container: Option<ContainerAsync<Postgres>>,
    pub db: Db,
    pub database_url: String,
    admin_url: String,
    db_name: String,
    pub data_dir: std::path::PathBuf,
    /// Allowlist root for `KEEPPIX_LIBRARY_ROOTS` in tests (under `data_dir`).
    pub photos_root: std::path::PathBuf,
    pub auth_pings: std::sync::Arc<std::sync::atomic::AtomicU64>,
    pub viewport_pings: std::sync::Arc<std::sync::atomic::AtomicU64>,
    pub base_url: String,
    pub client: reqwest::Client,
}

impl TestServer {
    /// # Panics
    /// If the database is unreachable or the server fails to start.
    #[allow(clippy::expect_used)]
    pub async fn start() -> Self {
        boot(None, provision().await, |state| state).await
    }

    /// Like [`Self::start`], but on Postgres with pgvector (`keeppix-db:dev`
    /// or a `KEEPPIX_TEST_DATABASE_URL` that offers it). The AI tests
    /// (`/tags`, …) must use it: schema `0043` is a no-op without `vector`.
    #[allow(clippy::expect_used)]
    pub async fn start_with_vector() -> Self {
        boot(None, provision_with_vector().await, |state| state).await
    }

    /// Like [`Self::start`], but with an already-modified state (fake
    /// demosaic, cache cap, TTL). The `/media/full` tests on RAW files use it
    /// to avoid depending on `dcraw_emu` in CI.
    #[allow(clippy::expect_used)]
    pub async fn start_with(
        configure: impl FnOnce(keeppix_api::AppState) -> keeppix_api::AppState,
    ) -> Self {
        boot(None, provision().await, configure).await
    }

    /// A dedicated container, so `stop_database` doesn't shut down the
    /// Postgres used by other tests in the same binary.
    #[allow(clippy::expect_used)]
    pub async fn start_stoppable() -> Self {
        let (container, provisioned) = provision_dedicated().await;
        boot(container, provisioned, |state| state).await
    }

    pub fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base_url)
    }

    /// Shuts down the Postgres backing the server, to observe how the HTTP
    /// surface behaves when the database stops responding. It's the only way
    /// to actually verify that property: no mock sits between the handlers
    /// and the pool.
    ///
    /// Returns `false` on the `KEEPPIX_TEST_DATABASE_URL` path, where the
    /// Postgres server is shared with other tests and stopping it would
    /// break them: the caller skips the test in that case.
    ///
    /// # Panics
    /// If the container exists but cannot be stopped.
    #[allow(clippy::expect_used)]
    pub async fn stop_database(&self) -> bool {
        match self.container.as_ref() {
            Some(container) => {
                container.stop().await.expect("Postgres container stop");
                true
            }
            None => false,
        }
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        drop_test_database(&self.admin_url, &self.db_name);
        let _ = std::fs::remove_dir_all(&self.data_dir);
    }
}

#[allow(clippy::expect_used)]
async fn boot(
    container: Option<ContainerAsync<Postgres>>,
    provisioned: ProvisionedDb,
    configure: impl FnOnce(keeppix_api::AppState) -> keeppix_api::AppState,
) -> TestServer {
    let url = provisioned.url.clone();
    let db = Db::connect(&url, 5).await.expect("connection");
    db.migrate().await.expect("migrations");

    let data_dir = std::env::temp_dir().join(format!("keeppix-api-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&data_dir).expect("data_dir");
    let photos_root = data_dir.join("photos");
    std::fs::create_dir_all(&photos_root).expect("photos_root");
    let auth_pings = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
    let ping = auth_pings.clone();
    let viewport_pings = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
    let viewport_ping = viewport_pings.clone();
    let watchers =
        keeppix_jobs::watch::LibraryWatchers::new(db.clone(), std::time::Duration::from_millis(80));
    let state = configure(
        keeppix_api::AppState::new(db.clone(), 3600, data_dir.clone())
            .with_database_url(url.clone())
            .with_library_roots(vec![photos_root.clone()])
            .with_library_watchers(watchers)
            .with_on_authenticated(std::sync::Arc::new(move || {
                ping.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }))
            .with_on_viewport_activity(std::sync::Arc::new(move || {
                viewport_ping.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            })),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("address");

    tokio::spawn(async move {
        axum::serve(listener, keeppix_api::router(state)).await.ok();
    });

    let client = reqwest::Client::builder()
        .cookie_store(true)
        .default_headers(client_headers())
        .build()
        .expect("http client");

    TestServer {
        container,
        db,
        database_url: url,
        admin_url: provisioned.admin_url,
        db_name: provisioned.name,
        data_dir,
        photos_root,
        auth_pings,
        viewport_pings,
        base_url: format!("http://{addr}"),
        client,
    }
}

/// Custom header required on mutations by the CSRF layer
/// (`keeppix_api::csrf`), exactly as the frontend's `apiFetch` sends it on
/// every call. Both the harness client and `plain_client()` carry it by
/// default, so tests speak like the real client; the two tests that verify
/// the layer itself build a client without the header by hand.
#[allow(clippy::expect_used)]
pub fn client_headers() -> reqwest::header::HeaderMap {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        keeppix_api::csrf::CLIENT_HEADER,
        reqwest::header::HeaderValue::from_static("test"),
    );
    headers
}

/// A client without a cookie store — for tests that resubmit a specific
/// cookie by hand — but with the custom header, i.e. a legitimate client
/// that simply doesn't remember sessions.
#[allow(clippy::expect_used)]
pub fn plain_client() -> reqwest::Client {
    reqwest::Client::builder()
        .default_headers(client_headers())
        .build()
        .expect("http client")
}

/// Runs the job pipeline on owned clones, so it can live inside a
/// `tokio::spawn` while the test remains free to query the database or call
/// other HTTP routes in the meantime (used to observe cancellation midway
/// through a long scan, both via `keeppix-api` and via WebSocket).
#[allow(clippy::expect_used, dead_code)]
pub fn spawn_worker_pool(
    db: Db,
    database_url: String,
    data_dir: std::path::PathBuf,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let tracker = std::sync::Arc::new(keeppix_jobs::ActivityTracker::new());
        let handler = keeppix_jobs::IngestHandler {
            db: db.clone(),
            data_dir,
            stability_wait: Duration::ZERO,
            trash_retention_days: keeppix_db::TRASH_RETENTION_DAYS,
            database_url,
            config_path: None,
            activity: tracker.clone(),
        };
        let pool = keeppix_jobs::WorkerPool::new(
            db.clone(),
            handler,
            tracker,
            512 * 1024 * 1024,
            keeppix_jobs::default_night_window(),
            std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        );
        let start = std::time::Instant::now();
        loop {
            if start.elapsed() > Duration::from_secs(60) {
                return;
            }
            if !pool.step().await.expect("step") {
                // Only jobs that are already claimable: `pending` jobs with a
                // `run_after` in the future (backoff retry) don't keep the
                // worker alive — otherwise a batch of failed derives on an
                // invalid fixture would time out the cancel tests (20s) while
                // the pool spins idle.
                let pending: i64 = sqlx::query_scalar(
                    "SELECT count(*) FROM jobs \
                     WHERE status = 'running' \
                        OR (status = 'pending' AND run_after <= now())",
                )
                .fetch_one(db.pool())
                .await
                .expect("count");
                if pending == 0 {
                    return;
                }
            }
        }
    })
}

/// The security-header assertions live in `keeppix-test-support`, a single
/// copy for the whole workspace: `keeppix-api` and `keeppix-server` can't
/// share test code any other way. Re-exported here so files that declare
/// `mod harness;` keep importing it from one place.
pub use keeppix_test_support::assert_security_headers;

/// Provisions a fresh database. One container per process, one `CREATE
/// DATABASE` per test — aligned with `crates/keeppix-jobs/tests/harness/mod.rs`.
///
/// Default: `postgis/postgis:17-3.5` (or `KEEPPIX_TEST_DATABASE_URL` in CI).
/// The AI schema is a no-op without `vector`; tests that need it use
/// [`TestServer::start_with_vector`].
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
                .expect("Postgres container");
            let port = mapped_port(&container).await;
            let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
            (container, url)
        })
        .await;
    named_database(admin_url).await
}

/// Postgres with pgvector for the AI tests. Same contract as
/// `keeppix-jobs::tests::harness::provision_with_vector`.
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
                    "starting the Postgres container (keeppix-db:dev); \
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

#[allow(clippy::expect_used)]
async fn provision_dedicated() -> (Option<ContainerAsync<Postgres>>, ProvisionedDb) {
    if std::env::var("KEEPPIX_TEST_DATABASE_URL").is_ok() {
        return (None, provision().await);
    }
    let container = Postgres::default()
        .with_tag("17-3.5")
        .with_name("postgis/postgis")
        .start()
        .await
        .expect("Postgres container");
    let port = mapped_port(&container).await;
    let admin_url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    // Dedicated container: use a named DB so Drop can clean it; stopping the
    // container later still tears the whole cluster down for the 503 test.
    let provisioned = named_database(&admin_url).await;
    (Some(container), provisioned)
}

/// Docker Desktop sometimes exposes the port a moment after `start()`.
/// Without retry, `PortNotExposed` randomly fails 1-3 tests locally.
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
            Err(err) => panic!("port not mapped after {attempt} attempts: {err}"),
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
        .expect("connection to the existing Postgres server");
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

    let authority_start = base.find("://").map_or(0, |i| i + 3);
    let without_db = base[authority_start..]
        .find('/')
        .map_or(base, |i| &base[..authority_start + i]);

    match query {
        Some(query) => format!("{without_db}/{name}?{query}"),
        None => format!("{without_db}/{name}"),
    }
}
