mod harness;

use harness::TestDb;
use keeppix_db::JobRepo;
use keeppix_domain::{AuthContext, JobKind, JobPriority, JobStatus, SystemRole};
use serde_json::json;
use uuid::Uuid;

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn enqueue_is_idempotent_on_dedup_key() {
    let test = TestDb::start().await;
    let repo = JobRepo::new(test.db());

    let first = repo
        .enqueue(
            JobKind::HashAsset,
            json!({"asset_id": "a"}),
            JobPriority::Background,
            Some("hash:a"),
        )
        .await
        .unwrap();
    let second = repo
        .enqueue(
            JobKind::HashAsset,
            json!({"asset_id": "a"}),
            JobPriority::Background,
            Some("hash:a"),
        )
        .await
        .unwrap();

    assert_eq!(first.id, second.id);
}

/// A single network round trip for a whole batch instead of one per file.
#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn enqueue_many_inserts_every_item() {
    let test = TestDb::start().await;
    let repo = JobRepo::new(test.db());

    let items = vec![
        (json!({"asset_id": "a"}), "meta:a".to_owned()),
        (json!({"asset_id": "b"}), "meta:b".to_owned()),
        (json!({"asset_id": "c"}), "meta:c".to_owned()),
    ];
    repo.enqueue_many(JobKind::ExtractMetadata, &items, JobPriority::Background)
        .await
        .unwrap();

    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM jobs WHERE kind = 'extract_metadata' AND status = 'pending'",
    )
    .fetch_one(test.db().pool())
    .await
    .unwrap();
    assert_eq!(count, 3);
}

