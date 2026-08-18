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

    repo.record_progress("IT", 1_048_576).await.unwrap();
    repo.mark_error("IT", "disk full").await.unwrap();

    let failed = repo.find(&ctx, "IT").await.unwrap();
    assert_eq!(failed.status, RegionStatus::Error);
    assert_eq!(failed.downloaded_bytes, 0);
    assert_eq!(failed.last_error.as_deref(), Some("disk full"));
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
