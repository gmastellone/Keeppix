mod harness;

use harness::TestDb;
use keeppix_db::{JobRepo, NewMapRegion, RegionRepo};
use keeppix_domain::{AuthContext, JobPriority, JobStatus, SystemRole};

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

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn repair_reenqueues_a_downloading_region_without_a_live_job() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    RegionRepo::new(test.db())
        .begin_download(
            &ctx,
            NewMapRegion {
                id: "IT".to_owned(),
                label: "Italia".to_owned(),
                size_bytes: 11,
                version: "2026-08".to_owned(),
                source_url: "https://build.protomaps.com/20260818.pmtiles".to_owned(),
                checksum_sha256: "ab".repeat(32),
            },
        )
        .await
        .unwrap();

    let repaired = keeppix_jobs::regions::repair_interrupted_downloads(test.db())
        .await
        .unwrap();

    assert_eq!(repaired.reenqueued, 1);
    let job = JobRepo::new(test.db())
        .claim(uuid::Uuid::now_v7(), JobPriority::High)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(job.status, JobStatus::Running);
    assert_eq!(job.payload["region_id"], "IT");
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn repair_reaps_a_claimed_region_job_without_duplicating_it() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let job = keeppix_jobs::regions::enqueue_download(
        test.db(),
        &ctx,
        NewMapRegion {
            id: "IT".to_owned(),
            label: "Italia".to_owned(),
            size_bytes: 11,
            version: "2026-08".to_owned(),
            source_url: "https://build.protomaps.com/20260818.pmtiles".to_owned(),
            checksum_sha256: "ab".repeat(32),
        },
    )
    .await
    .unwrap();
    let jobs = JobRepo::new(test.db());
    jobs.claim(uuid::Uuid::now_v7(), JobPriority::High)
        .await
        .unwrap()
        .unwrap();
    RegionRepo::new(test.db())
        .record_progress("IT", 5)
        .await
        .unwrap();
    sqlx::query("UPDATE jobs SET locked_at = now() - interval '20 minutes' WHERE id = $1")
        .bind(job.id)
        .execute(test.db().pool())
        .await
        .unwrap();

    let repaired = keeppix_jobs::regions::repair_interrupted_downloads(test.db())
        .await
        .unwrap();

    assert_eq!(repaired.reaped, 1);
    assert_eq!(repaired.reenqueued, 0);
    assert_eq!(jobs.count_for_dedup_key("map-region:IT").await.unwrap(), 1);
    let resumed = jobs
        .claim(uuid::Uuid::now_v7(), JobPriority::High)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(resumed.id, job.id);
    assert_eq!(
        RegionRepo::new(test.db())
            .find(&ctx, "IT")
            .await
            .unwrap()
            .downloaded_bytes,
        5
    );
}
