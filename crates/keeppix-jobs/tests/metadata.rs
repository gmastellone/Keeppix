#![allow(clippy::unwrap_used, clippy::expect_used)]

mod harness;

use std::fs;
use std::time::Duration;

use harness::TestDb;
use keeppix_db::{AssetRepo, LibraryRepo, OverrideRepo};
use keeppix_domain::{AssetStatus, AuthContext, NewLibrary, SystemRole};
use keeppix_jobs::discover;
use keeppix_jobs::metadata;

#[tokio::test]
async fn metadata_indexes_and_enqueues_hash() {
    let test = TestDb::start().await;
    let root = std::env::temp_dir().join(format!("kpx-meta-{}", uuid::Uuid::now_v7()));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("foto.jpg"), b"\xFF\xD8\xFF\xE0\x00\x10JFIF\x00").unwrap();
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let library = LibraryRepo::new(test.db())
        .create(
            &ctx,
            NewLibrary {
                name: "Foto".to_owned(),
                owner_id: admin,
                root_path: root.clone(),
                exclude_patterns: vec![],
            },
        )
        .await
        .unwrap();
    discover::run(test.db(), library.id, Duration::ZERO)
        .await
        .unwrap();
    let asset_id: uuid::Uuid = sqlx::query_scalar("SELECT id FROM assets")
        .fetch_one(test.db().pool())
        .await
        .unwrap();
    metadata::run(test.db(), keeppix_domain::AssetId::from_uuid(asset_id))
        .await
        .unwrap();
    let asset = AssetRepo::new(test.db())
        .get_for_scan(keeppix_domain::AssetId::from_uuid(asset_id))
        .await
        .unwrap();
    assert_eq!(asset.status, AssetStatus::Indexed);
    assert!(asset.taken_at_utc.is_some());
    let hashes: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM jobs WHERE kind = 'hash_asset' AND status = 'pending'",
    )
    .fetch_one(test.db().pool())
    .await
    .unwrap();
    let _ = fs::remove_dir_all(&root);
    assert_eq!(hashes, 1);
}

fn sony_tiff_header() -> Vec<u8> {
    let mut header = vec![0x49, 0x49, 0x2A, 0x00, 0x08, 0x00, 0x00, 0x00];
    header.extend_from_slice(&[0; 32]);
    header.extend_from_slice(b"SONY");
    header
}

