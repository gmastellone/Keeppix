#![allow(clippy::unwrap_used, clippy::expect_used)]

mod harness;

use std::fs;
use std::time::Duration;

use harness::TestDb;
use keeppix_db::OperationsRepo;
use keeppix_domain::{AuthContext, NewLibrary, OperationKind, OperationStatus, SystemRole};
use keeppix_jobs::discover;

async fn seed_library(test: &TestDb, root: &std::path::Path) -> keeppix_domain::Library {
    let admin = harness::seed_admin(test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    keeppix_db::LibraryRepo::new(test.db())
        .create(
            &ctx,
            NewLibrary {
                name: "Foto".to_owned(),
                owner_id: admin,
                root_path: root.to_path_buf(),
                exclude_patterns: vec![],
            },
        )
        .await
        .unwrap()
}

/// A complete pass, with no cancellation, must close the operation as
/// `Done` with the full list of successes: an `operation_id` that really
/// tracks the operation to the end.
#[tokio::test]
async fn a_completed_scan_marks_the_operation_done() {
    let test = TestDb::start().await;
    let root = std::env::temp_dir().join(format!("kpx-op-done-{}", uuid::Uuid::now_v7()));
    fs::create_dir_all(&root).unwrap();
    for n in 0..5 {
        fs::write(root.join(format!("{n}.jpg")), b"x").unwrap();
    }
    let library = seed_library(&test, &root).await;
    let admin = library.owner_id;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let ops = OperationsRepo::new(test.db());
    let op = ops.create(&ctx, OperationKind::LibraryScan).await.unwrap();

    discover::run_with_operation(test.db(), library.id, Duration::ZERO, Some(op.id))
        .await
        .unwrap();
    let _ = fs::remove_dir_all(&root);

    let seen = ops.find(&ctx, op.id).await.unwrap();
    assert_eq!(seen.status, OperationStatus::Done);
    assert_eq!(seen.done, 5);
    assert_eq!(seen.succeeded.len(), 5);
}

/// Ruling: cancelling midway produces a partial success, not a rollback.
/// Files already written stay; the operation lists them.
///
/// `TOTAL` must exceed `PRODUCTION_BATCH_SIZE`: since writes happen in
/// multi-row batches, the "midway" observability point is between two
/// batches, not between two files anymore — with fewer files than a full
/// batch, the whole scan would be written in a single statement, with no
/// window to genuinely cancel midway.
#[tokio::test]
async fn cancelling_mid_scan_leaves_exactly_the_files_already_applied() {
    const TOTAL: usize = 5 * keeppix_jobs::PRODUCTION_BATCH_SIZE;
    let test = TestDb::start().await;
    let root = std::env::temp_dir().join(format!("kpx-op-cancel-{}", uuid::Uuid::now_v7()));
    fs::create_dir_all(&root).unwrap();
    for n in 0..TOTAL {
        fs::write(root.join(format!("{n:03}.jpg")), b"x").unwrap();
    }
    let library = seed_library(&test, &root).await;
    let admin = library.owner_id;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let ops = OperationsRepo::new(test.db());
    let op = ops.create(&ctx, OperationKind::LibraryScan).await.unwrap();

    let run_db = test.db().clone();
    let library_id = library.id;
    let op_id = op.id;
    let task = tokio::spawn(async move {
        discover::run_with_operation(&run_db, library_id, Duration::ZERO, Some(op_id)).await
    });

    // Wait until some files have already gone through, then cancel: this is
    // the "midway" window the ruling describes.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        assert!(
            tokio::time::Instant::now() < deadline,
            "the scan did not make enough progress to be cancellable"
        );
        let seen = ops.find(&ctx, op.id).await.unwrap();
        if seen.done >= 3 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    ops.request_cancel(&ctx, op.id).await.unwrap();

    tokio::time::timeout(Duration::from_secs(10), task)
        .await
        .expect("the scan did not stop after cancellation")
        .unwrap()
        .unwrap();
    let _ = fs::remove_dir_all(&root);

    let seen = ops.find(&ctx, op.id).await.unwrap();
    assert_eq!(seen.status, OperationStatus::Cancelled);
    assert!(
        seen.done < i64::try_from(TOTAL).unwrap(),
        "a cancellation that still completes the whole batch proved nothing: done={}",
        seen.done
    );
    assert!(seen.done > 0, "at least one applied success must remain");
    assert_eq!(
        i64::try_from(seen.succeeded.len()).unwrap(),
        seen.done,
        "the partial outcome must list exactly what was applied"
    );

    let asset_count: i64 = sqlx::query_scalar("SELECT count(*) FROM assets")
        .fetch_one(test.db().pool())
        .await
        .unwrap();
    assert_eq!(
        asset_count, seen.done,
        "assets written to disk must match the partial outcome, not the whole batch"
    );
}

/// A rescan (with already-known assets) knows an approximate total ahead of
/// time — a first import instead stays honest and doesn't fake a number it
/// doesn't have (`total` is deliberately optional).
#[tokio::test]
async fn a_rescan_of_a_known_library_reports_a_total() {
    let test = TestDb::start().await;
    let root = std::env::temp_dir().join(format!("kpx-op-total-{}", uuid::Uuid::now_v7()));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("a.jpg"), b"x").unwrap();
    let library = seed_library(&test, &root).await;
    let admin = library.owner_id;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let ops = OperationsRepo::new(test.db());

    let first = ops.create(&ctx, OperationKind::LibraryScan).await.unwrap();
    discover::run_with_operation(test.db(), library.id, Duration::ZERO, Some(first.id))
        .await
        .unwrap();
    assert_eq!(
        ops.find(&ctx, first.id).await.unwrap().total,
        None,
        "the first import doesn't know the total yet"
    );

    let second = ops.create(&ctx, OperationKind::LibraryScan).await.unwrap();
    discover::run_with_operation(test.db(), library.id, Duration::ZERO, Some(second.id))
        .await
        .unwrap();
    let _ = fs::remove_dir_all(&root);

    assert_eq!(ops.find(&ctx, second.id).await.unwrap().total, Some(1));
}
