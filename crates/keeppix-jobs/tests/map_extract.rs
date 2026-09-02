mod harness;

use harness::TestDb;
use keeppix_db::RegionRepo;
use keeppix_domain::{AuthContext, JobKind, JobPriority, JobStatus, SystemRole};

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn enqueue_extraction_rejects_an_unknown_catalog_id() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);

    let result = keeppix_jobs::map_extract::enqueue_extraction(test.db(), &ctx, "atlantis").await;

    assert!(matches!(
        result,
        Err(keeppix_jobs::map_extract::MapExtractError::UnknownCatalogId(id)) if id == "atlantis"
    ));
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn enqueue_extraction_creates_an_extract_job_with_its_own_dedup_lane() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);

    let job = keeppix_jobs::map_extract::enqueue_extraction(test.db(), &ctx, "france")
        .await
        .unwrap();

    assert_eq!(job.kind, JobKind::ExtractMapRegion);
    let generation = job.payload["download_generation"].as_str().unwrap();
    assert_eq!(
        job.dedup_key.as_deref(),
        Some(format!("map-region-extract:france:{generation}").as_str())
    );

    let region = RegionRepo::new(test.db())
        .find(&ctx, "france")
        .await
        .unwrap();
    assert_eq!(region.status, keeppix_db::RegionStatus::Downloading);

    // A second call for the same country must not create a competing writer.
    let second = keeppix_jobs::map_extract::enqueue_extraction(test.db(), &ctx, "france").await;
    assert!(second.is_err());
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn repair_reenqueues_an_extraction_row_as_extract_not_download() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    // Simulates a crash right after `begin_extraction` committed but before
    // the job row was ever inserted — the exact case
    // `enqueue_missing_region_downloads` exists to repair.
    RegionRepo::new(test.db())
        .begin_extraction(
            &ctx,
            keeppix_db::NewMapRegion {
                id: "germany".to_owned(),
                label: "Germania".to_owned(),
                size_bytes: 520_000_000,
                version: "pending".to_owned(),
                source_url: "https://build.protomaps.com/pending".to_owned(),
                checksum_sha256: "0".repeat(64),
            },
        )
        .await
        .unwrap();

    let repaired = keeppix_jobs::regions::repair_interrupted_downloads(test.db())
        .await
        .unwrap();

    assert_eq!(repaired.reenqueued, 1);
    let job = keeppix_db::JobRepo::new(test.db())
        .claim(uuid::Uuid::now_v7(), JobPriority::High)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(job.status, JobStatus::Running);
    assert_eq!(job.kind, JobKind::ExtractMapRegion);
    assert_eq!(job.payload["region_id"], "germany");
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn startup_recovery_resets_a_running_extraction_job_too() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let queued = keeppix_jobs::map_extract::enqueue_extraction(test.db(), &ctx, "france")
        .await
        .unwrap();
    let jobs = keeppix_db::JobRepo::new(test.db());
    let running = jobs
        .claim(uuid::Uuid::now_v7(), JobPriority::High)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(running.id, queued.id);

    let recovered = keeppix_jobs::regions::recover_interrupted_downloads(test.db())
        .await
        .unwrap();

    assert_eq!(recovered.reaped, 1);
    assert_eq!(recovered.reenqueued, 0);
    let runnable = jobs
        .claim(uuid::Uuid::now_v7(), JobPriority::High)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(runnable.id, queued.id);
    assert_eq!(runnable.kind, JobKind::ExtractMapRegion);
}
