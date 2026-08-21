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

/// Un giro completo, senza annullamento, deve chiudere l'operazione come
/// `Done` con l'elenco completo dei successi (spec Task 16, punto 1: un
/// `operation_id` che segue davvero l'operazione fino in fondo).
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

/// Ruling Task 16: annullare a metà produce una riuscita parziale, non un
/// rollback. I file già scritti restano; l'operazione li elenca.
#[tokio::test]
async fn cancelling_mid_scan_leaves_exactly_the_files_already_applied() {
    const TOTAL: usize = 40;
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

    // Aspetta che qualche file sia già passato, poi annulla: è la finestra
    // "a metà" che il Ruling descrive.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        assert!(
            tokio::time::Instant::now() < deadline,
            "la scansione non ha fatto abbastanza progresso da poter annullare"
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
        .expect("la scansione non si è fermata dopo l'annullamento")
        .unwrap()
        .unwrap();
    let _ = fs::remove_dir_all(&root);

    let seen = ops.find(&ctx, op.id).await.unwrap();
    assert_eq!(seen.status, OperationStatus::Cancelled);
    assert!(
        seen.done < i64::try_from(TOTAL).unwrap(),
        "un annullamento che completa comunque tutto il lotto non ha provato nulla: done={}",
        seen.done
    );
    assert!(seen.done > 0, "deve restare almeno un successo applicato");
    assert_eq!(
        i64::try_from(seen.succeeded.len()).unwrap(),
        seen.done,
        "l'involucro parziale deve elencare esattamente ciò che è stato applicato"
    );

    let asset_count: i64 = sqlx::query_scalar("SELECT count(*) FROM assets")
        .fetch_one(test.db().pool())
        .await
        .unwrap();
    assert_eq!(
        asset_count, seen.done,
        "gli asset scritti sul disco devono combaciare con l'esito parziale, non con il lotto intero"
    );
}

/// Una riscansione (con asset già noti) conosce in anticipo un totale
/// approssimativo — la prima importazione invece resta onesta e non finge
/// un numero che non ha (spec Task 16: `total` è opzionale apposta).
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
        "la prima importazione non conosce ancora il totale"
    );

    let second = ops.create(&ctx, OperationKind::LibraryScan).await.unwrap();
    discover::run_with_operation(test.db(), library.id, Duration::ZERO, Some(second.id))
        .await
        .unwrap();
    let _ = fs::remove_dir_all(&root);

    assert_eq!(ops.find(&ctx, second.id).await.unwrap().total, Some(1));
}
