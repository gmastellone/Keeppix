mod harness;

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{TimeZone, Utc};
use harness::TestDb;
use keeppix_db::{
    AssetRepo, DbError, FlagRepo, FolderRepo, LibraryRepo, NewGrant, ObjectType, PermissionRepo,
    SubjectType,
};
use keeppix_domain::{
    AssetFlags, AssetId, AssetKind, AssetName, AssetStatus, AuthContext, FolderId, GeoPoint,
    LibraryId, NewAsset, NewLibrary, ObjectRole, Rating, SystemRole, UserId,
};

type LocationRow = (String, Option<f64>, Option<f64>, Option<String>);

#[allow(clippy::expect_used, clippy::unwrap_used)]
async fn seed_library(test: &TestDb, owner: UserId, name: &str, path: &str) -> LibraryId {
    LibraryRepo::new(test.db())
        .create(
            &AuthContext::user(owner, SystemRole::Admin),
            NewLibrary {
                name: name.to_owned(),
                owner_id: owner,
                root_path: std::path::PathBuf::from(path),
                exclude_patterns: vec![],
            },
        )
        .await
        .expect("libreria")
        .id
}

/// Una radice di libreria vera sul filesystem, non `/mnt/foto`: `move_asset`
/// fa `rename()` per davvero (stesso principio di `tests/trash.rs`), quindi
/// serve un percorso su cui si possa scrivere.
#[allow(clippy::expect_used, clippy::unwrap_used)]
fn temp_library_root(label: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "keeppix-move-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("orologio di sistema")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("creazione della radice di test");
    root
}

#[allow(clippy::expect_used, clippy::unwrap_used)]
async fn seed_library_at(test: &TestDb, owner: UserId, root: &Path) -> LibraryId {
    LibraryRepo::new(test.db())
        .create(
            &AuthContext::user(owner, SystemRole::Admin),
            NewLibrary {
                name: "Foto".to_owned(),
                owner_id: owner,
                root_path: root.to_path_buf(),
                exclude_patterns: vec![],
            },
        )
        .await
        .expect("libreria")
        .id
}

#[allow(clippy::expect_used, clippy::unwrap_used)]
fn discovered(folder: FolderId, filename: &str, size: i64) -> NewAsset {
    NewAsset {
        folder_id: folder,
        filename: AssetName::parse(filename).expect("nome"),
        size_bytes: size,
        mtime: Utc.with_ymd_and_hms(2024, 6, 1, 12, 0, 0).unwrap(),
        inode: Some(42),
        kind: AssetKind::Image,
    }
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn upsert_discovered_is_idempotent_and_refreshes_stat() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let library = seed_library(&test, admin, "Foto", "/mnt/foto").await;
    let folder = FolderRepo::new(test.db())
        .ensure_path(library, &["2024"])
        .await
        .unwrap();
    let repo = AssetRepo::new(test.db());

    let first = repo
        .upsert_discovered(discovered(folder.id, "DSC_0042.ARW", 1000))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(first.status, AssetStatus::Discovered);
    assert_eq!(first.size_bytes, 1000);

    let mut again = discovered(folder.id, "DSC_0042.ARW", 2000);
    again.mtime = Utc.with_ymd_and_hms(2024, 6, 2, 12, 0, 0).unwrap();
    let second = repo.upsert_discovered(again).await.unwrap().unwrap();

    assert_eq!(first.id, second.id, "riscansionare non duplica l'asset");
    assert_eq!(second.size_bytes, 2000);
    assert_eq!(
        second.mtime,
        Utc.with_ymd_and_hms(2024, 6, 2, 12, 0, 0).unwrap()
    );
}

/// Fase 10 Task 21: l'inserimento a lotti deve produrre gli stessi asset
/// dell'inserimento a file singolo, in una sola istruzione.
#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn batch_upsert_discovered_inserts_every_new_file() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let library = seed_library(&test, admin, "Foto", "/mnt/foto").await;
    let folder = FolderRepo::new(test.db())
        .ensure_path(library, &["2024"])
        .await
        .unwrap();
    let repo = AssetRepo::new(test.db());

    let items: Vec<NewAsset> = (0..5)
        .map(|n| discovered(folder.id, &format!("IMG_{n:04}.jpg"), 100 + n))
        .collect();
    let inserted = repo.batch_upsert_discovered(&items).await.unwrap();

    assert_eq!(inserted.len(), 5);
    let mut names: Vec<&str> = inserted.iter().map(|a| a.filename.as_str()).collect();
    names.sort_unstable();
    assert_eq!(
        names,
        vec![
            "IMG_0000.jpg",
            "IMG_0001.jpg",
            "IMG_0002.jpg",
            "IMG_0003.jpg",
            "IMG_0004.jpg"
        ]
    );

    let count = AssetRepo::new(test.db())
        .count_in_library(library)
        .await
        .unwrap();
    assert_eq!(count, 5);
}

