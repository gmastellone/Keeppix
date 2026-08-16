#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::io::Write as _;

use keeppix_media::{is_stable, iter_entries};

fn temp_tree() -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!(
        "keeppix-walk-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(root.join("@eaDir")).unwrap();
    fs::create_dir_all(root.join("2024")).unwrap();
    fs::write(root.join("foto.jpg"), b"not a real jpeg").unwrap();
    fs::write(root.join("2024").join("altra.jpg"), b"xx").unwrap();
    fs::write(root.join("sidecar.xmp"), b"<xmp/>").unwrap();
    fs::write(root.join(".DS_Store"), b"ds").unwrap();
    fs::write(root.join("@eaDir").join("thumb.jpg"), b"synology").unwrap();
    fs::write(root.join("Thumbs.db"), b"win").unwrap();
    root
}

#[test]
fn walker_skips_sidecars_and_vendor_dirs() {
    let root = temp_tree();
    let files: Vec<_> = iter_entries(&root, &[]).map(|f| f.filename).collect();
    let _ = fs::remove_dir_all(&root);

    assert!(files.contains(&"foto.jpg".to_owned()));
    assert!(files.contains(&"altra.jpg".to_owned()));
    assert!(!files.contains(&"sidecar.xmp".to_owned()));
    assert!(!files.iter().any(|n| n == ".DS_Store" || n == "Thumbs.db"));
    assert_eq!(files.len(), 2);
}

/// Pin critico (Task 7): un asset spostato in `.keeppix-trash/` non deve
/// tornare a comparire come "scoperto" al giro di scansione successivo —
/// altrimenti il cestino produce un ciclo infinito di reindicizzazione su
/// una libreria grande. `keeppix_db::TrashRepo::TRASH_DIR_NAME` **deve**
/// restare uguale al nome qui sotto.
#[test]
fn walker_excludes_the_keeppix_trash_directory() {
    let root = std::env::temp_dir().join(format!(
        "keeppix-walk-trash-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(root.join(".keeppix-trash").join("2024")).unwrap();
    fs::write(
        root.join(".keeppix-trash")
            .join("2024")
            .join("cestinata.jpg"),
        b"x",
    )
    .unwrap();
    fs::write(root.join("viva.jpg"), b"x").unwrap();

    let files: Vec<_> = iter_entries(&root, &[]).map(|f| f.filename).collect();
    let _ = fs::remove_dir_all(&root);

    assert!(files.contains(&"viva.jpg".to_owned()));
    assert!(
        !files.contains(&"cestinata.jpg".to_owned()),
        "un file dentro .keeppix-trash non deve mai essere ridescoperto dal walker"
    );
    assert_eq!(files.len(), 1);
}

#[test]
fn unstable_size_is_not_stable() {
    let dir = std::env::temp_dir().join(format!(
        "keeppix-grow-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("growing.bin");
    fs::write(&path, b"aaa").unwrap();
    let first = fs::metadata(&path).unwrap();
    {
        let mut f = fs::OpenOptions::new().append(true).open(&path).unwrap();
        f.write_all(b"bbb").unwrap();
    }
    let second = fs::metadata(&path).unwrap();
    let _ = fs::remove_dir_all(&dir);
    assert!(!is_stable(&first, &second));
}
