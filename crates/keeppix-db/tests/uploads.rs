mod harness;

use std::fs;
use std::path::{Path, PathBuf};

use harness::TestDb;
use keeppix_db::{
    AssetRepo, DbError, FolderRepo, LibraryRepo, NewShareLink, NewUploadSession, ShareLinkRepo,
    UploadSessionRepo,
};
use keeppix_domain::{
    AssetKind, AssetName, AuthContext, CollisionOutcome, FolderId, LibraryId, NewAsset, NewLibrary,
    ShareLinkParams, SystemRole, UploadOwner, UserId,
};
use uuid::Uuid;

/// Una radice di libreria vera sul filesystem: la sessione scrive un
/// temporaneo per davvero e `finalize` fa `rename()`.
#[allow(clippy::expect_used, clippy::unwrap_used)]
fn temp_library_root() -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "keeppix-uploads-{}-{}",
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
async fn seed_library(test: &TestDb, owner: UserId, root: &Path) -> LibraryId {
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

/// Cartella radice della libreria (sotto la quale i test creano il target),
/// creata sia come riga di dominio sia come cartella vera sul disco.
#[allow(clippy::expect_used, clippy::unwrap_used)]
async fn seed_target_folder(
    test: &TestDb,
    library: LibraryId,
    root: &Path,
    name: &str,
) -> FolderId {
    let folder = FolderRepo::new(test.db())
        .ensure_path(library, &[name])
        .await
        .expect("cartella");
    fs::create_dir_all(root.join(name)).expect("cartella su disco");
    folder.id
}

/// Stesso algoritmo di `keeppix_media::hash::hash_file`, calcolato qui senza
/// dipendere da quella crate: `keeppix-db` non deve conoscere le immagini,
/// nemmeno nei test.
#[allow(clippy::expect_used)]
fn hash_file(path: &Path) -> [u8; 32] {
    let bytes = fs::read(path).expect("lettura del file per l'hash di test");
    *blake3::hash(&bytes).as_bytes()
}

/// Un link condiviso vero, con riga in `share_links`: la FK di
/// `upload_sessions.share_link_id` la richiede, un `Uuid::now_v7()` sciolto
/// no.
#[allow(clippy::expect_used)]
async fn seed_share_link(
    test: &TestDb,
    admin: UserId,
    folder: FolderId,
    allow_upload: bool,
) -> Uuid {
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let (id, _token) = ShareLinkRepo::new(test.db())
        .create(
            &ctx,
            NewShareLink {
                object_type: "folder".to_owned(),
                object_id: folder.as_uuid(),
                password_hash: None,
                expires_at: None,
                max_views: None,
                allow_download: true,
                allow_original: false,
                allow_upload,
                allow_cdn_cache: false,
                hide_metadata: true,
                upload_quota_bytes: None,
            },
        )
        .await
        .expect("creazione del link condiviso");
    id
}

fn new_session(folder: FolderId, filename: &str, expected_size: i64) -> NewUploadSession {
    NewUploadSession {
        target_folder_id: folder,
        filename: filename.to_owned(),
        expected_size,
        expected_hash: None,
        client_mtime: None,
    }
}

#[allow(clippy::expect_used, clippy::unwrap_used)]
async fn seed_existing_asset(
    test: &TestDb,
    folder: FolderId,
    filename: &str,
    hash: [u8; 32],
) -> keeppix_domain::AssetId {
    let asset = AssetRepo::new(test.db())
        .upsert_discovered(NewAsset {
            folder_id: folder,
            filename: AssetName::parse(filename).expect("nome"),
            size_bytes: 3,
            mtime: chrono::Utc::now(),
            inode: None,
            kind: AssetKind::Image,
        })
        .await
        .expect("insert")
        .expect("nuovo asset");
    AssetRepo::new(test.db())
        .set_hash(asset.id, hash)
        .await
        .expect("hash");
    asset.id
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn creating_a_session_puts_the_temp_path_inside_keeppix_tmp() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let root = temp_library_root();
    let library = seed_library(&test, admin, &root).await;
    let folder = seed_target_folder(&test, library, &root, "2024").await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);

    let session = UploadSessionRepo::new(test.db())
        .create(&ctx, new_session(folder, "foto.jpg", 1024))
        .await
        .unwrap();

    assert_eq!(session.filename, "foto.jpg");
    assert_eq!(session.received_bytes, 0);
    assert!(
        session
            .temp_path
            .starts_with(root.join(keeppix_db::UPLOAD_TMP_DIR_NAME)),
        "il temporaneo deve stare in .keeppix-tmp dentro la libreria: {}",
        session.temp_path.display()
    );

    let _ = fs::remove_dir_all(&root);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn insufficient_disk_space_is_rejected_at_creation_not_mid_upload() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let root = temp_library_root();
    let library = seed_library(&test, admin, &root).await;
    let folder = seed_target_folder(&test, library, &root, "2024").await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);

    // Nessun filesystem reale ha un exabyte libero.
    let huge = 1_000_000_000_000_000_000_i64;
    let result = UploadSessionRepo::new(test.db())
        .create(&ctx, new_session(folder, "gigante.mov", huge))
        .await;
    assert!(matches!(result, Err(DbError::InsufficientStorage)));

    let sessions: i64 = sqlx::query_scalar("SELECT count(*) FROM upload_sessions")
        .fetch_one(test.db().pool())
        .await
        .unwrap();
    assert_eq!(sessions, 0, "una sessione respinta non deve lasciare righe");

    let _ = fs::remove_dir_all(&root);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn a_share_link_without_allow_upload_is_forbidden_before_accepting_any_byte() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let root = temp_library_root();
    let library = seed_library(&test, admin, &root).await;
    let folder = seed_target_folder(&test, library, &root, "2024").await;

    let link_id = seed_share_link(&test, admin, folder, false).await;
    let ctx = AuthContext::share_link(
        link_id,
        ShareLinkParams {
            object_type: "folder".to_owned(),
            object_id: folder.as_uuid(),
            allow_download: true,
            allow_original: false,
            hide_metadata: true,
            allow_upload: false,
            upload_quota_bytes: None,
        },
    );

    let result = UploadSessionRepo::new(test.db())
        .create(&ctx, new_session(folder, "ospite.jpg", 10))
        .await;
    assert!(matches!(result, Err(DbError::Forbidden)));

    let sessions: i64 = sqlx::query_scalar("SELECT count(*) FROM upload_sessions")
        .fetch_one(test.db().pool())
        .await
        .unwrap();
    assert_eq!(sessions, 0);

    let _ = fs::remove_dir_all(&root);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn a_share_link_with_allow_upload_can_open_a_session_on_its_own_folder() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let root = temp_library_root();
    let library = seed_library(&test, admin, &root).await;
    let folder = seed_target_folder(&test, library, &root, "2024").await;

    let link_id = seed_share_link(&test, admin, folder, true).await;
    let ctx = AuthContext::share_link(
        link_id,
        ShareLinkParams {
            object_type: "folder".to_owned(),
            object_id: folder.as_uuid(),
            allow_download: true,
            allow_original: false,
            hide_metadata: true,
            allow_upload: true,
            upload_quota_bytes: None,
        },
    );

    let session = UploadSessionRepo::new(test.db())
        .create(&ctx, new_session(folder, "ospite.jpg", 10))
        .await
        .unwrap();
    assert!(matches!(session.owner, UploadOwner::ShareLink(_)));

    let _ = fs::remove_dir_all(&root);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn advance_updates_the_offset_and_is_scoped_to_the_owner() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let mario = harness::seed_user(&test, admin, "mario").await;
    let root = temp_library_root();
    let library = seed_library(&test, admin, &root).await;
    let folder = seed_target_folder(&test, library, &root, "2024").await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);

    let repo = UploadSessionRepo::new(test.db());
    let session = repo
        .create(&ctx, new_session(folder, "foto.jpg", 1024))
        .await
        .unwrap();

    repo.advance(&ctx, session.id, 512).await.unwrap();
    let reloaded = repo.load_owned(&ctx, session.id).await.unwrap();
    assert_eq!(reloaded.received_bytes, 512);

    let stranger_ctx = AuthContext::user(mario, SystemRole::User);
    let result = repo.advance(&stranger_ctx, session.id, 999).await;
    assert!(matches!(result, Err(DbError::Forbidden)));
    // L'offset non deve muoversi dal tentativo altrui.
    let reloaded = repo.load_owned(&ctx, session.id).await.unwrap();
    assert_eq!(reloaded.received_bytes, 512);

    let _ = fs::remove_dir_all(&root);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn probing_someone_elses_session_is_forbidden_never_not_found() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let mario = harness::seed_user(&test, admin, "mario").await;
    let root = temp_library_root();
    let library = seed_library(&test, admin, &root).await;
    let folder = seed_target_folder(&test, library, &root, "2024").await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);

    let repo = UploadSessionRepo::new(test.db());
    let session = repo
        .create(&ctx, new_session(folder, "foto.jpg", 1024))
        .await
        .unwrap();

    let stranger_ctx = AuthContext::user(mario, SystemRole::User);
    assert!(matches!(
        repo.load_owned(&stranger_ctx, session.id).await,
        Err(DbError::Forbidden)
    ));

    // Un id davvero inesistente resta Forbidden per un non-admin, NotFound
    // solo per un admin — mai un oracolo di esistenza.
    let missing = keeppix_domain::UploadSessionId::new();
    assert!(matches!(
        repo.load_owned(&stranger_ctx, missing).await,
        Err(DbError::Forbidden)
    ));
    assert!(matches!(
        repo.load_owned(&ctx, missing).await,
        Err(DbError::NotFound)
    ));

    let _ = fs::remove_dir_all(&root);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn an_expired_session_is_cleaned_up_and_reported_as_gone() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let root = temp_library_root();
    let library = seed_library(&test, admin, &root).await;
    let folder = seed_target_folder(&test, library, &root, "2024").await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);

    let repo = UploadSessionRepo::new(test.db());
    let session = repo
        .create(&ctx, new_session(folder, "foto.jpg", 3))
        .await
        .unwrap();
    fs::write(&session.temp_path, b"abc").unwrap();

    sqlx::query(
        "UPDATE upload_sessions SET expires_at = now() - interval '1 second' WHERE id = $1",
    )
    .bind(session.id.as_uuid())
    .execute(test.db().pool())
    .await
    .unwrap();

    let result = repo.load_owned(&ctx, session.id).await;
    assert!(matches!(result, Err(DbError::Gone)));

    assert!(
        !session.temp_path.exists(),
        "il temporaneo di una sessione scaduta va ripulito subito"
    );
    let remaining: i64 = sqlx::query_scalar("SELECT count(*) FROM upload_sessions WHERE id = $1")
        .bind(session.id.as_uuid())
        .fetch_one(test.db().pool())
        .await
        .unwrap();
    assert_eq!(remaining, 0);

    let _ = fs::remove_dir_all(&root);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn fail_removes_both_the_session_row_and_its_temp_file() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let root = temp_library_root();
    let library = seed_library(&test, admin, &root).await;
    let folder = seed_target_folder(&test, library, &root, "2024").await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);

    let repo = UploadSessionRepo::new(test.db());
    let session = repo
        .create(&ctx, new_session(folder, "corrotto.jpg", 3))
        .await
        .unwrap();
    fs::write(&session.temp_path, b"abc").unwrap();

    repo.fail(&ctx, session.id).await.unwrap();

    assert!(!session.temp_path.exists());
    let remaining: i64 = sqlx::query_scalar("SELECT count(*) FROM upload_sessions WHERE id = $1")
        .bind(session.id.as_uuid())
        .fetch_one(test.db().pool())
        .await
        .unwrap();
    assert_eq!(remaining, 0);

    let _ = fs::remove_dir_all(&root);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn finalize_creates_a_new_asset_when_there_is_no_collision() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let root = temp_library_root();
    let library = seed_library(&test, admin, &root).await;
    let folder = seed_target_folder(&test, library, &root, "2024").await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);

    let repo = UploadSessionRepo::new(test.db());
    let session = repo
        .create(&ctx, new_session(folder, "foto.jpg", 3))
        .await
        .unwrap();
    fs::write(&session.temp_path, b"abc").unwrap();
    let hash = hash_file(&session.temp_path);

    let outcome = repo
        .finalize(&ctx, session.id, AssetKind::Image, hash)
        .await
        .unwrap();
    assert_eq!(outcome.collision, CollisionOutcome::Created);
    assert_eq!(outcome.filename, "foto.jpg");

    let final_path = root.join("2024").join("foto.jpg");
    assert!(final_path.is_file());
    assert_eq!(fs::read(&final_path).unwrap(), b"abc");
    assert!(!session.temp_path.exists());

    let sessions: i64 = sqlx::query_scalar("SELECT count(*) FROM upload_sessions")
        .fetch_one(test.db().pool())
        .await
        .unwrap();
    assert_eq!(sessions, 0);

    let stored_hash: Vec<u8> = sqlx::query_scalar("SELECT content_hash FROM assets WHERE id = $1")
        .bind(outcome.asset_id.as_uuid())
        .fetch_one(test.db().pool())
        .await
        .unwrap();
    assert_eq!(stored_hash, hash.to_vec());

    let _ = fs::remove_dir_all(&root);
}

