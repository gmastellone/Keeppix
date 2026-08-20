#![allow(clippy::expect_used, clippy::unwrap_used)]

mod harness;
mod journey;

use chrono::TimeZone as _;
use harness::TestServer;
use journey::{create_library, setup_admin};
use keeppix_db::{AssetRepo, CHANGE_LOG_PAGE};
use keeppix_domain::{AssetId, AssetKind, AssetName, NewAsset};
use sqlx::PgConnection;

#[tokio::test]
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
async fn delta_pagination_restarts_from_returned_cursor() {
    let server = TestServer::start().await;
    let asset_id = seed_asset(&server).await.to_string();

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
    assert_eq!(first["has_more"], false);

    let second = server
        .client
        .get(server.url(&format!("/api/v1/sync/delta?cursor={cursor}")))
        .send()
        .await
        .expect("second page")
        .json::<serde_json::Value>()
        .await
        .expect("json");
    // `safe_cursor` may retract and re-deliver already-seen upserts; clients
    // must treat them as idempotent. What must not happen is losing the asset
    // or inventing unrelated ones after a quiescent page.
    let second_ids = upserted_ids(&second);
    assert!(
        second_ids.is_empty() || second_ids.iter().all(|id| id == &asset_id),
        "post-cursor page may only re-deliver known upserts, got {second_ids:?}"
    );
    assert_eq!(second["has_more"], false);
}

#[tokio::test]
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

#[tokio::test]
async fn delta_http_overlapping_transactions_do_not_drop_rows() {
    let server = TestServer::start().await;
    let folder = seed_folder(&server).await;

    let mut tx_a = server.db.pool().begin().await.unwrap();
    let id_a = insert_asset_on(&mut tx_a, folder, "a.jpg").await;

    let mut tx_b = server.db.pool().begin().await.unwrap();
    let id_b = insert_asset_on(&mut tx_b, folder, "b.jpg").await;
    tx_b.commit().await.unwrap();

    let first = server
        .client
        .get(server.url("/api/v1/sync/delta?cursor=0"))
        .send()
        .await
        .expect("first delta")
        .json::<serde_json::Value>()
        .await
        .expect("json");
    let first_ids = upserted_ids(&first);
    assert!(
        first_ids.contains(&id_b.to_string()),
        "committed B must appear on the HTTP delta: {first_ids:?}"
    );

    tx_a.commit().await.unwrap();

    let cursor = first["cursor"].as_i64().expect("cursor");
    let second = server
        .client
        .get(server.url(&format!("/api/v1/sync/delta?cursor={cursor}")))
        .send()
        .await
        .expect("second delta")
        .json::<serde_json::Value>()
        .await
        .expect("json");
    let seen: Vec<String> = first_ids.into_iter().chain(upserted_ids(&second)).collect();
    assert!(
        seen.contains(&id_a.to_string()),
        "A commits after B: HTTP cursor must not drop it (cursor={cursor}, seen={seen:?})"
    );
    assert!(seen.contains(&id_b.to_string()));
}

#[tokio::test]
async fn delta_pagination_splits_cleanly_at_page_boundary() {
    let server = TestServer::start().await;
    let folder = seed_folder(&server).await;
    let library_uuid: uuid::Uuid =
        sqlx::query_scalar("SELECT library_id FROM folders WHERE id = $1")
            .bind(folder.as_uuid())
            .fetch_one(server.db.pool())
            .await
            .expect("library_id");

    let total = CHANGE_LOG_PAGE + 1;
    sqlx::query(
        "INSERT INTO change_log (entity, entity_id, op, library_id) \
         SELECT 'asset', gen_random_uuid(), 'upsert', $1 \
           FROM generate_series(1, $2)",
    )
    .bind(library_uuid)
    .bind(i64::try_from(total).unwrap())
    .execute(server.db.pool())
    .await
    .expect("seed page+1 change_log rows");

    let mut cursor = 0_i64;
    let mut seen = Vec::<String>::new();
    let mut pages = 0_u32;
    let mut saw_full_page = false;
    loop {
        pages += 1;
        assert!(pages <= 5, "pagination must terminate");
        let page = server
            .client
            .get(server.url(&format!("/api/v1/sync/delta?cursor={cursor}")))
            .send()
            .await
            .expect("page")
            .json::<serde_json::Value>()
            .await
            .expect("json");
        let ids = upserted_ids(&page);
        if ids.len() == CHANGE_LOG_PAGE {
            saw_full_page = true;
        }
        for id in ids {
            if !seen.contains(&id) {
                seen.push(id);
            }
        }
        let next = page["cursor"].as_i64().expect("cursor");
        if page["has_more"] == true {
            assert!(
                next > cursor,
                "has_more requires a strictly advancing cursor (was {cursor}, got {next})"
            );
        }
        cursor = next;
        if page["has_more"] != true {
            break;
        }
    }

    assert!(
        saw_full_page,
        "with PAGE+1 rows the HTTP endpoint must return at least one full page"
    );
    assert!(
        pages >= 2,
        "PAGE+1 rows must span more than one HTTP page, got {pages}"
    );
    assert_eq!(
        seen.len(),
        total,
        "walking cursors must neither drop nor permanently lose rows"
    );
}

async fn seed_asset(server: &TestServer) -> keeppix_domain::AssetId {
    let folder = seed_folder(server).await;
    AssetRepo::new(&server.db)
        .upsert_discovered(NewAsset {
            folder_id: folder,
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

async fn seed_folder(server: &TestServer) -> keeppix_domain::FolderId {
    setup_admin(server).await;
    let root = server
        .photos_root
        .join(format!("sync-{}", uuid::Uuid::now_v7().simple()));
    std::fs::create_dir_all(&root).expect("library dir");
    let library_id = create_library(server, "sync-lib", &root).await;
    let library = library_id.parse().expect("library uuid");
    keeppix_db::FolderRepo::new(&server.db)
        .ensure_path(library, &[])
        .await
        .expect("root folder")
        .id
}

async fn insert_asset_on(
    conn: &mut PgConnection,
    folder: keeppix_domain::FolderId,
    name: &str,
) -> AssetId {
    let id = AssetId::new();
    sqlx::query(
        "INSERT INTO assets (id, folder_id, filename, size_bytes, mtime) \
         VALUES ($1, $2, $3, 100, now())",
    )
    .bind(id.as_uuid())
    .bind(folder.as_uuid())
    .bind(name)
    .execute(&mut *conn)
    .await
    .expect("insert asset");
    id
}

fn upserted_ids(body: &serde_json::Value) -> Vec<String> {
    body["upserted"]
        .as_array()
        .expect("upserted")
        .iter()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect()
}
