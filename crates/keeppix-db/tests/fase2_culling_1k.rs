//! Batch culling on >=1000 assets (path flags) must stay under an
//! interactive latency threshold. Files on disk and RAW hashing live in
//! `keeppix-media` (the db knows nothing about the images — `deny.toml`).

mod harness;

use std::time::Instant;

use chrono::{TimeZone, Utc};
use harness::TestDb;
use keeppix_db::{FlagRepo, FolderRepo, LibraryRepo};
use keeppix_domain::{
    AssetFlags, AssetId, AuthContext, LibraryId, NewLibrary, Pick, Rating, SystemRole, UserId,
};

const N: usize = 1000;

#[allow(clippy::expect_used)]
async fn seed_library(test: &TestDb, owner: UserId) -> LibraryId {
    LibraryRepo::new(test.db())
        .create(
            &AuthContext::user(owner, SystemRole::Admin),
            NewLibrary {
                name: "Culling 1k".to_owned(),
                owner_id: owner,
                root_path: std::path::PathBuf::from("/mnt/foto-1k"),
                exclude_patterns: vec![],
            },
        )
        .await
        .expect("library")
        .id
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn batch_flags_on_one_thousand_assets_stays_interactive() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let library = seed_library(&test, admin).await;
    let folder = FolderRepo::new(test.db())
        .ensure_path(library, &["2024"])
        .await
        .unwrap();

    let ids: Vec<uuid::Uuid> = (0..N).map(|_| AssetId::new().as_uuid()).collect();
    let filenames: Vec<String> = (0..N).map(|i| format!("IMG_{i:05}.jpg")).collect();
    let mtime = Utc.with_ymd_and_hms(2024, 6, 1, 12, 0, 0).unwrap();
    sqlx::query(
        "INSERT INTO assets (id, folder_id, filename, size_bytes, mtime, kind, status) \
         SELECT aid, $2, name, 200000, $3, 'image', 'indexed' \
           FROM unnest($1::uuid[], $4::text[]) AS t(aid, name)",
    )
    .bind(&ids)
    .bind(folder.id.as_uuid())
    .bind(mtime)
    .bind(&filenames)
    .execute(test.db().pool())
    .await
    .unwrap();

    let asset_ids: Vec<AssetId> = ids.into_iter().map(AssetId::from_uuid).collect();
    let flags = AssetFlags {
        rating: Some(Rating::parse(4).unwrap()),
        pick: Pick::Pick,
        color_label: None,
        favorite: false,
    };

    let started = Instant::now();
    FlagRepo::new(test.db())
        .batch_set(&ctx, &asset_ids, &flags)
        .await
        .unwrap();
    let flag_elapsed = started.elapsed();
    eprintln!("MEASUREMENT batch_set flags on {N} assets: {flag_elapsed:?}");
    assert!(
        flag_elapsed.as_secs() < 3,
        "batch flags on {N} assets must stay interactive, took {flag_elapsed:?}"
    );

    let counted: i64 =
        sqlx::query_scalar("SELECT count(*) FROM asset_flags WHERE pick = 'pick' AND rating = 4")
            .fetch_one(test.db().pool())
            .await
            .unwrap();
    assert_eq!(counted, i64::try_from(N).unwrap());
}
