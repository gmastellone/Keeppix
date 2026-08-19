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
    let dedup_key = first.dedup_key.as_deref().unwrap();
    let second = keeppix_jobs::regions::enqueue_download(test.db(), &ctx, region()).await;

    assert!(second.is_err());
    assert_eq!(
        keeppix_db::JobRepo::new(test.db())
            .count_for_dedup_key(dedup_key)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        dedup_key,
        format!(
            "map-region:IT:{}",
            first.payload["download_generation"].as_str().unwrap()
        )
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn old_worker_cannot_finalize_after_a_new_region_job_is_stored() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let region = || NewMapRegion {
        id: "IT".to_owned(),
        label: "Italia".to_owned(),
        size_bytes: 11,
        version: "2026-08".to_owned(),
        source_url: "https://build.protomaps.com/20260818.pmtiles".to_owned(),
        checksum_sha256: "ab".repeat(32),
    };
    let old_job = keeppix_jobs::regions::enqueue_download(test.db(), &ctx, region())
        .await
        .unwrap();
    let old_generation =
        uuid::Uuid::parse_str(old_job.payload["download_generation"].as_str().unwrap()).unwrap();
    let regions = RegionRepo::new(test.db());
    regions.request_cancel(&ctx, "IT").await.unwrap();
    JobRepo::new(test.db())
        .retire_active(old_job.dedup_key.as_deref().unwrap(), "Download cancelled")
        .await
        .unwrap();
    regions.finish_cancel("IT", old_generation).await.unwrap();
    let new_job = keeppix_jobs::regions::enqueue_download(test.db(), &ctx, region())
        .await
        .unwrap();
    assert_ne!(old_job.id, new_job.id);
    assert_ne!(old_job.payload["file_path"], new_job.payload["file_path"]);

    assert!(!regions.mark_available("IT", old_generation).await.unwrap());
    assert!(
        !regions
            .mark_error("IT", old_generation, "old worker failed")
            .await
            .unwrap()
    );
    let region = regions.find(&ctx, "IT").await.unwrap();
    assert_eq!(region.status, keeppix_db::RegionStatus::Downloading);
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn cancelled_region_with_running_old_job_enqueues_the_new_generation() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let region = || NewMapRegion {
        id: "IT".to_owned(),
        label: "Italia".to_owned(),
        size_bytes: 11,
        version: "2026-08".to_owned(),
        source_url: "https://build.protomaps.com/20260818.pmtiles".to_owned(),
        checksum_sha256: "ab".repeat(32),
    };
    let old_job = keeppix_jobs::regions::enqueue_download(test.db(), &ctx, region())
        .await
        .unwrap();
    let old_generation = old_job.payload["download_generation"].as_str().unwrap();
    let jobs = JobRepo::new(test.db());
    let running = jobs
        .claim(uuid::Uuid::now_v7(), JobPriority::High)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(running.id, old_job.id);
    let regions = RegionRepo::new(test.db());
    regions.request_cancel(&ctx, "IT").await.unwrap();
    regions
        .finish_cancel("IT", uuid::Uuid::parse_str(old_generation).unwrap())
        .await
        .unwrap();

    let new_job = keeppix_jobs::regions::enqueue_download(test.db(), &ctx, region())
        .await
        .unwrap();
    let current_generation = regions
        .find(&ctx, "IT")
        .await
        .unwrap()
        .download_generation
        .to_string();

    assert_ne!(new_job.id, old_job.id);
    assert_ne!(new_job.payload["download_generation"], old_generation);
    assert_eq!(
        new_job.payload["download_generation"].as_str(),
        Some(current_generation.as_str())
    );
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
    let generation =
        uuid::Uuid::parse_str(job.payload["download_generation"].as_str().unwrap()).unwrap();
    RegionRepo::new(test.db())
        .record_progress("IT", generation, 5)
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
    assert_eq!(
        jobs.count_for_dedup_key(job.dedup_key.as_deref().unwrap())
            .await
            .unwrap(),
        1
    );
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

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn startup_recovery_resets_a_running_region_job_immediately() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let queued = keeppix_jobs::regions::enqueue_download(
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
}
