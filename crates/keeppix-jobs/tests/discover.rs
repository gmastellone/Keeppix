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

async fn count_extract_metadata(test: &TestDb) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM jobs WHERE kind = 'extract_metadata'")
        .fetch_one(test.db().pool())
        .await
        .unwrap()
}

async fn mark_extract_metadata_done(test: &TestDb) {
    sqlx::query("UPDATE jobs SET status = 'done' WHERE kind = 'extract_metadata'")
        .execute(test.db().pool())
        .await
        .unwrap();
}

/// D2: una seconda discovery su file fermi non deve riaccodare nulla.
/// I job vanno marcati `done` prima del secondo giro: `dedup_key` protegge
/// solo `pending`/`running`, quindi senza questo passo il test passerebbe
/// anche con l'accodamento incondizionato.
#[tokio::test]
async fn a_second_discover_on_unchanged_files_does_not_enqueue_metadata() {
    let test = TestDb::start().await;
    let root = std::env::temp_dir().join(format!("kpx-d2-idle-{}", uuid::Uuid::now_v7()));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("a.jpg"), b"\xFF\xD8\xFF\xE0").unwrap();
    fs::write(root.join("b.jpg"), b"\xFF\xD8\xFF\xE0").unwrap();
    let library = seed_library(&test, &root).await;

    discover::run(test.db(), library.id, Duration::ZERO)
        .await
        .unwrap();
    let after_first = count_extract_metadata(&test).await;
    assert_eq!(after_first, 2, "prima discovery: un job per file");

    mark_extract_metadata_done(&test).await;
    let pending: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM jobs WHERE kind = 'extract_metadata' AND status IN ('pending','running')",
    )
    .fetch_one(test.db().pool())
    .await
    .unwrap();
    assert_eq!(
        pending, 0,
        "senza job pending il dedup non maschera D2: se questo assert fallisce il test non sta provando il difetto"
    );

    discover::run(test.db(), library.id, Duration::ZERO)
        .await
        .unwrap();
    let after_second = count_extract_metadata(&test).await;
    let _ = fs::remove_dir_all(&root);
    assert_eq!(
        after_second, after_first,
        "riscansione a libreria ferma non deve creare nuovi extract_metadata (D2)"
    );
}

#[tokio::test]
async fn touching_one_mtime_enqueues_exactly_one_metadata_job() {
    let test = TestDb::start().await;
    let root = std::env::temp_dir().join(format!("kpx-d2-touch-{}", uuid::Uuid::now_v7()));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("a.jpg"), b"\xFF\xD8\xFF\xE0").unwrap();
    fs::write(root.join("b.jpg"), b"\xFF\xD8\xFF\xE0").unwrap();
    let library = seed_library(&test, &root).await;

    discover::run(test.db(), library.id, Duration::ZERO)
        .await
        .unwrap();
    let after_first = count_extract_metadata(&test).await;
    mark_extract_metadata_done(&test).await;

    let later = filetime::FileTime::from_unix_time(chrono::Utc::now().timestamp() + 10, 0);
    filetime::set_file_mtime(root.join("a.jpg"), later).unwrap();

    discover::run(test.db(), library.id, Duration::ZERO)
        .await
        .unwrap();
    let after_touch = count_extract_metadata(&test).await;
    let _ = fs::remove_dir_all(&root);
    assert_eq!(
        after_touch,
        after_first + 1,
        "un solo mtime cambiato → un solo extract_metadata nuovo"
    );
}

/// D1+D2: `SET kind = EXCLUDED.kind` a ogni riscansione azzererebbe la
/// classificazione. Un asset già `raw_image` deve restarlo se il file è fermo.
#[tokio::test]
async fn rescan_of_unchanged_file_does_not_reset_kind_to_unknown() {
    let test = TestDb::start().await;
    let root = std::env::temp_dir().join(format!("kpx-d2-kind-{}", uuid::Uuid::now_v7()));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("DSC.ARW"), sony_tiff_header()).unwrap();
    let library = seed_library(&test, &root).await;

    discover::run(test.db(), library.id, Duration::ZERO)
        .await
        .unwrap();
    sqlx::query("UPDATE assets SET kind = 'raw_image'")
        .execute(test.db().pool())
        .await
        .unwrap();
    mark_extract_metadata_done(&test).await;

    discover::run(test.db(), library.id, Duration::ZERO)
        .await
        .unwrap();

    let kind: String = sqlx::query_scalar("SELECT kind FROM assets")
        .fetch_one(test.db().pool())
        .await
        .unwrap();
    let _ = fs::remove_dir_all(&root);
    assert_eq!(
        kind, "raw_image",
        "riscansione ferma non deve azzerare kind (D1+D2)"
    );
}

fn sony_tiff_header() -> Vec<u8> {
    let mut header = vec![0x49, 0x49, 0x2A, 0x00, 0x08, 0x00, 0x00, 0x00];
    header.extend_from_slice(&[0; 32]);
    header.extend_from_slice(b"SONY");
    header
}
