#![allow(clippy::unwrap_used)]

use keeppix_media::{derive_jpeg, hash_file};

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
