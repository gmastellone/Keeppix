#![allow(clippy::unwrap_used, clippy::expect_used)]

mod harness;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use harness::TestDb;
use keeppix_db::{AssetRepo, FolderRepo, LibraryRepo};
use keeppix_domain::{
    AssetId, AssetKind, AssetName, AssetStatus, AuthContext, NewAsset, NewLibrary, SystemRole,
    UserId,
};
use keeppix_jobs::JobError;
use keeppix_jobs::raw::{self, Demosaic};
use keeppix_media::{PreviewSource, RawPreview};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("keeppix-media/tests/fixtures")
        .join(name)
}

/// A fake demosaic that counts invocations: the point is to prove, by
/// count and not by timing, that the expensive step doesn't run when the
/// embedded preview is enough.
struct MockDemosaic<F> {
    calls: AtomicUsize,
    f: F,
}

impl<F> MockDemosaic<F>
where
    F: Fn() -> Result<RawPreview, JobError> + Send + Sync,
{
    fn new(f: F) -> Self {
        Self {
            calls: AtomicUsize::new(0),
            f,
        }
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl<F> Demosaic for MockDemosaic<F>
where
    F: Fn() -> Result<RawPreview, JobError> + Send + Sync,
{
    fn demosaic(&self, _path: &Path) -> Result<RawPreview, JobError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        (self.f)()
    }
}

fn never_called() -> Result<RawPreview, JobError> {
    Err(JobError::Worker(
        "demosaic must not be called when the embedded preview is big enough".to_owned(),
    ))
}

// The return type follows the signature shared by `MockDemosaic<F>`: this
// one never needs to return `Err`, but must stay compatible with the other
// mocks.
#[allow(clippy::unnecessary_wraps)]
fn fake_demosaic_output() -> Result<RawPreview, JobError> {
    // 4x2 flat-color RGB8: enough to pass through the derivatives pipeline,
    // the dimensions don't matter for this test.
    Ok(RawPreview {
        bytes: vec![120u8; 4 * 2 * 3],
        width: 4,
        height: 2,
        source: PreviewSource::Demosaic,
    })
}

fn always_fails() -> Result<RawPreview, JobError> {
    Err(JobError::Worker("dcraw_emu: corrupt raw file".to_owned()))
}

/// A valid TIFF (header + one empty IFD) but with no preview tag at all:
/// `extract_embedded_preview` recognizes it as RAW but returns `Ok(None)`.
fn minimal_tiff_without_preview() -> Vec<u8> {
    let mut buf = Vec::new();
    buf.extend_from_slice(b"II*\0"); // little-endian, magic 42
    buf.extend_from_slice(&8u32.to_le_bytes()); // offset to the first IFD
    buf.extend_from_slice(&0u16.to_le_bytes()); // 0 entries in the IFD
    buf.extend_from_slice(&0u32.to_le_bytes()); // no next IFD
    buf
}

struct Seeded {
    hash: [u8; 32],
    data_dir: PathBuf,
    root: PathBuf,
    asset_id: AssetId,
}

/// Creates a library, writes a file with these bytes into it, and
/// registers the corresponding RAW asset with its `content_hash` already
/// computed — the same state the hash pipeline would leave it in before
/// enqueueing `DeriveRaw`. `admin` must be created only once per `TestDb`:
/// `create_bootstrap_admin` rejects a second admin on the same instance.
async fn seed_raw_asset(test: &TestDb, admin: UserId, filename: &str, bytes: &[u8]) -> Seeded {
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let root = std::env::temp_dir().join(format!("kpx-raw-{}", uuid::Uuid::now_v7()));
    fs::create_dir_all(&root).unwrap();
    let path = root.join(filename);
    fs::write(&path, bytes).unwrap();

    let library = LibraryRepo::new(test.db())
        .create(
            &ctx,
            NewLibrary {
                name: "Raw".to_owned(),
                owner_id: admin,
                root_path: root.clone(),
                exclude_patterns: vec![],
            },
        )
        .await
        .unwrap();
    let folder = FolderRepo::new(test.db())
        .ensure_path(library.id, &[])
        .await
        .unwrap();

    let meta = fs::metadata(&path).unwrap();
    let assets = AssetRepo::new(test.db());
    let asset = assets
        .upsert_discovered(NewAsset {
            folder_id: folder.id,
            filename: AssetName::parse(filename).unwrap(),
            size_bytes: i64::try_from(meta.len()).unwrap(),
            mtime: chrono::DateTime::<chrono::Utc>::from(meta.modified().unwrap()),
            inode: None,
            kind: AssetKind::RawImage,
        })
        .await
        .unwrap()
        .unwrap();
    let hash = keeppix_media::hash_file(&path).unwrap();
    assets.set_hash(asset.id, hash).await.unwrap();

    Seeded {
        hash,
        data_dir: root.join(".keeppix-data"),
        root,
        asset_id: asset.id,
    }
}

#[tokio::test]
async fn a_raw_with_a_large_preview_never_calls_libraw() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let bytes = fs::read(fixture("sample.arw")).unwrap();
    let seeded = seed_raw_asset(&test, admin, "sample.arw", &bytes).await;

    let demosaic = MockDemosaic::new(never_called);
    raw::run_with(test.db(), &seeded.data_dir, seeded.hash, &demosaic)
        .await
        .unwrap();

    assert_eq!(
        demosaic.calls(),
        0,
        "sample.arw's embedded preview exceeds 1440px: libraw must not run"
    );
    let (thumb, _) = keeppix_media::derivative_paths(&seeded.data_dir, &seeded.hash);
    assert!(thumb.is_file(), "the thumbnail must still be generated");

    let _ = fs::remove_dir_all(&seeded.root);
}