/// Like `enqueue`: a `dedup_key` already queued as `pending`/`running` must
/// not produce a second row, not even within the same batch.
#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn enqueue_many_respects_dedup_key_against_existing_rows() {
    let test = TestDb::start().await;
    let repo = JobRepo::new(test.db());
    repo.enqueue(
        JobKind::ExtractMetadata,
        json!({"asset_id": "a"}),
        JobPriority::Background,
        Some("meta:a"),
    )
    .await
    .unwrap();

    let items = vec![
        (json!({"asset_id": "a"}), "meta:a".to_owned()),
        (json!({"asset_id": "b"}), "meta:b".to_owned()),
    ];
    repo.enqueue_many(JobKind::ExtractMetadata, &items, JobPriority::Background)
        .await
        .unwrap();

    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM jobs WHERE kind = 'extract_metadata'")
            .fetch_one(test.db().pool())
            .await
            .unwrap();
    assert_eq!(count, 2, "meta:a non deve duplicarsi");
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn claim_skips_a_locked_row() {
    let test = TestDb::start().await;
    let repo = JobRepo::new(test.db());
    repo.enqueue(
        JobKind::HashAsset,
        json!({}),
        JobPriority::Background,
        Some("hash:1"),
    )
    .await
    .unwrap();
    repo.enqueue(
        JobKind::HashAsset,
        json!({}),
        JobPriority::Background,
        Some("hash:2"),
    )
    .await
    .unwrap();

    let (a, b) = tokio::join!(
        repo.claim(Uuid::now_v7(), JobPriority::Background),
        repo.claim(Uuid::now_v7(), JobPriority::Background),
    );
    let a = a.unwrap().expect("first claim");
    let b = b.unwrap().expect("second claim");
    assert_ne!(a.id, b.id, "SKIP LOCKED deve dare due job distinti");
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn claim_respects_max_priority() {
    let test = TestDb::start().await;
    let repo = JobRepo::new(test.db());
    repo.enqueue(
        JobKind::DeriveAsset,
        json!({}),
        JobPriority::Background,
        Some("derive:x"),
    )
    .await
    .unwrap();

    let claimed = repo
        .claim(Uuid::now_v7(), JobPriority::Visible)
        .await
        .unwrap();
    assert!(
        claimed.is_none(),
        "un profilo interactive non prende i job di background"
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn claim_orders_by_priority_then_run_after() {
    let test = TestDb::start().await;
    let repo = JobRepo::new(test.db());
    repo.enqueue(
        JobKind::HashAsset,
        json!({"n": 1}),
        JobPriority::Background,
        Some("hash:late"),
    )
    .await
    .unwrap();
    repo.enqueue(
        JobKind::ExtractMetadata,
        json!({"n": 2}),
        JobPriority::Interactive,
        Some("meta:now"),
    )
    .await
    .unwrap();

    let claimed = repo
        .claim(Uuid::now_v7(), JobPriority::Background)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(claimed.kind, JobKind::ExtractMetadata);
    assert_eq!(claimed.priority, JobPriority::Interactive);
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn fail_retries_then_exhausts() {
    let test = TestDb::start().await;
    let repo = JobRepo::new(test.db());
    let job = repo
        .enqueue(
            JobKind::HashAsset,
            json!({}),
            JobPriority::High,
            Some("hash:retry"),
        )
        .await
        .unwrap();

    for attempt in 1..=2 {
        let claimed = repo
            .claim(Uuid::now_v7(), JobPriority::High)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(claimed.id, job.id);
        assert_eq!(claimed.attempts, attempt);
        repo.fail(claimed.id, "disk missing").await.unwrap();
        sqlx::query("UPDATE jobs SET run_after = now() WHERE id = $1")
            .bind(claimed.id)
            .execute(test.db().pool())
            .await
            .unwrap();
    }

    let last = repo
        .claim(Uuid::now_v7(), JobPriority::High)
        .await
        .unwrap()
        .unwrap();
    repo.fail(last.id, "disk missing").await.unwrap();

    let status: String = sqlx::query_scalar("SELECT status FROM jobs WHERE id = $1")
        .bind(job.id)
        .fetch_one(test.db().pool())
        .await
        .unwrap();
    assert_eq!(status, "failed");
    assert!(
        repo.claim(Uuid::now_v7(), JobPriority::High)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn reap_stale_returns_a_running_job_to_pending() {
    let test = TestDb::start().await;
    let repo = JobRepo::new(test.db());
    let job = repo
        .enqueue(
            JobKind::ReapStale,
            json!({}),
            JobPriority::Background,
            Some("reap:self"),
        )
        .await
        .unwrap();
    repo.claim(Uuid::now_v7(), JobPriority::Background)
        .await
        .unwrap();

    sqlx::query("UPDATE jobs SET locked_at = now() - interval '20 minutes' WHERE id = $1")
        .bind(job.id)
        .execute(test.db().pool())
        .await
        .unwrap();

    let n = repo
        .reap_stale(std::time::Duration::from_secs(600))
        .await
        .unwrap();
    assert_eq!(n, 1);

    let again = repo
        .claim(Uuid::now_v7(), JobPriority::Background)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(again.id, job.id);
    assert_eq!(again.status, JobStatus::Running);
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn renewing_a_running_job_prevents_stale_reaping() {
    let test = TestDb::start().await;
    let repo = JobRepo::new(test.db());
    let generation = Uuid::now_v7();
    let dedup_key = format!("map-region:IT:{generation}");
    let job = repo
        .enqueue(
            JobKind::DownloadMapRegion,
            json!({"region_id": "IT", "download_generation": generation}),
            JobPriority::High,
            Some(&dedup_key),
        )
        .await
        .unwrap();
    let worker = Uuid::now_v7();
    repo.claim(worker, JobPriority::High)
        .await
        .unwrap()
        .unwrap();
    sqlx::query("UPDATE jobs SET locked_at = now() - interval '20 minutes' WHERE id = $1")
        .bind(job.id)
        .execute(test.db().pool())
        .await
        .unwrap();

    assert!(repo.renew_lock(job.id, worker).await.unwrap());
    assert_eq!(
        repo.reap_stale(std::time::Duration::from_secs(600))
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn promote_raises_pending_jobs() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let repo = JobRepo::new(test.db());
    repo.enqueue(
        JobKind::DeriveAsset,
        json!({}),
        JobPriority::Background,
        Some("derive:photo"),
    )
    .await
    .unwrap();

    let n = repo
        .promote(&ctx, &["derive:photo".to_owned()], JobPriority::Visible)
        .await
        .unwrap();
    assert_eq!(n, 1);

    let claimed = repo
        .claim(Uuid::now_v7(), JobPriority::Visible)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(claimed.priority, JobPriority::Visible);
}

/// `asset.derivative.ready` on the WebSocket reads from here — only jobs
/// that became `done` **after** the cursor, never ones that were already
/// `done` before the cursor was fixed (otherwise a client connecting now
/// would resurface transcodes finished hours earlier).
#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn list_recently_done_skips_jobs_finished_before_the_cursor() {
    let test = TestDb::start().await;
    let repo = JobRepo::new(test.db());
    let old = repo
        .enqueue(
            JobKind::TranscodeVideo,
            json!({"asset_id": "old"}),
            JobPriority::Background,
            Some("transcode:old"),
        )
        .await
        .unwrap();
    let claimed = repo
        .claim(Uuid::now_v7(), JobPriority::Background)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(claimed.id, old.id);
    repo.complete(old.id).await.unwrap();

    let cursor = repo.max_done_id(JobKind::TranscodeVideo).await.unwrap();
    assert_eq!(cursor, old.id);

    let fresh = repo
        .enqueue(
            JobKind::TranscodeVideo,
            json!({"asset_id": "fresh"}),
            JobPriority::Background,
            Some("transcode:fresh"),
        )
        .await
        .unwrap();
    repo.claim(Uuid::now_v7(), JobPriority::Background)
        .await
        .unwrap();
    repo.complete(fresh.id).await.unwrap();

    let recent = repo
        .list_recently_done(JobKind::TranscodeVideo, cursor, 50)
        .await
        .unwrap();
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].id, fresh.id);
    assert_eq!(recent[0].payload["asset_id"], "fresh");
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn promote_does_not_raise_jobs_outside_visibility() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let mario = harness::seed_user(&test, admin, "mario").await;
    let admin_ctx = AuthContext::user(admin, SystemRole::Admin);
    let mario_ctx = AuthContext::user(mario, SystemRole::User);

    let library = keeppix_db::LibraryRepo::new(test.db())
        .create(
            &admin_ctx,
            keeppix_domain::NewLibrary {
                name: "Foto".to_owned(),
                owner_id: admin,
                root_path: std::path::PathBuf::from("/mnt/foto"),
                exclude_patterns: vec![],
            },
        )
        .await
        .unwrap();
    let folder = keeppix_db::FolderRepo::new(test.db())
        .ensure_path(library.id, &[])
        .await
        .unwrap();
    let hash = [0xab_u8; 32];
    let hex = "ab".repeat(32);
    let asset = keeppix_db::AssetRepo::new(test.db())
        .upsert_discovered(keeppix_domain::NewAsset {
            folder_id: folder.id,
            filename: keeppix_domain::AssetName::parse("a.jpg").unwrap(),
            size_bytes: 10,
            mtime: chrono::TimeZone::with_ymd_and_hms(&chrono::Utc, 2024, 7, 1, 0, 0, 0).unwrap(),
            inode: Some(1),
            kind: keeppix_domain::AssetKind::Image,
        })
        .await
        .unwrap()
        .unwrap();
    keeppix_db::AssetRepo::new(test.db())
        .set_hash(asset.id, hash)
        .await
        .unwrap();

    let repo = JobRepo::new(test.db());
    repo.enqueue(
        JobKind::DeriveAsset,
        json!({}),
        JobPriority::Background,
        Some(&format!("derive:{hex}")),
    )
    .await
    .unwrap();

    let n = repo
        .promote(&mario_ctx, &[format!("derive:{hex}")], JobPriority::Visible)
        .await
        .unwrap();
    assert_eq!(n, 0);

    let n = repo
        .promote(&admin_ctx, &[format!("derive:{hex}")], JobPriority::Visible)
        .await
        .unwrap();
    assert_eq!(n, 1);
}
