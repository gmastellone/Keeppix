mod harness;

use harness::{TestDb, seed_admin, seed_user};
use keeppix_db::{DbError, OperationsRepo};
use keeppix_domain::{AssetId, AuthContext, OperationKind, OperationStatus, SystemRole};

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn creating_an_operation_makes_the_caller_the_owner() {
    let test = TestDb::start().await;
    let admin = seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let repo = OperationsRepo::new(test.db());

    let op = repo.create(&ctx, OperationKind::LibraryScan).await.unwrap();

    assert_eq!(op.owner_id, admin);
    assert_eq!(op.kind, OperationKind::LibraryScan);
    assert_eq!(op.status, OperationStatus::Running);
    assert_eq!(op.done, 0);
    assert_eq!(op.total, None);
    assert!(op.succeeded.is_empty());
}

/// Invariante di sicurezza: chi sonda un id di un'altra persona riceve
/// `Forbidden`, mai `NotFound` — altrimenti l'endpoint diventa un oracolo
/// di esistenza.
#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn a_different_user_cannot_see_someone_elses_operation() {
    let test = TestDb::start().await;
    let admin = seed_admin(&test).await;
    let other = seed_user(&test, admin, "altro").await;
    let repo = OperationsRepo::new(test.db());
    let op = repo
        .create(&AuthContext::user(admin, SystemRole::Admin), OperationKind::LibraryScan)
        .await
        .unwrap();

    let err = repo
        .find(&AuthContext::user(other, SystemRole::User), op.id)
        .await
        .unwrap_err();
    assert!(matches!(err, DbError::Forbidden));
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn an_admin_can_see_any_operation() {
    let test = TestDb::start().await;
    let admin = seed_admin(&test).await;
    let other = seed_user(&test, admin, "altro").await;
    let repo = OperationsRepo::new(test.db());
    let op = repo
        .create(&AuthContext::user(other, SystemRole::User), OperationKind::LibraryScan)
        .await
        .unwrap();

    let seen = repo
        .find(&AuthContext::user(admin, SystemRole::Admin), op.id)
        .await
        .unwrap();
    assert_eq!(seen.id, op.id);
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn recording_success_appends_ids_and_advances_done() {
    let test = TestDb::start().await;
    let admin = seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let repo = OperationsRepo::new(test.db());
    let op = repo.create(&ctx, OperationKind::LibraryScan).await.unwrap();

    let a = AssetId::new();
    let b = AssetId::new();
    repo.record_success(op.id, a).await.unwrap();
    repo.record_success(op.id, b).await.unwrap();

    let seen = repo.find(&ctx, op.id).await.unwrap();
    assert_eq!(seen.done, 2);
    assert_eq!(seen.succeeded, vec![a, b]);
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn request_cancel_is_forbidden_for_a_non_owner() {
    let test = TestDb::start().await;
    let admin = seed_admin(&test).await;
    let other = seed_user(&test, admin, "altro").await;
    let repo = OperationsRepo::new(test.db());
    let op = repo
        .create(&AuthContext::user(admin, SystemRole::Admin), OperationKind::LibraryScan)
        .await
        .unwrap();

    let err = repo
        .request_cancel(&AuthContext::user(other, SystemRole::User), op.id)
        .await
        .unwrap_err();
    assert!(matches!(err, DbError::Forbidden));
    assert!(!repo.is_cancel_requested(op.id).await.unwrap());
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn request_cancel_by_the_owner_sets_the_flag() {
    let test = TestDb::start().await;
    let admin = seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let repo = OperationsRepo::new(test.db());
    let op = repo.create(&ctx, OperationKind::LibraryScan).await.unwrap();

    assert!(!repo.is_cancel_requested(op.id).await.unwrap());
    repo.request_cancel(&ctx, op.id).await.unwrap();
    assert!(repo.is_cancel_requested(op.id).await.unwrap());
}

/// Task 16, Ruling: annullare a metà produce una riuscita parziale, non un
/// rollback. Chiudere l'operazione come `Cancelled` non deve svuotare ciò
/// che era già stato registrato come riuscito.
#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn finishing_as_cancelled_keeps_the_partial_outcome() {
    let test = TestDb::start().await;
    let admin = seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let repo = OperationsRepo::new(test.db());
    let op = repo.create(&ctx, OperationKind::LibraryScan).await.unwrap();
    let a = AssetId::new();
    repo.record_success(op.id, a).await.unwrap();
    repo.request_cancel(&ctx, op.id).await.unwrap();

    repo.finish_cancelled(op.id).await.unwrap();

    let seen = repo.find(&ctx, op.id).await.unwrap();
    assert_eq!(seen.status, OperationStatus::Cancelled);
    assert_eq!(seen.succeeded, vec![a]);
    assert_eq!(seen.done, 1);
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn finish_done_marks_the_operation_complete() {
    let test = TestDb::start().await;
    let admin = seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let repo = OperationsRepo::new(test.db());
    let op = repo.create(&ctx, OperationKind::LibraryScan).await.unwrap();

    repo.finish_done(op.id).await.unwrap();

    let seen = repo.find(&ctx, op.id).await.unwrap();
    assert_eq!(seen.status, OperationStatus::Done);
}

/// Una volta terminata (fatta o annullata), l'operazione non deve più
/// avanzare: il worker si è già fermato, ma un late-write vagante non deve
/// riaprire uno stato concluso.
#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn a_terminal_operation_does_not_advance_further() {
    let test = TestDb::start().await;
    let admin = seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let repo = OperationsRepo::new(test.db());
    let op = repo.create(&ctx, OperationKind::LibraryScan).await.unwrap();
    repo.finish_done(op.id).await.unwrap();

    repo.record_success(op.id, AssetId::new()).await.unwrap();

    let seen = repo.find(&ctx, op.id).await.unwrap();
    assert_eq!(seen.done, 0, "un'operazione conclusa non deve più avanzare");
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn set_total_records_the_expected_count() {
    let test = TestDb::start().await;
    let admin = seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let repo = OperationsRepo::new(test.db());
    let op = repo.create(&ctx, OperationKind::LibraryScan).await.unwrap();

    repo.set_total(op.id, Some(42)).await.unwrap();

    let seen = repo.find(&ctx, op.id).await.unwrap();
    assert_eq!(seen.total, Some(42));
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn list_running_only_returns_the_callers_own_running_operations() {
    let test = TestDb::start().await;
    let admin = seed_admin(&test).await;
    let other = seed_user(&test, admin, "altro").await;
    let repo = OperationsRepo::new(test.db());
    let admin_ctx = AuthContext::user(admin, SystemRole::Admin);
    let other_ctx = AuthContext::user(other, SystemRole::User);

    let running = repo.create(&admin_ctx, OperationKind::LibraryScan).await.unwrap();
    let finished = repo.create(&admin_ctx, OperationKind::LibraryScan).await.unwrap();
    repo.finish_done(finished.id).await.unwrap();
    repo.create(&other_ctx, OperationKind::LibraryScan).await.unwrap();

    let seen = repo.list_running(&admin_ctx).await.unwrap();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].id, running.id);
}
