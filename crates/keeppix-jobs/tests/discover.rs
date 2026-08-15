#![allow(clippy::unwrap_used, clippy::expect_used)]

mod harness;

use std::fs;
use std::time::Duration;

use harness::TestDb;
use keeppix_db::{AssetRepo, FolderRepo, LibraryRepo};
use keeppix_domain::{AssetKind, AuthContext, NewLibrary, SystemRole};
use keeppix_jobs::discover;

fn write_tree(root: &std::path::Path) {
    fs::create_dir_all(root.join("@eaDir")).unwrap();
    fs::write(root.join("foto.jpg"), b"jpeg-bytes").unwrap();
    fs::write(root.join("sidecar.xmp"), b"<xmp/>").unwrap();
    fs::write(root.join(".DS_Store"), b"ds").unwrap();
    fs::write(root.join("@eaDir").join("hidden.jpg"), b"no").unwrap();
}

async fn seed_library(test: &TestDb, root: &std::path::Path) -> keeppix_domain::Library {
    let admin = harness::seed_admin(test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    LibraryRepo::new(test.db())
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

#[tokio::test]
async fn discover_indexes_photos_and_skips_junk() {
    let test = TestDb::start().await;
    let root = std::env::temp_dir().join(format!("kpx-disc-{}", uuid::Uuid::now_v7()));
    fs::create_dir_all(&root).unwrap();
    write_tree(&root);
    let library = seed_library(&test, &root).await;

    discover::run(test.db(), library.id, Duration::ZERO)
        .await
        .unwrap();

    let ctx = AuthContext::user(library.owner_id, SystemRole::Admin);
    let root_folder = FolderRepo::new(test.db())
        .ensure_path(library.id, &[])
        .await
        .unwrap();
    let assets = AssetRepo::new(test.db())
        .find_by_folder(&ctx, root_folder.id)
        .await
        .unwrap();
    let _ = fs::remove_dir_all(&root);

    assert_eq!(assets.len(), 1);
    assert_eq!(assets[0].filename.as_str(), "foto.jpg");
    assert_eq!(assets[0].kind, AssetKind::Unknown);

    let pending: i64 =
        sqlx::query_scalar("SELECT count(*) FROM jobs WHERE kind = 'extract_metadata'")
            .fetch_one(test.db().pool())
            .await
            .unwrap();
    assert_eq!(pending, 1);
}

#[tokio::test]
async fn missing_root_marks_the_library_offline_without_deleting() {
    let test = TestDb::start().await;
    let root = std::env::temp_dir().join(format!("kpx-gone-{}", uuid::Uuid::now_v7()));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("foto.jpg"), b"x").unwrap();
    let library = seed_library(&test, &root).await;
    discover::run(test.db(), library.id, Duration::ZERO)
        .await
        .unwrap();
    fs::remove_dir_all(&root).unwrap();

    discover::run(test.db(), library.id, Duration::ZERO)
        .await
        .unwrap();

    let again = LibraryRepo::new(test.db())
        .load_for_scan(library.id)
        .await
        .unwrap();
    assert_eq!(again.status, keeppix_domain::LibraryStatus::Offline);
    assert_eq!(
        AssetRepo::new(test.db())
            .count_in_library(library.id)
            .await
            .unwrap(),
        1,
        "un disco smontato non è una libreria svuotata"
    );
}

#[tokio::test]
async fn mass_disappearance_stops_without_marking_offline() {
    let test = TestDb::start().await;
    let root = std::env::temp_dir().join(format!("kpx-mass-{}", uuid::Uuid::now_v7()));
    fs::create_dir_all(&root).unwrap();
    for n in 0..5 {
        fs::write(root.join(format!("{n}.jpg")), b"x").unwrap();
    }
    let library = seed_library(&test, &root).await;
    discover::run(test.db(), library.id, Duration::ZERO)
        .await
        .unwrap();
    for n in 0..3 {
        fs::remove_file(root.join(format!("{n}.jpg"))).unwrap();
    }

    let err = discover::run(test.db(), library.id, Duration::ZERO)
        .await
        .expect_err("più del 20% è sparito");
    assert!(matches!(err, keeppix_jobs::JobError::MassDisappearance));

    let library = LibraryRepo::new(test.db())
        .load_for_scan(library.id)
        .await
        .unwrap();
    assert_eq!(library.status, keeppix_domain::LibraryStatus::Active);
    let _ = fs::remove_dir_all(&root);
}

#[tokio::test]
async fn discover_is_idempotent_on_the_metadata_job() {
    let test = TestDb::start().await;
    let root = std::env::temp_dir().join(format!("kpx-idemp-{}", uuid::Uuid::now_v7()));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("foto.jpg"), b"x").unwrap();
    let library = seed_library(&test, &root).await;
    discover::run(test.db(), library.id, Duration::ZERO)
        .await
        .unwrap();
    discover::run(test.db(), library.id, Duration::ZERO)
        .await
        .unwrap();
    let n: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM jobs WHERE kind = 'extract_metadata' AND status IN ('pending','running')",
    )
    .fetch_one(test.db().pool())
    .await
    .unwrap();
    let _ = fs::remove_dir_all(&root);
    assert_eq!(n, 1);
}