/// Regressione per il difetto trovato in review: prima, il commit (riga
/// `assets` creata, sessione cancellata) avveniva **prima** del `rename()`.
/// Se il `rename()` falliva a quel punto, restava un asset che punta a un
/// file mai arrivato a destinazione, e nessuna sessione da cui recuperare il
/// temporaneo. Qui si forza il fallimento del `rename()` cancellando la
/// cartella target dal disco (ma non dal database) e si verifica che né la
/// riga `assets` né la cancellazione della sessione siano avvenute.
#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn finalize_leaves_no_asset_and_keeps_the_session_when_rename_fails() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let root = temp_library_root();
    let library = seed_library(&test, admin, &root).await;
    let folder = seed_target_folder(&test, library, &root, "2024").await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);

    let repo = UploadSessionRepo::new(test.db());
    let session = repo
        .create(&ctx, new_session(folder, "foto.jpg", 3))
        .await
        .unwrap();
    fs::write(&session.temp_path, b"abc").unwrap();
    let hash = hash_file(&session.temp_path);

    fs::remove_dir_all(root.join("2024")).unwrap();

    let result = repo
        .finalize(&ctx, session.id, AssetKind::Image, hash)
        .await;
    assert!(matches!(result, Err(DbError::Io(_))));

    assert!(
        session.temp_path.exists(),
        "un rename fallito non deve far sparire il temporaneo"
    );
    let sessions: i64 = sqlx::query_scalar("SELECT count(*) FROM upload_sessions WHERE id = $1")
        .bind(session.id.as_uuid())
        .fetch_one(test.db().pool())
        .await
        .unwrap();
    assert_eq!(
        sessions, 1,
        "la sessione deve restare per un retry, mai cancellata prima di un rename riuscito"
    );
    let assets: i64 = sqlx::query_scalar("SELECT count(*) FROM assets WHERE filename = 'foto.jpg'")
        .fetch_one(test.db().pool())
        .await
        .unwrap();
    assert_eq!(
        assets, 0,
        "mai un asset senza il file corrispondente a destinazione"
    );

    let _ = fs::remove_dir_all(&root);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn finalize_skips_a_byte_identical_duplicate_without_a_second_file() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let root = temp_library_root();
    let library = seed_library(&test, admin, &root).await;
    let folder = seed_target_folder(&test, library, &root, "2024").await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);

    let existing_path = root.join("2024").join("foto.jpg");
    fs::write(&existing_path, b"stesso-contenuto").unwrap();
    let hash = hash_file(&existing_path);
    let existing_asset_id = seed_existing_asset(&test, folder, "foto.jpg", hash).await;

    let repo = UploadSessionRepo::new(test.db());
    let session = repo
        .create(&ctx, new_session(folder, "foto.jpg", 16))
        .await
        .unwrap();
    fs::write(&session.temp_path, b"stesso-contenuto").unwrap();

    let outcome = repo
        .finalize(&ctx, session.id, AssetKind::Image, hash)
        .await
        .unwrap();
    assert_eq!(
        outcome.collision,
        CollisionOutcome::SkippedDuplicate { existing_asset_id }
    );

    assert!(
        !session.temp_path.exists(),
        "il temporaneo del duplicato va rimosso"
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM assets WHERE filename = 'foto.jpg'")
        .fetch_one(test.db().pool())
        .await
        .unwrap();
    assert_eq!(count, 1, "nessun secondo file per un duplicato esatto");

    let _ = fs::remove_dir_all(&root);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn finalize_renames_on_same_name_different_hash_never_overwriting() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let root = temp_library_root();
    let library = seed_library(&test, admin, &root).await;
    let folder = seed_target_folder(&test, library, &root, "2024").await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);

    let existing_path = root.join("2024").join("foto.jpg");
    fs::write(&existing_path, b"originale").unwrap();
    let existing_hash = hash_file(&existing_path);
    seed_existing_asset(&test, folder, "foto.jpg", existing_hash).await;

    let repo = UploadSessionRepo::new(test.db());
    let session = repo
        .create(&ctx, new_session(folder, "foto.jpg", 16))
        .await
        .unwrap();
    fs::write(&session.temp_path, b"contenuto diverso").unwrap();
    let new_hash = hash_file(&session.temp_path);
    assert_ne!(existing_hash, new_hash);

    let outcome = repo
        .finalize(&ctx, session.id, AssetKind::Image, new_hash)
        .await
        .unwrap();
    assert_eq!(
        outcome.collision,
        CollisionOutcome::RenamedTo("foto_1.jpg".to_owned())
    );
    assert_eq!(outcome.filename, "foto_1.jpg");

    assert_eq!(
        fs::read(&existing_path).unwrap(),
        b"originale",
        "il file esistente non va mai sovrascritto"
    );
    let renamed_path = root.join("2024").join("foto_1.jpg");
    assert_eq!(fs::read(&renamed_path).unwrap(), b"contenuto diverso");

    let names: Vec<String> =
        sqlx::query_scalar("SELECT filename FROM assets WHERE folder_id = $1 ORDER BY filename")
            .bind(folder.as_uuid())
            .fetch_all(test.db().pool())
            .await
            .unwrap();
    assert_eq!(names, vec!["foto.jpg".to_owned(), "foto_1.jpg".to_owned()]);

    let _ = fs::remove_dir_all(&root);
}
