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

/// Un demosaic finto che conta le invocazioni: il punto del task è
/// dimostrare per conteggio, non per tempo, che il passo costoso non parte
/// quando la preview incorporata basta.
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

// Il tipo di ritorno segue la firma condivisa da `MockDemosaic<F>`: qui non
// serve mai restituire `Err`, ma deve restare compatibile con gli altri mock.
#[allow(clippy::unnecessary_wraps)]
fn fake_demosaic_output() -> Result<RawPreview, JobError> {
    // 4x2 RGB8 a tinta unita: basta a passare per la pipeline dei derivati,
    // le dimensioni non contano per questo test.
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

/// Un TIFF valido (header + una IFD vuota) ma senza alcun tag di preview:
/// `extract_embedded_preview` lo riconosce come RAW ma restituisce `Ok(None)`.
fn minimal_tiff_without_preview() -> Vec<u8> {
    let mut buf = Vec::new();
    buf.extend_from_slice(b"II*\0"); // little-endian, magic 42
    buf.extend_from_slice(&8u32.to_le_bytes()); // offset alla prima IFD
    buf.extend_from_slice(&0u16.to_le_bytes()); // 0 entry nella IFD
    buf.extend_from_slice(&0u32.to_le_bytes()); // nessuna IFD successiva
    buf
}

struct Seeded {
    hash: [u8; 32],
    data_dir: PathBuf,
    root: PathBuf,
    asset_id: AssetId,
}

/// Crea una libreria, ci scrive un file con questi byte, e registra
/// l'asset RAW corrispondente col suo `content_hash` già calcolato — lo
/// stesso stato in cui la pipeline di hash lo lascerebbe prima di accodare
/// `DeriveRaw`. `admin` va creato una sola volta per `TestDb`:
/// `create_bootstrap_admin` rifiuta un secondo amministratore sulla stessa
/// istanza.
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
        "la preview incorporata di sample.arw supera 1440px: libraw non deve partire"
    );
    let (thumb, _) = keeppix_media::derivative_paths(&seeded.data_dir, &seeded.hash);
    assert!(thumb.is_file(), "il thumbnail va comunque generato");

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
        "senza preview incorporata la cascata deve tentare il demosaic"
    );
    let (thumb, _) = keeppix_media::derivative_paths(&seeded.data_dir, &seeded.hash);
    assert!(
        thumb.is_file(),
        "il thumbnail va generato dall'output del demosaic"
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
        "un raw corrotto è un set_error, non un errore di job che blocca la coda"
    );

    let asset = AssetRepo::new(test.db())
        .get_for_scan(corrupt.asset_id)
        .await
        .unwrap();
    assert_eq!(asset.status, AssetStatus::Error);

    // La coda continua: il prossimo job (su un file diverso) va a buon fine.
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
        "il thumbhash deve essere salvato anche per la preview incorporata di un RAW"
    );

    let _ = fs::remove_dir_all(&seeded.root);
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
        "il thumbhash deve essere salvato anche per il fallback al demosaic"
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
        "una seconda esecuzione non deve rilanciare il demosaic: il derivato c'è già"
    );
    let second_run_bytes = fs::read(&thumb).unwrap();
    assert_eq!(
        first_run_bytes, second_run_bytes,
        "nessuna rigenerazione del derivato"
    );

    let _ = fs::remove_dir_all(&seeded.root);
}
