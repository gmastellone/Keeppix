mod harness;

use harness::TestDb;
use keeppix_db::{NewMapRegion, RegionRepo, RegionStatus};
use keeppix_domain::{AuthContext, SystemRole};

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn region_lifecycle_persists_progress_and_errors() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let repo = RegionRepo::new(test.db());

    let created = repo
        .begin_download(
            &ctx,
            NewMapRegion {
                id: "IT".to_owned(),
                label: "Italia".to_owned(),
                size_bytes: 712_000_000,
                version: "2026-08".to_owned(),
                source_url: "https://build.protomaps.com/it.pmtiles".to_owned(),
                checksum_sha256: "ab".repeat(32),
            },
        )
        .await
        .unwrap();
    assert_eq!(created.status, RegionStatus::Downloading);
    assert_eq!(created.downloaded_bytes, 0);

    repo.record_progress("IT", created.download_generation, 1_048_576)
        .await
        .unwrap();
    repo.mark_error("IT", created.download_generation, "disk full")
        .await
        .unwrap();

    let failed = repo.find(&ctx, "IT").await.unwrap();
    assert_eq!(failed.status, RegionStatus::Error);
    assert_eq!(failed.downloaded_bytes, 0);
    assert_eq!(failed.last_error.as_deref(), Some("disk full"));
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn mark_error_requires_the_current_uncancelled_download() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let repo = RegionRepo::new(test.db());
    let region = || NewMapRegion {
        id: "IT".to_owned(),
        label: "Italia".to_owned(),
        size_bytes: 712_000_000,
        version: "2026-08".to_owned(),
        source_url: "https://build.protomaps.com/it.pmtiles".to_owned(),
        checksum_sha256: "ab".repeat(32),
    };
    let old = repo.begin_download(&ctx, region()).await.unwrap();
    repo.request_cancel(&ctx, "IT").await.unwrap();

    assert!(
        !repo
            .mark_error("IT", old.download_generation, "cancel race")
            .await
            .unwrap()
    );
    assert_eq!(
        repo.find(&ctx, "IT").await.unwrap().status,
        RegionStatus::Downloading
    );

    repo.finish_cancel("IT", old.download_generation)
        .await
        .unwrap();
    let current = repo.begin_download(&ctx, region()).await.unwrap();
    assert!(
        !repo
            .mark_error("IT", old.download_generation, "stale worker")
            .await
            .unwrap()
    );
    let unchanged = repo.find(&ctx, "IT").await.unwrap();
    assert_eq!(unchanged.status, RegionStatus::Downloading);
    assert_eq!(unchanged.download_generation, current.download_generation);
    assert!(unchanged.last_error.is_none());
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn begin_extraction_records_actuals_only_on_completion() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let repo = RegionRepo::new(test.db());

    let created = repo
        .begin_extraction(
            &ctx,
            NewMapRegion {
                id: "france".to_owned(),
                label: "Francia".to_owned(),
                size_bytes: 480_000_000,
                version: "pending".to_owned(),
                source_url: "https://build.protomaps.com/pending".to_owned(),
                checksum_sha256: "0".repeat(64),
            },
        )
        .await
        .unwrap();
    assert_eq!(created.status, RegionStatus::Downloading);

    let updated = repo
        .mark_available_with_actuals(
            "france",
            created.download_generation,
            123_456,
            &"cd".repeat(32),
            "https://build.protomaps.com/20260901.pmtiles",
        )
        .await
        .unwrap();
    assert!(updated);

    let region = repo.find(&ctx, "france").await.unwrap();
    assert_eq!(region.status, RegionStatus::Available);
    assert_eq!(region.size_bytes, 123_456);
    assert_eq!(region.downloaded_bytes, 123_456);
    assert_eq!(region.checksum_sha256, "cd".repeat(32));
    assert_eq!(
        region.source_url,
        "https://build.protomaps.com/20260901.pmtiles"
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn non_admin_cannot_mutate_global_regions() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let user = harness::seed_user(&test, admin, "region-user").await;
    let ctx = AuthContext::user(user, SystemRole::User);

    let result = RegionRepo::new(test.db())
        .begin_download(
            &ctx,
            NewMapRegion {
                id: "IT".to_owned(),
                label: "Italia".to_owned(),
                size_bytes: 1,
                version: "2026-08".to_owned(),
                source_url: "https://build.protomaps.com/it.pmtiles".to_owned(),
                checksum_sha256: "ab".repeat(32),
            },
        )
        .await;

    assert!(matches!(result, Err(keeppix_db::DbError::Forbidden)));
}