/// Come `upsert_discovered`: un file già noto con lo stesso mtime/size non
/// deve comparire nel risultato — il chiamante non deve riaccodare
/// metadata/hash per lui.
#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn batch_upsert_discovered_omits_unchanged_files() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let library = seed_library(&test, admin, "Foto", "/mnt/foto").await;
    let folder = FolderRepo::new(test.db())
        .ensure_path(library, &["2024"])
        .await
        .unwrap();
    let repo = AssetRepo::new(test.db());

    let first_pass = vec![
        discovered(folder.id, "changed.jpg", 100),
        discovered(folder.id, "unchanged.jpg", 100),
    ];
    repo.batch_upsert_discovered(&first_pass).await.unwrap();

    let mut changed = discovered(folder.id, "changed.jpg", 200);
    changed.mtime = Utc.with_ymd_and_hms(2024, 6, 3, 12, 0, 0).unwrap();
    let unchanged = discovered(folder.id, "unchanged.jpg", 100);
    let second_pass = vec![changed, unchanged];

    let result = repo.batch_upsert_discovered(&second_pass).await.unwrap();

    assert_eq!(result.len(), 1, "solo il file cambiato deve tornare");
    assert_eq!(result[0].filename.as_str(), "changed.jpg");
    assert_eq!(result[0].size_bytes, 200);
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn the_same_filename_in_two_folders_is_two_assets() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let library = seed_library(&test, admin, "Foto", "/mnt/foto").await;
    let folders = FolderRepo::new(test.db());
    let a = folders.ensure_path(library, &["a"]).await.unwrap();
    let b = folders.ensure_path(library, &["b"]).await.unwrap();
    let repo = AssetRepo::new(test.db());

    let left = repo
        .upsert_discovered(discovered(a.id, "DSC_0042.ARW", 100))
        .await
        .unwrap()
        .unwrap();
    let right = repo
        .upsert_discovered(discovered(b.id, "DSC_0042.ARW", 100))
        .await
        .unwrap()
        .unwrap();

    assert_ne!(left.id, right.id);
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn the_same_hash_on_two_assets_is_not_a_conflict() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let library = seed_library(&test, admin, "Foto", "/mnt/foto").await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let folder = FolderRepo::new(test.db())
        .ensure_path(library, &["2024"])
        .await
        .unwrap();
    let repo = AssetRepo::new(test.db());
    let hash = [0xab_u8; 32];

    let a = repo
        .upsert_discovered(discovered(folder.id, "a.jpg", 100))
        .await
        .unwrap()
        .unwrap();
    let b = repo
        .upsert_discovered(discovered(folder.id, "b.jpg", 100))
        .await
        .unwrap()
        .unwrap();
    repo.set_hash(a.id, hash).await.unwrap();
    repo.set_hash(b.id, hash).await.unwrap();

    let found = repo.find_by_hash(&ctx, &hash).await.unwrap();
    let ids: Vec<_> = found.iter().map(|asset| asset.id).collect();
    assert!(ids.contains(&a.id) && ids.contains(&b.id));
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn status_transitions_follow_the_pipeline() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let library = seed_library(&test, admin, "Foto", "/mnt/foto").await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let folder = FolderRepo::new(test.db())
        .ensure_path(library, &["2024"])
        .await
        .unwrap();
    let repo = AssetRepo::new(test.db());

    let asset = repo
        .upsert_discovered(discovered(folder.id, "DSC_0042.ARW", 100))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(asset.status, AssetStatus::Discovered);

    repo.set_indexed(
        asset.id,
        Utc.with_ymd_and_hms(2024, 6, 1, 10, 0, 0).unwrap(),
        6000,
        4000,
    )
    .await
    .unwrap();
    let indexed = repo.find_by_id(&ctx, asset.id).await.unwrap();
    assert_eq!(indexed.status, AssetStatus::Indexed);
    assert_eq!(indexed.width, Some(6000));
    assert_eq!(indexed.height, Some(4000));

    repo.set_error(asset.id, "unreadable").await.unwrap();
    assert_eq!(
        repo.find_by_id(&ctx, asset.id).await.unwrap().status,
        AssetStatus::Error
    );

    repo.mark_offline(asset.id).await.unwrap();
    assert_eq!(
        repo.find_by_id(&ctx, asset.id).await.unwrap().status,
        AssetStatus::Offline
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn a_plain_user_cannot_read_someone_elses_assets() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let mario = harness::seed_user(&test, admin, "mario").await;
    let library = seed_library(&test, admin, "Foto", "/mnt/foto").await;
    let folder = FolderRepo::new(test.db())
        .ensure_path(library, &["2024"])
        .await
        .unwrap();
    let repo = AssetRepo::new(test.db());
    let asset = repo
        .upsert_discovered(discovered(folder.id, "DSC_0042.ARW", 100))
        .await
        .unwrap()
        .unwrap();

    let mario_ctx = AuthContext::user(mario, SystemRole::User);
    assert!(matches!(
        repo.find_by_id(&mario_ctx, asset.id).await,
        Err(DbError::Forbidden)
    ));
    assert!(matches!(
        repo.find_by_folder(&mario_ctx, folder.id).await,
        Err(DbError::Forbidden)
    ));
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn probing_an_unknown_asset_id_is_forbidden() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let mario = harness::seed_user(&test, admin, "mario").await;
    let mario_ctx = AuthContext::user(mario, SystemRole::User);
    let repo = AssetRepo::new(test.db());

    assert!(matches!(
        repo.find_by_id(&mario_ctx, AssetId::new()).await,
        Err(DbError::Forbidden)
    ));
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn a_library_tree_and_assets_round_trip_with_permissions() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let mario = harness::seed_user(&test, admin, "mario").await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let library = seed_library(&test, admin, "Foto", "/mnt/foto").await;
    let folders = FolderRepo::new(test.db());
    let leaf = folders
        .ensure_path(library, &["2024", "Grecia", "Santorini"])
        .await
        .unwrap();
    assert_eq!(leaf.depth, 4, "radice piu tre livelli");

    let repo = AssetRepo::new(test.db());
    for i in 1..=12 {
        repo.upsert_discovered(discovered(leaf.id, &format!("DSC_{i:04}.ARW"), 1000 + i))
            .await
            .unwrap();
    }

    let listed = repo.find_by_folder(&ctx, leaf.id).await.unwrap();
    assert_eq!(listed.len(), 12);
    assert_eq!(
        repo.count_by_status(&ctx, AssetStatus::Discovered)
            .await
            .unwrap(),
        12
    );

    let mario_ctx = AuthContext::user(mario, SystemRole::User);
    assert!(matches!(
        repo.find_by_folder(&mario_ctx, leaf.id).await,
        Err(DbError::Forbidden)
    ));
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn moving_a_folder_does_not_touch_assets() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let library = seed_library(&test, admin, "Foto", "/mnt/foto").await;
    let folders = FolderRepo::new(test.db());
    folders
        .ensure_path(library, &["2024", "Grecia"])
        .await
        .unwrap();
    let archive = folders.ensure_path(library, &["Archivio"]).await.unwrap();
    let root = folders.ensure_root(library).await.unwrap();
    let y2024 = folders.ensure_child(&root, "2024").await.unwrap();
    let greece = folders.ensure_child(&y2024, "Grecia").await.unwrap();

    let repo = AssetRepo::new(test.db());
    let asset = repo
        .upsert_discovered(discovered(greece.id, "DSC_0042.ARW", 100))
        .await
        .unwrap()
        .unwrap();

    folders
        .move_subtree(&ctx, greece.id, archive.id)
        .await
        .unwrap();

    let after = repo.find_by_id(&ctx, asset.id).await.unwrap();
    assert_eq!(after.folder_id, greece.id, "l'asset resta sulla cartella");
    assert_eq!(after.filename.as_str(), "DSC_0042.ARW");
    assert_eq!(after.size_bytes, 100);
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn find_by_folder_omits_trashed_assets() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let library = seed_library(&test, admin, "Foto", "/mnt/foto").await;
    let folder = FolderRepo::new(test.db())
        .ensure_path(library, &["2024"])
        .await
        .unwrap();
    let repo = AssetRepo::new(test.db());
    let live = repo
        .upsert_discovered(discovered(folder.id, "live.jpg", 10))
        .await
        .unwrap()
        .unwrap();
    let dumped = repo
        .upsert_discovered(discovered(folder.id, "bin.jpg", 10))
        .await
        .unwrap()
        .unwrap();
    sqlx::query("UPDATE assets SET status = 'trashed' WHERE id = $1")
        .bind(dumped.id.as_uuid())
        .execute(test.db().pool())
        .await
        .unwrap();

    let listed = repo.find_by_folder(&ctx, folder.id).await.unwrap();
    let ids: Vec<_> = listed.iter().map(|a| a.id).collect();
    assert!(ids.contains(&live.id));
    assert!(!ids.contains(&dumped.id));
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn upsert_discovered_returns_none_when_stat_is_unchanged() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let library = seed_library(&test, admin, "Foto", "/mnt/foto").await;
    let folder = FolderRepo::new(test.db())
        .ensure_path(library, &["2024"])
        .await
        .unwrap();
    let repo = AssetRepo::new(test.db());
    let new = discovered(folder.id, "DSC_0042.ARW", 1000);

    let first = repo.upsert_discovered(new.clone()).await.unwrap();
    assert!(first.is_some(), "primo insert restituisce l'asset");

    let second = repo.upsert_discovered(new).await.unwrap();
    assert!(
        second.is_none(),
        "stessi mtime e size_bytes → None, niente da rifare (D2)"
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn upsert_discovered_returns_some_when_size_changes() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let library = seed_library(&test, admin, "Foto", "/mnt/foto").await;
    let folder = FolderRepo::new(test.db())
        .ensure_path(library, &["2024"])
        .await
        .unwrap();
    let repo = AssetRepo::new(test.db());

    let first = repo
        .upsert_discovered(discovered(folder.id, "DSC_0042.ARW", 1000))
        .await
        .unwrap()
        .unwrap();
    let second = repo
        .upsert_discovered(discovered(folder.id, "DSC_0042.ARW", 2000))
        .await
        .unwrap()
        .unwrap();

    assert_eq!(first.id, second.id);
    assert_eq!(second.size_bytes, 2000);
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn set_kind_persists_and_unchanged_upsert_does_not_reset_it() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let library = seed_library(&test, admin, "Foto", "/mnt/foto").await;
    let folder = FolderRepo::new(test.db())
        .ensure_path(library, &["2024"])
        .await
        .unwrap();
    let repo = AssetRepo::new(test.db());
    let mut new = discovered(folder.id, "DSC_0042.ARW", 1000);
    new.kind = AssetKind::Unknown;

    let asset = repo.upsert_discovered(new.clone()).await.unwrap().unwrap();
    repo.set_kind(asset.id, AssetKind::RawImage).await.unwrap();

    assert!(
        repo.upsert_discovered(new).await.unwrap().is_none(),
        "file invariato"
    );
    let again = repo.get_for_scan(asset.id).await.unwrap();
    assert_eq!(again.kind, AssetKind::RawImage);
}

#[tokio::test]
#[allow(clippy::too_many_lines, clippy::unwrap_used)]
async fn exif_location_does_not_overwrite_any_assigned_location() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let library = seed_library(&test, admin, "Foto", "/mnt/foto").await;
    let folder = FolderRepo::new(test.db())
        .ensure_path(library, &["2024"])
        .await
        .unwrap();
    let repo = AssetRepo::new(test.db());
    let unset = repo
        .upsert_discovered(discovered(folder.id, "unset.jpg", 100))
        .await
        .unwrap()
        .unwrap();
    let user = repo
        .upsert_discovered(discovered(folder.id, "user.jpg", 100))
        .await
        .unwrap()
        .unwrap();
    let map_pin = repo
        .upsert_discovered(discovered(folder.id, "map-pin.jpg", 100))
        .await
        .unwrap()
        .unwrap();
    let copied = repo
        .upsert_discovered(discovered(folder.id, "copied.jpg", 100))
        .await
        .unwrap()
        .unwrap();
    let gpx = repo
        .upsert_discovered(discovered(folder.id, "gpx.jpg", 100))
        .await
        .unwrap()
        .unwrap();

    for (asset, source, lon, lat) in [
        (user.id, "user", 9.0_f64, 45.0_f64),
        (map_pin.id, "map_pin", 12.5_f64, 41.9_f64),
        (copied.id, "copied", 151.2_f64, -33.8_f64),
        (gpx.id, "gpx", 18.0_f64, 50.0_f64),
    ] {
        sqlx::query(
            "UPDATE assets \
             SET location = ST_SetSRID(ST_MakePoint($2, $3), 4326)::geography, \
                 location_source = $4 \
             WHERE id = $1",
        )
        .bind(asset.as_uuid())
        .bind(lon)
        .bind(lat)
        .bind(source)
        .execute(test.db().pool())
        .await
        .unwrap();
    }

    let exif = GeoPoint {
        lat: -34.5,
        lon: -58.375,
    };
    for asset in [unset.id, user.id, map_pin.id, copied.id, gpx.id] {
        repo.set_exif_location(asset, exif).await.unwrap();
    }

    let rows: Vec<LocationRow> = sqlx::query_as(
        "SELECT filename, ST_Y(location::geometry), ST_X(location::geometry), location_source \
         FROM assets ORDER BY filename",
    )
    .fetch_all(test.db().pool())
    .await
    .unwrap();

    assert_eq!(
        rows,
        vec![
            (
                "copied.jpg".to_owned(),
                Some(-33.8),
                Some(151.2),
                Some("copied".to_owned()),
            ),
            (
                "gpx.jpg".to_owned(),
                Some(50.0),
                Some(18.0),
                Some("gpx".to_owned()),
            ),
            (
                "map-pin.jpg".to_owned(),
                Some(41.9),
                Some(12.5),
                Some("map_pin".to_owned()),
            ),
            (
                "unset.jpg".to_owned(),
                Some(-34.5),
                Some(-58.375),
                Some("exif".to_owned()),
            ),
            (
                "user.jpg".to_owned(),
                Some(45.0),
                Some(9.0),
                Some("user".to_owned()),
            ),
        ]
    );
}

