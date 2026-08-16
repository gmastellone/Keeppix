#![allow(clippy::unwrap_used)]

use keeppix_media::{derive_from_bytes, derive_jpeg, hash_file};

fn fixture(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

#[test]
fn deriving_from_bytes_matches_deriving_from_a_file() {
    let dir = tempfile::tempdir().unwrap();
    let src = fixture("tiny.jpg");
    let bytes = std::fs::read(&src).unwrap();
    let hash = [7u8; 32];

    let from_bytes = derive_from_bytes(&bytes, dir.path(), &hash).unwrap();

    let dir2 = tempfile::tempdir().unwrap();
    let from_file = derive_jpeg(&src, dir2.path(), &hash).unwrap();

    assert_eq!(
        from_bytes.thumbhash, from_file.thumbhash,
        "la stessa immagine deve produrre lo stesso thumbhash da entrambe le vie"
    );
    assert_eq!(
        std::fs::read(&from_bytes.thumb).unwrap().len(),
        std::fs::read(&from_file.thumb).unwrap().len()
    );
}

#[test]
fn derive_writes_thumb_and_leaves_original() {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tiny.jpg");
    let original = std::fs::read(&fixture).unwrap();
    let hash = hash_file(&fixture).unwrap();
    let data = std::env::temp_dir().join(format!("kpx-der-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&data);
    let first = derive_jpeg(&fixture, &data, &hash).unwrap();
    assert!(first.thumb.is_file());
    assert!(!first.skipped);
    let mtime = std::fs::metadata(&first.thumb).unwrap().modified().unwrap();
    let second = derive_jpeg(&fixture, &data, &hash).unwrap();
    assert!(second.skipped);
    let mtime2 = std::fs::metadata(&second.thumb)
        .unwrap()
        .modified()
        .unwrap();
    assert_eq!(mtime, mtime2);
    assert_eq!(std::fs::read(&fixture).unwrap(), original);
    let _ = std::fs::remove_dir_all(&data);
}
