mod harness;

use std::str::FromStr;
use std::sync::{Arc, Mutex};

use harness::TestServer;
use keeppix_db::{Db, FolderRepo, LibraryRepo, PreferencesRepo, UserRepo};
use keeppix_domain::{AuthContext, NewLibrary, SystemRole};
use serde_json::json;
use sqlx::ConnectOptions;
use sqlx::postgres::PgConnectOptions;
use tracing::Subscriber;
use tracing_subscriber::Layer;
use tracing_subscriber::prelude::*;

#[allow(clippy::expect_used, clippy::unwrap_used)]
async fn setup_admin(server: &TestServer) -> keeppix_domain::UserId {
    let response = server
        .client
        .post(server.url("/api/v1/setup"))
        .json(&json!({
            "username": "giovanni",
            "display_name": "Giovanni",
            "password": "correct horse battery staple"
        }))
        .send()
        .await
        .expect("setup");
    assert_eq!(response.status(), 201);
    response.json::<serde_json::Value>().await.expect("json")["user"]["id"]
        .as_str()
        .expect("id")
        .parse()
        .expect("uuid")
}

#[allow(clippy::expect_used, clippy::unwrap_used)]
async fn seed_library(server: &TestServer, admin: keeppix_domain::UserId) -> String {
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let root = server.photos_root.join("bootstrap-lib");
    std::fs::create_dir_all(&root).expect("library dir");
    let library = LibraryRepo::new(&server.db)
        .create(
            &ctx,
            NewLibrary {
                name: "Foto".to_owned(),
                owner_id: admin,
                root_path: root,
                exclude_patterns: vec![],
            },
        )
        .await
        .expect("library");
    library.id.to_string()
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
            #[allow(clippy::expect_used)]
            {
                self.0.lock().expect("lock").push(visitor.0);
            }
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

/// Capture sqlx **process-wide**: `set_default` (TLS) perde gli eventi quando
/// sqlx logga da un worker diverso dal task del test (CI parallela →
/// `individual=0`). `set_global_default` una volta per binario; i budget test
/// si serializzano su `BUDGET_LOCK`.
fn global_sql_capture() -> Arc<Mutex<Vec<String>>> {
    static CAPTURE: std::sync::OnceLock<Arc<Mutex<Vec<String>>>> = std::sync::OnceLock::new();
    static INIT: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    let captured = CAPTURE
        .get_or_init(|| Arc::new(Mutex::new(Vec::new())))
        .clone();
    INIT.get_or_init(|| {
        let subscriber = tracing_subscriber::Registry::default()
            .with(tracing_subscriber::EnvFilter::new("sqlx=debug"))
            .with(SqlCapture(captured.clone()));
        // Ok se un altro test ha già installato un global: in quel caso il
        // capture non riceve nulla e l'assert individual>0 fallisce in modo
        // esplicito — meglio di un falso verde.
        let _ = tracing::subscriber::set_global_default(subscriber);
    });
    captured
}

#[allow(clippy::expect_used)]
async fn traced_db(url: &str) -> (Db, Arc<Mutex<Vec<String>>>) {
    let captured = global_sql_capture();
    let options = PgConnectOptions::from_str(url)
        .expect("url")
        .log_statements(tracing::log::LevelFilter::Debug);
    let db = Db::connect_with(options, 5)
        .await
        .expect("connessione tracciata");
    (db, captured)
}

#[allow(clippy::expect_used, clippy::unwrap_used)]
async fn load_individual_repos(db: &Db, ctx: &AuthContext) {
    let user_id = ctx.user_id().expect("user");
    UserRepo::new(db)
        .find_by_id(ctx, user_id)
        .await
        .expect("user");
    PreferencesRepo::new(db).get(ctx).await.expect("prefs");
    FolderRepo::new(db).tree(ctx).await.expect("folders");
    let libraries = LibraryRepo::new(db).list(ctx).await.expect("libraries");
    let library_repo = LibraryRepo::new(db);
    for library in &libraries {
        library_repo
            .storage(ctx, library.id)
            .await
            .expect("storage");
    }
    // Fase 7 Task 9: metà tag del badge `revision` — stessa query di `compose`.
    keeppix_db::AssetTagRepo::new(db)
        .count_proposed_visible(ctx)
        .await
        .expect("proposed count");
    // Fase 8 Task 8: metà volti dello stesso badge — stessa query di `compose`.
    keeppix_db::FaceRepo::new(db)
        .count_proposed_visible(ctx)
        .await
        .expect("proposed count");
}

#[allow(clippy::expect_used, clippy::unwrap_used)]
async fn load_bootstrap_repos(db: &Db, ctx: &AuthContext) {
    keeppix_api::routes::bootstrap::compose(db, ctx, false, "Keeppix")
        .await
        .expect("bootstrap compose");
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn bootstrap_matches_individual_endpoints() {
    let server = TestServer::start().await;
    let admin = setup_admin(&server).await;
    let library_id = seed_library(&server, admin).await;

    let me = server
        .client
        .get(server.url("/api/v1/auth/me"))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    let prefs = server
        .client
        .get(server.url("/api/v1/users/me/preferences"))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    let folders = server
        .client
        .get(server.url("/api/v1/folders/tree"))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    let storage = server
        .client
        .get(server.url(&format!("/api/v1/libraries/{library_id}/storage")))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();

    let bootstrap = server
        .client
        .get(server.url("/api/v1/bootstrap"))
        .send()
        .await
        .unwrap();
    assert_eq!(bootstrap.status(), 200);
    let body: serde_json::Value = bootstrap.json().await.unwrap();

    assert_eq!(body["user"], me["user"]);
    assert_eq!(body["preferences"], prefs);
    assert_eq!(body["folders"], folders);
    assert_eq!(body["storage"][&library_id], storage);
    assert_eq!(body["badges"]["culling"], 0);
    assert_eq!(body["badges"]["revision"], 0);
}

/// Budget di query di `compose`: deve restare ≤ alla somma dei repo singoli.
#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn bootstrap_emits_no_more_queries_than_individual_repos() {
    let server = TestServer::start().await;
    let admin = setup_admin(&server).await;
    let _library_id = seed_library(&server, admin).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);

    assert_bootstrap_query_budget(&server.database_url, &ctx).await;
}

