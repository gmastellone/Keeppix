//! Budget HTTP di ordine di grandezza (Fase 2R Task 8). Soglie larghe per
//! il bersaglio Raspberry Pi 5; in CI debug possono essere più lenti del
//! misurato su hardware reale.

mod harness;

use std::fs;
use std::time::{Duration, Instant};

use chrono::{TimeZone, Utc};
use harness::TestServer;
use keeppix_db::{FolderRepo, LibraryRepo, UserRepo};
use keeppix_domain::{
    AuthContext, NewLibrary, SystemRole, Username,
};
use serde_json::json;

const TIMELINE_ASSETS: usize = 10_000;
const LIBRARY_COUNT: usize = 20;

#[allow(clippy::expect_used)]
async fn setup_admin(server: &TestServer) {
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
}

#[allow(clippy::expect_used)]
async fn admin_ctx(server: &TestServer) -> AuthContext {
    let username = Username::parse("giovanni").unwrap();
    let (user, _) = UserRepo::new(&server.db)
        .find_by_username(&username)
        .await
        .unwrap()
        .expect("admin");
    AuthContext::user(user.id, SystemRole::Admin)
}

/// Diecimila asset indicizzati con `taken_at_utc` nello stesso mese — i
/// trigger mantengono `folder_month_counts` allineato.
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn seed_timeline_assets(server: &TestServer) -> String {
    setup_admin(server).await;
    let ctx = admin_ctx(server).await;
    let root = server.photos_root.join("budget-timeline");
    fs::create_dir_all(&root).expect("timeline root");

    let library = LibraryRepo::new(&server.db)
        .create(
            &ctx,
            NewLibrary {
                name: "Budget timeline".to_owned(),
                owner_id: ctx.user_id().unwrap(),
                root_path: root,
                exclude_patterns: vec![],
            },
        )
        .await
        .expect("library");
    let folder = FolderRepo::new(&server.db)
        .ensure_path(library.id, &["2024", "06"])
        .await
        .expect("folder");

    let ids: Vec<uuid::Uuid> = (0..TIMELINE_ASSETS)
        .map(|_| uuid::Uuid::now_v7())
        .collect();
    let filenames: Vec<String> = (0..TIMELINE_ASSETS)
        .map(|i| format!("IMG_{i:05}.jpg"))
        .collect();
    let taken = Utc.with_ymd_and_hms(2024, 6, 15, 12, 0, 0).unwrap();

    sqlx::query(
        "INSERT INTO assets (id, folder_id, filename, size_bytes, mtime, kind, status, \
         taken_at_utc, width, height) \
         SELECT aid, $2, name, 200000, $3, 'image', 'indexed', $3, 100, 100 \
           FROM unnest($1::uuid[], $4::text[]) AS t(aid, name)",
    )
    .bind(&ids)
    .bind(folder.id.as_uuid())
    .bind(taken)
    .bind(&filenames)
    .execute(server.db.pool())
    .await
    .expect("bulk insert assets");

    let counted: i64 = sqlx::query_scalar(
        "SELECT coalesce(sum(asset_count), 0)::bigint FROM folder_month_counts",
    )
    .fetch_one(server.db.pool())
    .await
    .expect("month counts");
    assert_eq!(
        counted,
        i64::try_from(TIMELINE_ASSETS).expect("count fits i64"),
        "i trigger devono aver riempito folder_month_counts"
    );

    "2024-06".to_owned()
}

#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn seed_twenty_libraries(server: &TestServer) {
    setup_admin(server).await;
    for i in 0..LIBRARY_COUNT {
        let name = format!("lib-{i:02}");
        let root = server.photos_root.join(&name);
        fs::create_dir_all(&root).expect("library dir");
        let response = server
            .client
            .post(server.url("/api/v1/libraries"))
            .json(&json!({
                "name": name,
                "root_path": root.to_string_lossy(),
            }))
            .send()
            .await
            .expect("create library");
        assert_eq!(response.status(), 201, "library {i}");
    }
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn timeline_buckets_with_ten_thousand_assets_stays_within_budget() {
    let server = TestServer::start().await;
    let _bucket = seed_timeline_assets(&server).await;

    let start = Instant::now();
    let response = server
        .client
        .get(server.url("/api/v1/timeline/buckets"))
        .send()
        .await
        .expect("buckets");
    let elapsed = start.elapsed();
    eprintln!("MEASUREMENT GET /timeline/buckets ({TIMELINE_ASSETS} assets): {elapsed:?}");

    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.expect("json");
    assert!(
        body.as_array().is_some_and(|a| !a.is_empty()),
        "bucket attesi con asset indicizzati"
    );
    assert!(
        elapsed < Duration::from_millis(200),
        "GET /timeline/buckets su {TIMELINE_ASSETS} asset: {elapsed:?} (budget 200 ms)"
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn timeline_page_with_ten_thousand_assets_stays_within_budget() {
    let server = TestServer::start().await;
    let bucket = seed_timeline_assets(&server).await;

    let start = Instant::now();
    let response = server
        .client
        .get(server.url(&format!("/api/v1/timeline?bucket={bucket}&limit=50")))
        .send()
        .await
        .expect("timeline page");
    let elapsed = start.elapsed();
    eprintln!("MEASUREMENT GET /timeline one page ({TIMELINE_ASSETS} assets): {elapsed:?}");

    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.expect("json");
    assert!(
        body["assets"].as_array().is_some_and(|a| !a.is_empty()),
        "pagina timeline non vuota"
    );
    assert!(
        elapsed < Duration::from_millis(300),
        "GET /timeline su {TIMELINE_ASSETS} asset: {elapsed:?} (budget 300 ms)"
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn listing_twenty_libraries_stays_within_budget() {
    let server = TestServer::start().await;
    seed_twenty_libraries(&server).await;

    let start = Instant::now();
    let response = server
        .client
        .get(server.url("/api/v1/libraries"))
        .send()
        .await
        .expect("libraries");
    let elapsed = start.elapsed();
    eprintln!("MEASUREMENT GET /libraries ({LIBRARY_COUNT} libraries): {elapsed:?}");

    assert_eq!(response.status(), 200);
    let list: serde_json::Value = response.json().await.expect("json");
    assert_eq!(
        list.as_array().map(Vec::len),
        Some(LIBRARY_COUNT),
        "lista completa delle librerie create"
    );
    assert!(
        elapsed < Duration::from_millis(100),
        "GET /libraries con {LIBRARY_COUNT} librerie: {elapsed:?} (budget 100 ms)"
    );
}
