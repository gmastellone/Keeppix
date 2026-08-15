#![allow(clippy::unwrap_used)]

use keeppix_media::hash_file;

#[test]
fn same_bytes_same_hash() {
    let dir = std::env::temp_dir().join(format!("kpx-hash-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let a = dir.join("a.bin");
    let b = dir.join("b.bin");
    std::fs::write(&a, b"keeppix").unwrap();
    std::fs::write(&b, b"keeppix").unwrap();
    let ha = hash_file(&a).unwrap();
    let hb = hash_file(&b).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(ha, hb);
}

#[test]
fn different_bytes_different_hash() {
    let dir = std::env::temp_dir().join(format!("kpx-hashd-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let a = dir.join("a.bin");
    std::fs::write(&a, b"keeppix").unwrap();
    let ha = hash_file(&a).unwrap();
    std::fs::write(&a, b"keeppix!").unwrap();
    let hb = hash_file(&a).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    assert_ne!(ha, hb);
}
