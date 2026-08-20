//! Task 11: la sidebar non deve pagare aggregati per riga (cartelle, album,
//! link). Il conteggio del culling resta altrove (per lotto, non qui).

mod harness;

use chrono::{TimeZone, Utc};
use harness::TestDb;
use keeppix_db::{
    AlbumRepo, AssetRepo, FolderRepo, LibraryRepo, NewAlbum, NewShareLink, ShareLinkRepo,
};
use keeppix_domain::{
    AssetKind, AssetName, AuthContext, FolderId, LibraryId, NewAsset, NewLibrary, SystemRole,
    UserId,
};

#[allow(clippy::expect_used)]
async fn seed_admin(test: &TestDb) -> UserId {
    harness::seed_admin(test).await
}

#[allow(clippy::expect_used)]
async fn seed_library(test: &TestDb, owner: UserId) -> LibraryId {
    let ctx = AuthContext::user(owner, SystemRole::Admin);
    LibraryRepo::new(test.db())
        .create(
            &ctx,
            NewLibrary {
                name: "Foto".to_owned(),
                owner_id: owner,
                root_path: std::path::PathBuf::from("/mnt/foto"),
                exclude_patterns: vec![],
            },
        )
        .await
        .expect("libreria")
        .id
}

#[allow(clippy::expect_used)]
async fn seed_folder_tree(test: &TestDb, library: LibraryId) -> FolderId {
    FolderRepo::new(test.db())
        .ensure_path(library, &["2024", "Urbino", "Centro"])
        .await
        .expect("albero")
        .id
}

#[allow(clippy::expect_used)]
async fn seed_indexed_asset(test: &TestDb, folder: FolderId, filename: &str) {
    let repo = AssetRepo::new(test.db());
    let asset = repo
        .upsert_discovered(NewAsset {
            folder_id: folder,
            filename: AssetName::parse(filename).expect("nome"),
            size_bytes: 10,
            mtime: Utc.with_ymd_and_hms(2024, 7, 1, 0, 0, 0).unwrap(),
            inode: None,
            kind: AssetKind::Image,
        })
        .await
        .expect("discovered")
        .expect("nuovo asset");
    repo.set_indexed(
        asset.id,
        Utc.with_ymd_and_hms(2024, 7, 2, 12, 0, 0).unwrap(),
        1,
        1,
    )
    .await
    .expect("indexed");
}

fn is_per_row_aggregation_query(sql: &str) -> bool {
    let normalized = sql.to_ascii_lowercase();
    normalized.contains("count(")
        || normalized.contains("group by")
        || normalized.contains("folder_month_counts")
        || (normalized.contains("sum(") && normalized.contains("asset_count"))
}

#[allow(clippy::expect_used)]
async fn load_sidebar_repos(test: &TestDb, ctx: &AuthContext) {
    FolderRepo::new(test.db()).roots(ctx).await.expect("roots");
    FolderRepo::new(test.db()).tree(ctx).await.expect("tree");
    AlbumRepo::new(test.db()).list(ctx).await.expect("albums");
    ShareLinkRepo::new(test.db())
        .list_by_creator(ctx, 100, 0)
        .await
        .expect("links");
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn sidebar_load_emits_no_per_row_aggregation_queries() {
    let (test, captured, _guard) = TestDb::start_traced().await;
    let admin = seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let library = seed_library(&test, admin).await;
    let folder = seed_folder_tree(&test, library).await;
    seed_indexed_asset(&test, folder, "a.jpg").await;
    seed_indexed_asset(&test, folder, "b.jpg").await;

    AlbumRepo::new(test.db())
        .create(
            &ctx,
            NewAlbum {
                name: "Viaggio".to_owned(),
                description: String::new(),
                rule: None,
            },
        )
        .await
        .expect("album");

    ShareLinkRepo::new(test.db())
        .create(
            &ctx,
            NewShareLink {
                object_type: "folder".to_owned(),
                object_id: folder.as_uuid(),
                password_hash: None,
                expires_at: None,
                max_views: None,
                allow_download: true,
                allow_original: false,
                allow_upload: false,
                allow_cdn_cache: false,
                hide_metadata: true,
                upload_quota_bytes: None,
            },
        )
        .await
        .expect("link");

    captured.lock().expect("lock").clear();
    load_sidebar_repos(&test, &ctx).await;
    let queries = captured.lock().expect("lock").clone();

    assert!(
        !queries.is_empty(),
        "il test deve catturare almeno una query sqlx del caricamento sidebar"
    );
    for sql in queries {
        assert!(
            !is_per_row_aggregation_query(&sql),
            "aggregato per riga vietato dal Task 11: {sql}"
        );
    }
}