fn push_u16(bytes: &mut Vec<u8>, value: u16) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn push_u32(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn push_ifd_entry(bytes: &mut Vec<u8>, tag: u16, kind: u16, count: u32, value: u32) {
    push_u16(bytes, tag);
    push_u16(bytes, kind);
    push_u32(bytes, count);
    push_u32(bytes, value);
}

fn jpeg_with_gps() -> Vec<u8> {
    let mut tiff = vec![b'I', b'I', 0x2A, 0x00];
    push_u32(&mut tiff, 8);
    push_u16(&mut tiff, 1);
    push_ifd_entry(&mut tiff, 0x8825, 4, 1, 26);
    push_u32(&mut tiff, 0);
    push_u16(&mut tiff, 4);
    push_ifd_entry(&mut tiff, 0x0001, 2, 2, u32::from(b'S'));
    push_ifd_entry(&mut tiff, 0x0002, 5, 3, 80);
    push_ifd_entry(&mut tiff, 0x0003, 2, 2, u32::from(b'W'));
    push_ifd_entry(&mut tiff, 0x0004, 5, 3, 104);
    push_u32(&mut tiff, 0);
    for (numerator, denominator) in [(34, 1), (30, 1), (0, 1), (58, 1), (22, 1), (30, 1)] {
        push_u32(&mut tiff, numerator);
        push_u32(&mut tiff, denominator);
    }

    let mut jpeg = vec![0xFF, 0xD8, 0xFF, 0xE1];
    let segment_len = u16::try_from(2 + 6 + tiff.len()).expect("fixture APP1 fits");
    jpeg.extend_from_slice(&segment_len.to_be_bytes());
    jpeg.extend_from_slice(b"Exif\0\0");
    jpeg.extend_from_slice(&tiff);
    jpeg.extend_from_slice(&[0xFF, 0xD9]);
    jpeg
}

async fn seed_tree(
    test: &TestDb,
    root: &std::path::Path,
    filename: &str,
    bytes: &[u8],
) -> (uuid::Uuid, AuthContext) {
    fs::create_dir_all(root).unwrap();
    fs::write(root.join(filename), bytes).unwrap();
    let admin = harness::seed_admin(test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let library = LibraryRepo::new(test.db())
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
        .unwrap();
    discover::run(test.db(), library.id, Duration::ZERO)
        .await
        .unwrap();
    let asset_id = sqlx::query_scalar("SELECT id FROM assets")
        .fetch_one(test.db().pool())
        .await
        .unwrap();
    (asset_id, ctx)
}

#[tokio::test]
async fn metadata_ingest_persists_standard_exif_gps() {
    let test = TestDb::start().await;
    let root = std::env::temp_dir().join(format!("kpx-gps-{}", uuid::Uuid::now_v7()));
    let (asset_id, ctx) = seed_tree(&test, &root, "gps.jpg", &jpeg_with_gps()).await;
    let id = keeppix_domain::AssetId::from_uuid(asset_id);

    metadata::run(test.db(), id).await.unwrap();

    let location = OverrideRepo::new(test.db())
        .effective(&ctx, id)
        .await
        .unwrap()
        .location
        .expect("metadata job persists parsed EXIF GPS");
    let _ = fs::remove_dir_all(&root);
    assert!((location.lat - -34.5).abs() < 1e-9);
    assert!((location.lon - -58.375).abs() < 1e-9);
}

/// D1: `detect_kind` deve classificare il RAW nel job metadata, e hash
/// deve accodare `derive_raw` — non `derive_asset`.
#[tokio::test]
async fn metadata_classifies_sony_tiff_as_raw_and_hash_enqueues_derive_raw() {
    let test = TestDb::start().await;
    let root = std::env::temp_dir().join(format!("kpx-d1-raw-{}", uuid::Uuid::now_v7()));
    let (asset_id, _) = seed_tree(&test, &root, "DSC.ARW", &sony_tiff_header()).await;
    let id = keeppix_domain::AssetId::from_uuid(asset_id);

    metadata::run(test.db(), id).await.unwrap();
    let asset = AssetRepo::new(test.db()).get_for_scan(id).await.unwrap();
    assert_eq!(asset.kind, keeppix_domain::AssetKind::RawImage);

    keeppix_jobs::hash::run(test.db(), id).await.unwrap();
    let derive_raw: i64 = sqlx::query_scalar("SELECT count(*) FROM jobs WHERE kind = 'derive_raw'")
        .fetch_one(test.db().pool())
        .await
        .unwrap();
    let derive_asset: i64 =
        sqlx::query_scalar("SELECT count(*) FROM jobs WHERE kind = 'derive_asset'")
            .fetch_one(test.db().pool())
            .await
            .unwrap();
    let _ = fs::remove_dir_all(&root);
    assert_eq!(derive_raw, 1, "RAW deve andare su derive_raw");
    assert_eq!(derive_asset, 0, "RAW non deve andare sul decoder JPEG");
}

/// D1: un file di testo con estensione media resta unknown e non genera hash.
#[tokio::test]
async fn metadata_leaves_unknown_files_unhashed() {
    let test = TestDb::start().await;
    let root = std::env::temp_dir().join(format!("kpx-d1-unk-{}", uuid::Uuid::now_v7()));
    let (asset_id, _) = seed_tree(&test, &root, "notes.jpg", b"this is not a jpeg").await;
    let id = keeppix_domain::AssetId::from_uuid(asset_id);

    metadata::run(test.db(), id).await.unwrap();
    let asset = AssetRepo::new(test.db()).get_for_scan(id).await.unwrap();
    assert_eq!(asset.kind, keeppix_domain::AssetKind::Unknown);

    let hashes: i64 = sqlx::query_scalar("SELECT count(*) FROM jobs WHERE kind = 'hash_asset'")
        .fetch_one(test.db().pool())
        .await
        .unwrap();
    let _ = fs::remove_dir_all(&root);
    assert_eq!(hashes, 0, "unknown non deve generare hash_asset (D1/D3)");
}

#[test]
fn detect_kind_is_wired_into_the_metadata_job() {
    let src = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/metadata.rs"));
    assert!(
        src.contains("detect_kind"),
        "D1: detect_kind deve essere chiamata da metadata::run, non solo dai suoi test"
    );
}