/// Stesso budget **dopo** un giro HTTP come `bootstrap_matches_*`.
#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn bootstrap_query_budget_still_holds_after_http_bootstrap_round_trip() {
    let server = TestServer::start().await;
    let admin = setup_admin(&server).await;
    let library_id = seed_library(&server, admin).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);

    let bootstrap = server
        .client
        .get(server.url("/api/v1/bootstrap"))
        .send()
        .await
        .unwrap();
    assert_eq!(bootstrap.status(), 200);
    let body: serde_json::Value = bootstrap.json().await.unwrap();
    assert!(body["storage"][&library_id].is_object());

    assert_bootstrap_query_budget(&server.database_url, &ctx).await;
}

#[allow(clippy::expect_used, clippy::unwrap_used)]
async fn assert_bootstrap_query_budget(database_url: &str, ctx: &AuthContext) {
    let _serial = BUDGET_LOCK.lock().await;

    let (db, captured) = traced_db(database_url).await;

    captured.lock().expect("lock").clear();
    load_individual_repos(&db, ctx).await;
    let individual = captured.lock().expect("lock").len();

    captured.lock().expect("lock").clear();
    load_bootstrap_repos(&db, ctx).await;
    let bootstrap = captured.lock().expect("lock").len();

    assert!(
        individual > 0,
        "il test deve catturare query dai singoli repository (individual=0: subscriber assente?)"
    );
    assert!(
        bootstrap <= individual,
        "bootstrap={bootstrap} query, singoli={individual}: deve essere ≤"
    );
}

static BUDGET_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