/// Fase 9 Task 1: la primitiva di spostamento sicuro.
mod move_asset {
    use super::*;

    #[tokio::test]
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    async fn moves_the_row_and_the_file_keeping_the_same_id() {
        let test = TestDb::start().await;
        let admin = harness::seed_admin(&test).await;
        let ctx = AuthContext::user(admin, SystemRole::Admin);
        let root = temp_library_root("basic");
        let library = seed_library_at(&test, admin, &root).await;
        let folders = FolderRepo::new(test.db());
        let src = folders.ensure_path(library, &["2024"]).await.unwrap();
        let dst = folders
            .ensure_path(library, &["2024", "Scelte"])
            .await
            .unwrap();
        fs::create_dir_all(root.join("2024").join("Scelte")).unwrap();
        let original = root.join("2024").join("foto.jpg");
        fs::write(&original, b"contenuto").unwrap();

        let asset = AssetRepo::new(test.db())
            .upsert_discovered(discovered(src.id, "foto.jpg", 9))
            .await
            .unwrap()
            .unwrap();

        let moved = AssetRepo::new(test.db())
            .move_asset(
                &ctx,
                asset.id,
                dst.id,
                AssetName::parse("scelta.jpg").unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(moved.id, asset.id, "stesso id, non un asset nuovo");
        assert_eq!(moved.folder_id, dst.id);
        assert_eq!(moved.filename.as_str(), "scelta.jpg");
        assert!(!original.exists(), "il file non resta al vecchio percorso");
        let new_path = root.join("2024").join("Scelte").join("scelta.jpg");
        assert!(new_path.is_file());
        assert_eq!(fs::read(&new_path).unwrap(), b"contenuto");

        let by_id = AssetRepo::new(test.db())
            .find_by_id(&ctx, asset.id)
            .await
            .unwrap();
        assert_eq!(
            by_id.folder_id, dst.id,
            "la riga esistente si aggiorna, non se ne crea una seconda"
        );

        let _ = fs::remove_dir_all(&root);
    }

    /// Fase 11 Task 7 (§13.3 campo 8, "Sposta in cartella"): il wrapper
    /// dietro la rotta di massa — sposta senza rinominare.
    #[tokio::test]
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    async fn move_to_folder_keeps_the_filename_unchanged() {
        let test = TestDb::start().await;
        let admin = harness::seed_admin(&test).await;
        let ctx = AuthContext::user(admin, SystemRole::Admin);
        let root = temp_library_root("move-to-folder");
        let library = seed_library_at(&test, admin, &root).await;
        let folders = FolderRepo::new(test.db());
        let src = folders.ensure_path(library, &["2024"]).await.unwrap();
        let dst = folders
            .ensure_path(library, &["2024", "Scelte"])
            .await
            .unwrap();
        fs::create_dir_all(root.join("2024").join("Scelte")).unwrap();
        fs::write(root.join("2024").join("foto.jpg"), b"contenuto").unwrap();

        let asset = AssetRepo::new(test.db())
            .upsert_discovered(discovered(src.id, "foto.jpg", 9))
            .await
            .unwrap()
            .unwrap();

        let moved = AssetRepo::new(test.db())
            .move_to_folder(&ctx, asset.id, dst.id)
            .await
            .unwrap();

        assert_eq!(moved.folder_id, dst.id);
        assert_eq!(moved.filename.as_str(), "foto.jpg", "il nome non cambia");
        assert!(root.join("2024").join("Scelte").join("foto.jpg").is_file());

        let _ = fs::remove_dir_all(&root);
    }

    #[tokio::test]
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    async fn preserves_flags_across_the_move() {
        let test = TestDb::start().await;
        let admin = harness::seed_admin(&test).await;
        let ctx = AuthContext::user(admin, SystemRole::Admin);
        let root = temp_library_root("flags");
        let library = seed_library_at(&test, admin, &root).await;
        let folders = FolderRepo::new(test.db());
        let src = folders.ensure_path(library, &["2024"]).await.unwrap();
        let dst = folders.ensure_path(library, &["Preferite"]).await.unwrap();
        fs::create_dir_all(root.join("2024")).unwrap();
        fs::create_dir_all(root.join("Preferite")).unwrap();
        fs::write(root.join("2024").join("foto.jpg"), b"x").unwrap();

        let asset = AssetRepo::new(test.db())
            .upsert_discovered(discovered(src.id, "foto.jpg", 1))
            .await
            .unwrap()
            .unwrap();

        FlagRepo::new(test.db())
            .set(
                &ctx,
                asset.id,
                &AssetFlags {
                    rating: Some(Rating::parse(5).unwrap()),
                    favorite: true,
                    ..Default::default()
                },
            )
            .await
            .unwrap();

        AssetRepo::new(test.db())
            .move_asset(
                &ctx,
                asset.id,
                dst.id,
                AssetName::parse("foto.jpg").unwrap(),
            )
            .await
            .unwrap();

        let flags = FlagRepo::new(test.db()).get(&ctx, asset.id).await.unwrap();
        assert_eq!(flags.rating, Some(Rating::parse(5).unwrap()));
        assert!(
            flags.favorite,
            "asset_flags è una chiave esterna su asset_id, non su (folder_id, filename): \
             lo spostamento non deve perderla"
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[tokio::test]
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    async fn refuses_to_overwrite_an_existing_destination() {
        let test = TestDb::start().await;
        let admin = harness::seed_admin(&test).await;
        let ctx = AuthContext::user(admin, SystemRole::Admin);
        let root = temp_library_root("collision");
        let library = seed_library_at(&test, admin, &root).await;
        let folders = FolderRepo::new(test.db());
        let folder = folders.ensure_path(library, &["2024"]).await.unwrap();
        fs::create_dir_all(root.join("2024")).unwrap();
        fs::write(root.join("2024").join("a.jpg"), b"a").unwrap();
        fs::write(root.join("2024").join("b.jpg"), b"b").unwrap();

        let assets = AssetRepo::new(test.db());
        let a = assets
            .upsert_discovered(discovered(folder.id, "a.jpg", 1))
            .await
            .unwrap()
            .unwrap();
        assets
            .upsert_discovered(discovered(folder.id, "b.jpg", 1))
            .await
            .unwrap()
            .unwrap();

        let result = assets
            .move_asset(&ctx, a.id, folder.id, AssetName::parse("b.jpg").unwrap())
            .await;

        assert!(
            matches!(result, Err(DbError::Collision(_))),
            "b.jpg esiste già nella cartella di destinazione: {result:?}"
        );
        assert!(
            root.join("2024").join("a.jpg").is_file(),
            "il file di partenza non deve spostarsi se la collisione blocca l'operazione"
        );
        assert_eq!(
            fs::read(root.join("2024").join("b.jpg")).unwrap(),
            b"b",
            "il file di destinazione non deve essere toccato"
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[tokio::test]
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    async fn requires_editor_on_the_destination_folder_not_just_the_source() {
        let test = TestDb::start().await;
        let admin = harness::seed_admin(&test).await;
        let root = temp_library_root("perm");
        let library = seed_library_at(&test, admin, &root).await;
        let folders = FolderRepo::new(test.db());
        let src = folders.ensure_path(library, &["Src"]).await.unwrap();
        let dst = folders.ensure_path(library, &["Dst"]).await.unwrap();
        fs::create_dir_all(root.join("Src")).unwrap();
        fs::create_dir_all(root.join("Dst")).unwrap();
        fs::write(root.join("Src").join("foto.jpg"), b"x").unwrap();

        let asset = AssetRepo::new(test.db())
            .upsert_discovered(discovered(src.id, "foto.jpg", 1))
            .await
            .unwrap()
            .unwrap();

        let editor = harness::seed_user(&test, admin, "editor-src-only").await;
        PermissionRepo::new(test.db())
            .grant(
                &AuthContext::user(admin, SystemRole::Admin),
                NewGrant {
                    subject: SubjectType::User,
                    subject_id: editor.as_uuid(),
                    object: ObjectType::Folder,
                    object_id: src.id.as_uuid(),
                    role: ObjectRole::Editor,
                    inherit: true,
                },
            )
            .await
            .unwrap();
        // Deliberatamente nessuna concessione su `dst`: editor solo sulla
        // cartella di partenza, come l'utente che ha scelto/scartato foto
        // nel proprio culling ma non ha accesso a `_taken` altrui.

        let editor_ctx = AuthContext::user(editor, SystemRole::User);
        let result = AssetRepo::new(test.db())
            .move_asset(
                &editor_ctx,
                asset.id,
                dst.id,
                AssetName::parse("foto.jpg").unwrap(),
            )
            .await;

        assert!(
            matches!(result, Err(DbError::Forbidden)),
            "editor solo sulla sorgente non deve poter scrivere nella destinazione: {result:?}"
        );
        assert!(
            root.join("Src").join("foto.jpg").is_file(),
            "un tentativo respinto non deve toccare il file"
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[tokio::test]
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    async fn moves_the_xmp_sidecar_alongside_the_asset() {
        let test = TestDb::start().await;
        let admin = harness::seed_admin(&test).await;
        let ctx = AuthContext::user(admin, SystemRole::Admin);
        let root = temp_library_root("sidecar");
        let library = seed_library_at(&test, admin, &root).await;
        let folders = FolderRepo::new(test.db());
        let src = folders.ensure_path(library, &["2024"]).await.unwrap();
        let dst = folders.ensure_path(library, &["2025"]).await.unwrap();
        fs::create_dir_all(root.join("2024")).unwrap();
        fs::create_dir_all(root.join("2025")).unwrap();
        fs::write(root.join("2024").join("foto.arw"), b"raw").unwrap();
        fs::write(root.join("2024").join("foto.arw.xmp"), b"<xmp/>").unwrap();

        let asset = AssetRepo::new(test.db())
            .upsert_discovered(discovered(src.id, "foto.arw", 3))
            .await
            .unwrap()
            .unwrap();

        AssetRepo::new(test.db())
            .move_asset(
                &ctx,
                asset.id,
                dst.id,
                AssetName::parse("foto.arw").unwrap(),
            )
            .await
            .unwrap();

        assert!(
            !root.join("2024").join("foto.arw.xmp").exists(),
            "il sidecar non deve restare al vecchio percorso"
        );
        assert_eq!(
            fs::read(root.join("2025").join("foto.arw.xmp")).unwrap(),
            b"<xmp/>",
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[tokio::test]
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    async fn is_a_no_op_when_the_destination_equals_the_source() {
        let test = TestDb::start().await;
        let admin = harness::seed_admin(&test).await;
        let ctx = AuthContext::user(admin, SystemRole::Admin);
        let root = temp_library_root("noop");
        let library = seed_library_at(&test, admin, &root).await;
        let folders = FolderRepo::new(test.db());
        let folder = folders.ensure_path(library, &["2024"]).await.unwrap();
        fs::create_dir_all(root.join("2024")).unwrap();
        let path = root.join("2024").join("foto.jpg");
        fs::write(&path, b"contenuto").unwrap();

        let asset = AssetRepo::new(test.db())
            .upsert_discovered(discovered(folder.id, "foto.jpg", 9))
            .await
            .unwrap()
            .unwrap();

        let result = AssetRepo::new(test.db())
            .move_asset(
                &ctx,
                asset.id,
                folder.id,
                AssetName::parse("foto.jpg").unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(result.id, asset.id);
        assert!(
            path.is_file(),
            "nessun rename inutile sullo stesso percorso"
        );

        let _ = fs::remove_dir_all(&root);
    }
}

// Fase 11 Task 7 (SP-3 §11, dimensione "Fotocamera" — `AssetView`).
mod camera_models_among {
    use super::*;

    #[tokio::test]
    #[allow(clippy::unwrap_used)]
    async fn returns_only_assets_with_a_readable_camera_model() {
        let test = TestDb::start().await;
        let admin = harness::seed_admin(&test).await;
        let library = seed_library(&test, admin, "Foto", "/mnt/camera-models").await;
        let folder = FolderRepo::new(test.db())
            .ensure_path(library, &["2024"])
            .await
            .unwrap();
        let repo = AssetRepo::new(test.db());
        let with_camera = repo
            .upsert_discovered(discovered(folder.id, "a.jpg", 1))
            .await
            .unwrap()
            .unwrap();
        let no_exif_row = repo
            .upsert_discovered(discovered(folder.id, "b.jpg", 1))
            .await
            .unwrap()
            .unwrap();
        let exif_without_camera = repo
            .upsert_discovered(discovered(folder.id, "c.jpg", 1))
            .await
            .unwrap()
            .unwrap();

        sqlx::query("INSERT INTO asset_exif (asset_id, camera_model) VALUES ($1, $2)")
            .bind(with_camera.id.as_uuid())
            .bind("FUJIFILM X-T5")
            .execute(test.db().pool())
            .await
            .unwrap();
        sqlx::query("INSERT INTO asset_exif (asset_id, camera_model) VALUES ($1, NULL)")
            .bind(exif_without_camera.id.as_uuid())
            .execute(test.db().pool())
            .await
            .unwrap();

        let map = repo
            .camera_models_among(&[with_camera.id, no_exif_row.id, exif_without_camera.id])
            .await
            .unwrap();

        assert_eq!(map.len(), 1);
        assert_eq!(map[&with_camera.id], "FUJIFILM X-T5");
        assert!(
            !map.contains_key(&no_exif_row.id),
            "no asset_exif row at all"
        );
        assert!(
            !map.contains_key(&exif_without_camera.id),
            "an asset_exif row exists but camera_model is NULL"
        );
    }

    #[tokio::test]
    #[allow(clippy::unwrap_used)]
    async fn is_empty_for_an_empty_id_list() {
        let test = TestDb::start().await;
        let map = AssetRepo::new(test.db())
            .camera_models_among(&[])
            .await
            .unwrap();
        assert!(map.is_empty());
    }
}

// Fase 11 Task 8 (§19.2 campi 6-9, sezione "SCATTO" del pannello
// informazioni): a differenza di `camera_models_among` (bulk, un solo
// campo), qui il dettaglio pieno per un asset alla volta.
mod exif_for {
    use super::*;

    #[tokio::test]
    #[allow(clippy::unwrap_used)]
    async fn returns_the_full_row_when_one_exists() {
        let test = TestDb::start().await;
        let admin = harness::seed_admin(&test).await;
        let library = seed_library(&test, admin, "Foto", "/mnt/exif-for-full").await;
        let folder = FolderRepo::new(test.db())
            .ensure_path(library, &["2024"])
            .await
            .unwrap();
        let asset = AssetRepo::new(test.db())
            .upsert_discovered(discovered(folder.id, "a.jpg", 1))
            .await
            .unwrap()
            .unwrap();

        sqlx::query(
            "INSERT INTO asset_exif (asset_id, raw, camera_make, camera_model, lens, iso, \
                                      f_number, exposure, focal_length) \
             VALUES ($1, '{}', $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(asset.id.as_uuid())
        .bind("Sony")
        .bind("Sony A7 IV")
        .bind("FE 24-70mm f/2.8")
        .bind(400_i32)
        .bind(3.5_f32)
        .bind("1/250")
        .bind(70.0_f32)
        .execute(test.db().pool())
        .await
        .unwrap();

        let exif = AssetRepo::new(test.db())
            .exif_for(asset.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(exif.camera_make.as_deref(), Some("Sony"));
        assert_eq!(exif.camera_model.as_deref(), Some("Sony A7 IV"));
        assert_eq!(exif.lens.as_deref(), Some("FE 24-70mm f/2.8"));
        assert_eq!(exif.iso, Some(400));
        assert_eq!(exif.f_number, Some(3.5));
        assert_eq!(exif.exposure.as_deref(), Some("1/250"));
        assert_eq!(exif.focal_length, Some(70.0));
    }

    #[tokio::test]
    #[allow(clippy::unwrap_used)]
    async fn is_none_without_an_asset_exif_row() {
        let test = TestDb::start().await;
        let admin = harness::seed_admin(&test).await;
        let library = seed_library(&test, admin, "Foto", "/mnt/exif-for-none").await;
        let folder = FolderRepo::new(test.db())
            .ensure_path(library, &["2024"])
            .await
            .unwrap();
        let asset = AssetRepo::new(test.db())
            .upsert_discovered(discovered(folder.id, "b.jpg", 1))
            .await
            .unwrap()
            .unwrap();

        let exif = AssetRepo::new(test.db()).exif_for(asset.id).await.unwrap();
        assert!(exif.is_none());
    }
}
