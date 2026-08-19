mod harness;
mod journey;

use chrono::TimeZone as _;
use harness::TestServer;
use journey::{create_library, setup_admin};
use keeppix_db::AssetRepo;
use keeppix_domain::{AssetKind, AssetName, NewAsset};

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn delta_returns_upserted_assets_since_cursor() {
    let server = TestServer::start().await;
    let asset_id = seed_asset(&server).await;

    let resp = server
        .client
        .get(server.url("/api/v1/sync/delta?cursor=0"))
        .send()
        .await
        .expect("delta");
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.expect("json");
    let upserted = body["upserted"]
        .as_array()
        .expect("upserted array")
        .iter()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect::<Vec<_>>();
    assert!(
        upserted.contains(&asset_id.to_string()),
        "new asset must appear in upserted, got {upserted:?}"
    );
    assert_eq!(body["has_more"], false);
    assert!(body["cursor"].as_i64().is_some_and(|c| c >= 0));
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn delta_pagination_restarts_from_returned_cursor() {
    let server = TestServer::start().await;
    seed_asset(&server).await;

    let first = server
        .client
        .get(server.url("/api/v1/sync/delta?cursor=0"))
        .send()
        .await
        .expect("first page")
        .json::<serde_json::Value>()
        .await
        .expect("json");
    let cursor = first["cursor"].as_i64().expect("cursor");

    let second = server
        .client
        .get(server.url(&format!("/api/v1/sync/delta?cursor={cursor}")))
        .send()
        .await
        .expect("second page")
        .json::<serde_json::Value>()
        .await
        .expect("json");
    assert_eq!(second["upserted"].as_array().map(Vec::len), Some(0));
    assert_eq!(second["deleted"].as_array().map(Vec::len), Some(0));
    assert_eq!(second["has_more"], false);
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn delta_requires_authentication() {
    let server = TestServer::start().await;
    let resp = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("client")
        .get(server.url("/api/v1/sync/delta"))
        .send()
        .await
        .expect("request");
    assert_eq!(resp.status(), 401);
}

async fn seed_asset(server: &TestServer) -> keeppix_domain::AssetId {
    setup_admin(server).await;
    let root = server
        .photos_root
        .join(format!("sync-{}", uuid::Uuid::now_v7().simple()));
    std::fs::create_dir_all(&root).expect("library dir");
    let library_id = create_library(server, "sync-lib", &root).await;
    let library = library_id.parse().expect("library uuid");
    let folder = keeppix_db::FolderRepo::new(&server.db)
        .ensure_path(library, &[])
        .await
        .expect("root folder");
    AssetRepo::new(&server.db)
        .upsert_discovered(NewAsset {
            folder_id: folder.id,
            filename: AssetName::parse("delta.jpg").expect("name"),
            size_bytes: 100,
            mtime: chrono::Utc.with_ymd_and_hms(2024, 6, 1, 12, 0, 0).unwrap(),
            inode: Some(1),
            kind: AssetKind::Image,
        })
        .await
        .expect("asset")
        .expect("inserted")
        .id
}