#[tokio::test]
async fn a_raw_without_a_preview_falls_back_to_demosaic() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let seeded = seed_raw_asset(&test, admin, "empty.tiff", &minimal_tiff_without_preview()).await;

    let demosaic = MockDemosaic::new(fake_demosaic_output);
    raw::run_with(test.db(), &seeded.data_dir, seeded.hash, &demosaic)
        .await
        .unwrap();

    assert_eq!(
        demosaic.calls(),
        1,
        "without an embedded preview the cascade must attempt demosaic"
    );
    let (thumb, _) = keeppix_media::derivative_paths(&seeded.data_dir, &seeded.hash);
    assert!(
        thumb.is_file(),
        "the thumbnail must be generated from the demosaic output"
    );

    let _ = fs::remove_dir_all(&seeded.root);
}

#[tokio::test]
async fn a_corrupt_raw_sets_the_asset_to_error_and_does_not_block_the_queue() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let corrupt = seed_raw_asset(&test, admin, "garbage.raw", b"not a raw file at all").await;
    let good_bytes = fs::read(fixture("sample.nef")).unwrap();
    let good = seed_raw_asset(&test, admin, "sample.nef", &good_bytes).await;

    let failing = MockDemosaic::new(always_fails);
    let result = raw::run_with(test.db(), &corrupt.data_dir, corrupt.hash, &failing).await;
    assert!(
        result.is_ok(),
        "a corrupt raw is a set_error, not a job error that blocks the queue"
    );

    let asset = AssetRepo::new(test.db())
        .get_for_scan(corrupt.asset_id)
        .await
        .unwrap();
    assert_eq!(asset.status, AssetStatus::Error);

    // The queue keeps going: the next job (on a different file) succeeds.
    let succeeding = MockDemosaic::new(never_called);
    raw::run_with(test.db(), &good.data_dir, good.hash, &succeeding)
        .await
        .unwrap();
    let (thumb, _) = keeppix_media::derivative_paths(&good.data_dir, &good.hash);
    assert!(thumb.is_file());

    let _ = fs::remove_dir_all(&corrupt.root);
    let _ = fs::remove_dir_all(&good.root);
}

#[tokio::test]
async fn deriving_from_the_embedded_preview_populates_the_thumbhash() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let bytes = fs::read(fixture("sample.arw")).unwrap();
    let seeded = seed_raw_asset(&test, admin, "sample.arw", &bytes).await;

    let demosaic = MockDemosaic::new(never_called);
    raw::run_with(test.db(), &seeded.data_dir, seeded.hash, &demosaic)
        .await
        .unwrap();

    let asset = AssetRepo::new(test.db())
        .get_for_scan(seeded.asset_id)
        .await
        .unwrap();
    assert!(
        asset.thumbhash.is_some_and(|h| !h.is_empty()),
        "the thumbhash must be saved even for a RAW's embedded preview"
    );

    let _ = fs::remove_dir_all(&seeded.root);
}

