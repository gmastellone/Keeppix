//! Task 15: `GET /share/links` (§29) needs an item count per link — a
//! `GROUP BY` for the whole list, not a `COUNT` per row (same rule as
//! Task 11's culling count).
#![allow(clippy::unwrap_used)]

mod harness;

use chrono::{TimeZone, Utc};
use harness::TestDb;
use keeppix_db::{
    AlbumRepo, AssetRepo, FolderRepo, LibraryRepo, NewAlbum, NewShareLink, ShareLinkRepo,
};
use keeppix_domain::{
    AssetKind, AssetName, AuthContext, FolderId, NewAsset, NewLibrary, SystemRole, UserId,
};

async fn seed_library(test: &TestDb, owner: UserId) -> keeppix_domain::LibraryId {
    LibraryRepo::new(test.db())
        .create(
            &AuthContext::user(owner, SystemRole::Admin),
            NewLibrary {
                name: "Foto".to_owned(),
                owner_id: owner,
                root_path: std::path::PathBuf::from("/mnt/nas/foto"),
                exclude_patterns: vec![],
            },
        )
        .await
        .unwrap()
        .id
}

async fn index_photo(test: &TestDb, folder: FolderId, name: &str) {
    let repo = AssetRepo::new(test.db());
    let asset = repo
        .upsert_discovered(NewAsset {
            folder_id: folder,
            filename: AssetName::parse(name).unwrap(),
            size_bytes: 10,
            mtime: Utc.with_ymd_and_hms(2024, 7, 1, 0, 0, 0).unwrap(),
            inode: None,
            kind: AssetKind::Image,
        })
        .await
        .unwrap()
        .unwrap();
    repo.set_indexed(
        asset.id,
        Utc.with_ymd_and_hms(2024, 7, 2, 12, 0, 0).unwrap(),
        1,
        1,
    )
    .await
    .unwrap();
}

fn folder_link(folder: FolderId) -> NewShareLink {
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
    }
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn item_counts_reports_indexed_assets_for_a_folder_link() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let library = seed_library(&test, admin).await;
    let folder = FolderRepo::new(test.db())
        .ensure_path(library, &["Vacanze"])
        .await
        .unwrap();
    index_photo(&test, folder.id, "a.jpg").await;
    index_photo(&test, folder.id, "b.jpg").await;
    // Discovered but not yet indexed: not visible on the public share link,
    // so it must not inflate the count either.
    AssetRepo::new(test.db())
        .upsert_discovered(NewAsset {
            folder_id: folder.id,
            filename: AssetName::parse("c.jpg").unwrap(),
            size_bytes: 10,
            mtime: Utc.with_ymd_and_hms(2024, 7, 1, 0, 0, 0).unwrap(),
            inode: None,
            kind: AssetKind::Image,
        })
        .await
        .unwrap();

    let links_repo = ShareLinkRepo::new(test.db());
    links_repo
        .create(&ctx, folder_link(folder.id))
        .await
        .unwrap();

    let links = links_repo.list_by_creator(&ctx, 100, 0).await.unwrap();
    let counts = links_repo.item_counts(&links).await.unwrap();

    assert_eq!(
        counts.get(&folder.id.as_uuid()),
        Some(&2),
        "un asset discovered-only non è ancora un elemento condivisibile"
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn item_counts_reports_album_members() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let library = seed_library(&test, admin).await;
    let folder = FolderRepo::new(test.db())
        .ensure_path(library, &["Vacanze"])
        .await
        .unwrap();
    index_photo(&test, folder.id, "a.jpg").await;
    let asset = AssetRepo::new(test.db())
        .find_by_folder(&ctx, folder.id)
        .await
        .unwrap()
        .remove(0);

    let album = AlbumRepo::new(test.db())
        .create(
            &ctx,
            NewAlbum {
                name: "Migliori scatti".to_owned(),
                description: String::new(),
                rule: None,
            },
        )
        .await
        .unwrap();
    AlbumRepo::new(test.db())
        .add_asset(&ctx, album.id, asset.id)
        .await
        .unwrap();

    let links_repo = ShareLinkRepo::new(test.db());
    links_repo
        .create(
            &ctx,
            NewShareLink {
                object_type: "album".to_owned(),
                object_id: album.id.as_uuid(),
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
        .unwrap();

    let links = links_repo.list_by_creator(&ctx, 100, 0).await.unwrap();
    let counts = links_repo.item_counts(&links).await.unwrap();

    assert_eq!(counts.get(&album.id.as_uuid()), Some(&1));
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn item_counts_treats_an_asset_link_as_exactly_one() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let library = seed_library(&test, admin).await;
    let folder = FolderRepo::new(test.db())
        .ensure_path(library, &["Vacanze"])
        .await
        .unwrap();
    index_photo(&test, folder.id, "a.jpg").await;
    let asset = AssetRepo::new(test.db())
        .find_by_folder(&ctx, folder.id)
        .await
        .unwrap()
        .remove(0);

    let links_repo = ShareLinkRepo::new(test.db());
    links_repo
        .create(
            &ctx,
            NewShareLink {
                object_type: "asset".to_owned(),
                object_id: asset.id.as_uuid(),
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
        .unwrap();

    let links = links_repo.list_by_creator(&ctx, 100, 0).await.unwrap();
    let counts = links_repo.item_counts(&links).await.unwrap();

    assert_eq!(counts.get(&asset.id.as_uuid()), Some(&1));
}

/// Un `count(*)` per riga rifatto in un ciclo scalerebbe a N query: la stessa
/// regola del Task 11 vale anche qui, solo applicata a un campo che quel
/// task aveva invece rimosso. Due link (una cartella con 2 foto, un'altra
/// vuota) devono costare due `GROUP BY`, non due `COUNT` singoli.
#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn item_counts_batches_multiple_folder_links_in_one_query() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let library = seed_library(&test, admin).await;
    let folders = FolderRepo::new(test.db());
    let full = folders.ensure_path(library, &["Piena"]).await.unwrap();
    let empty = folders.ensure_path(library, &["Vuota"]).await.unwrap();
    index_photo(&test, full.id, "a.jpg").await;
    index_photo(&test, full.id, "b.jpg").await;

    let links_repo = ShareLinkRepo::new(test.db());
    links_repo.create(&ctx, folder_link(full.id)).await.unwrap();
    links_repo
        .create(&ctx, folder_link(empty.id))
        .await
        .unwrap();

    let links = links_repo.list_by_creator(&ctx, 100, 0).await.unwrap();
    let counts = links_repo.item_counts(&links).await.unwrap();

    assert_eq!(counts.get(&full.id.as_uuid()), Some(&2));
    assert_eq!(
        counts.get(&empty.id.as_uuid()),
        None,
        "una cartella senza asset indicizzati non produce riga nel GROUP BY"
    );
}
