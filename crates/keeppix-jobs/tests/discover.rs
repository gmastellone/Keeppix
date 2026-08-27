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
async fn discover_schedules_vacuum_analyze_after_scan() {
    let test = TestDb::start().await;
    let root = std::env::temp_dir().join(format!("kpx-vac-{}", uuid::Uuid::now_v7()));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("foto.jpg"), b"jpeg-bytes").unwrap();
    let library = seed_library(&test, &root).await;

    discover::run(test.db(), library.id, Duration::ZERO)
        .await
        .unwrap();

    let n: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM jobs \
         WHERE kind = 'vacuum_analyze' AND dedup_key = 'vacuum_analyze'",
    )
    .fetch_one(test.db().pool())
    .await
    .unwrap();
    let _ = fs::remove_dir_all(&root);
    assert_eq!(n, 1, "a completed scan must enqueue VACUUM ANALYZE");
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
        "an unmounted disk is not an emptied library"
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
        .expect_err("more than 20% disappeared");
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

/// A second discovery over unchanged files must not re-enqueue anything.
/// The jobs need to be marked `done` before the second pass: `dedup_key`
/// only protects `pending`/`running`, so without this step the test would
/// pass even with unconditional enqueueing.
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
    assert_eq!(after_first, 2, "first discovery: one job per file");

    mark_extract_metadata_done(&test).await;
    let pending: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM jobs WHERE kind = 'extract_metadata' AND status IN ('pending','running')",
    )
    .fetch_one(test.db().pool())
    .await
    .unwrap();
    assert_eq!(
        pending, 0,
        "without pending jobs the dedup can't be masking a regression: if this assert fails the test isn't proving what it claims"
    );

    discover::run(test.db(), library.id, Duration::ZERO)
        .await
        .unwrap();
    let after_second = count_extract_metadata(&test).await;
    let _ = fs::remove_dir_all(&root);
    assert_eq!(
        after_second, after_first,
        "rescanning an idle library must not create new extract_metadata jobs"
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
        "a single changed mtime → a single new extract_metadata"
    );
}

/// `SET kind = EXCLUDED.kind` on every rescan would reset the
/// classification. An asset already `raw_image` must stay that way if the
/// file is unchanged.
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
        "rescanning an idle file must not reset kind"
    );
}

fn sony_tiff_header() -> Vec<u8> {
    let mut header = vec![0x49, 0x49, 0x2A, 0x00, 0x08, 0x00, 0x00, 0x00];
    header.extend_from_slice(&[0; 32]);
    header.extend_from_slice(b"SONY");
    header
}