#[tokio::test]
async fn a_duplicate_hashed_after_the_first_derive_still_gets_the_thumbhash() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let bytes = fs::read(fixture("sample.arw")).unwrap();
    let first = seed_raw_asset(&test, admin, "a.arw", &bytes).await;

    let demosaic = MockDemosaic::new(never_called);
    raw::run_with(test.db(), &first.data_dir, first.hash, &demosaic)
        .await
        .unwrap();

    let assets = AssetRepo::new(test.db());
    let asset_a = assets.get_for_scan(first.asset_id).await.unwrap();
    let folder_a = FolderRepo::new(test.db())
        .find_by_id(&ctx, asset_a.folder_id)
        .await
        .unwrap();
    fs::create_dir_all(first.root.join("copy")).unwrap();
    fs::write(first.root.join("copy/b.arw"), &bytes).unwrap();
    let folder_b = FolderRepo::new(test.db())
        .ensure_path(folder_a.library_id, &["copy"])
        .await
        .unwrap();
    let meta = fs::metadata(first.root.join("copy/b.arw")).unwrap();
    let asset_b = assets
        .upsert_discovered(NewAsset {
            folder_id: folder_b.id,
            filename: AssetName::parse("b.arw").unwrap(),
            size_bytes: i64::try_from(meta.len()).unwrap(),
            mtime: chrono::DateTime::<chrono::Utc>::from(meta.modified().unwrap()),
            inode: None,
            kind: AssetKind::RawImage,
        })
        .await
        .unwrap()
        .unwrap();
    assets.set_hash(asset_b.id, first.hash).await.unwrap();

    raw::run_with(test.db(), &first.data_dir, first.hash, &demosaic)
        .await
        .unwrap();

    let a = assets.get_for_scan(first.asset_id).await.unwrap();
    let b = assets.get_for_scan(asset_b.id).await.unwrap();
    assert!(
        a.thumbhash.is_some_and(|h| !h.is_empty()),
        "the first asset must keep its thumbhash"
    );
    assert!(
        b.thumbhash.is_some_and(|h| !h.is_empty()),
        "a duplicate hashed after the first derive_raw must not be left without a thumbhash"
    );

    let _ = fs::remove_dir_all(&first.root);
}

#[tokio::test]
async fn deriving_from_the_demosaic_fallback_populates_the_thumbhash() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let seeded = seed_raw_asset(&test, admin, "empty.tiff", &minimal_tiff_without_preview()).await;

    let demosaic = MockDemosaic::new(fake_demosaic_output);
    raw::run_with(test.db(), &seeded.data_dir, seeded.hash, &demosaic)
        .await
        .unwrap();

    let asset = AssetRepo::new(test.db())
        .get_for_scan(seeded.asset_id)
        .await
        .unwrap();
    assert!(
        asset.thumbhash.is_some_and(|h| !h.is_empty()),
        "the thumbhash must be saved even for the demosaic fallback"
    );

    let _ = fs::remove_dir_all(&seeded.root);
}

#[tokio::test]
async fn the_job_is_idempotent() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let seeded = seed_raw_asset(&test, admin, "empty.tiff", &minimal_tiff_without_preview()).await;

    let demosaic = MockDemosaic::new(fake_demosaic_output);
    raw::run_with(test.db(), &seeded.data_dir, seeded.hash, &demosaic)
        .await
        .unwrap();
    assert_eq!(demosaic.calls(), 1);

    let (thumb, _) = keeppix_media::derivative_paths(&seeded.data_dir, &seeded.hash);
    let first_run_bytes = fs::read(&thumb).unwrap();

    raw::run_with(test.db(), &seeded.data_dir, seeded.hash, &demosaic)
        .await
        .unwrap();
    assert_eq!(
        demosaic.calls(),
        1,
        "a second run must not re-trigger demosaic: the derivative already exists"
    );
    let second_run_bytes = fs::read(&thumb).unwrap();
    assert_eq!(
        first_run_bytes, second_run_bytes,
        "no regeneration of the derivative"
    );

    let _ = fs::remove_dir_all(&seeded.root);
}
