mod harness;

use harness::TestDb;
use keeppix_db::NewMapRegion;
use keeppix_domain::{AuthContext, SystemRole};

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn double_enqueue_creates_only_one_region_writer() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let region = || NewMapRegion {
        id: "IT".to_owned(),
        label: "Italia".to_owned(),
        size_bytes: 712_000_000,
        version: "2026-08".to_owned(),
        source_url: "https://build.protomaps.com/20260818.pmtiles".to_owned(),
        checksum_sha256: "ab".repeat(32),
    };

    let first = keeppix_jobs::regions::enqueue_download(test.db(), &ctx, region())
        .await
        .unwrap();
    let second = keeppix_jobs::regions::enqueue_download(test.db(), &ctx, region()).await;

    assert!(second.is_err());
    assert_eq!(
        keeppix_db::JobRepo::new(test.db())
            .count_for_dedup_key("map-region:IT")
            .await
            .unwrap(),
        1
    );
    assert_eq!(first.dedup_key.as_deref(), Some("map-region:IT"));
}
