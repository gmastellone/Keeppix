mod harness;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use chrono::{TimeZone, Utc};
use harness::TestServer;
use keeppix_db::{
    AssetRepo, FolderRepo, LibraryRepo, NewGrant, ObjectType, PermissionRepo, SubjectType,
};
use keeppix_domain::{
    AssetId, AssetKind, AssetName, AuthContext, FolderId, LibraryId, NewAsset, NewLibrary,
    ObjectRole, SystemRole, UserId,
};
use serde_json::json;

#[allow(clippy::expect_used, clippy::unwrap_used)]
async fn setup_admin(server: &TestServer) -> UserId {
    let response = server
        .client
        .post(server.url("/api/v1/setup"))
        .json(&json!({
            "username": "giovanni",
            "display_name": "Giovanni",
            "password": "correct horse battery staple"
        }))
        .send()
        .await
        .expect("richiesta di setup");
    let body: serde_json::Value = response.json().await.expect("corpo JSON");
    body["user"]["id"]
        .as_str()
        .expect("id utente")
        .parse()
        .expect("uuid valido")
}

#[allow(clippy::expect_used, clippy::unwrap_used)]
async fn seed_asset(server: &TestServer, admin: UserId, root: &std::path::Path) -> AssetId {
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let library = LibraryRepo::new(&server.db)
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
        .expect("libreria")
        .id;
    let folder: FolderId = FolderRepo::new(&server.db)
        .ensure_path(library, &["2024"])
        .await
        .expect("cartella")
        .id;
    fs::create_dir_all(root.join("2024")).expect("cartella su disco");
    fs::write(root.join("2024").join("foto.jpg"), b"contenuto").expect("file su disco");

    AssetRepo::new(&server.db)
        .upsert_discovered(NewAsset {
            folder_id: folder,
            filename: AssetName::parse("foto.jpg").expect("nome"),
            size_bytes: 9,
            mtime: Utc.with_ymd_and_hms(2024, 6, 1, 12, 0, 0).unwrap(),
            inode: None,
            kind: AssetKind::Image,
        })
        .await
        .expect("asset")
        .unwrap()
        .id
}

#[allow(clippy::expect_used, clippy::unwrap_used)]
fn temp_root() -> PathBuf {
    let root = std::env::temp_dir().join(format!("keeppix-api-trash-{}", uuid::Uuid::now_v7()));
    fs::create_dir_all(&root).expect("radice di test");
    root
}

/// Crea un utente e autentica `server.client` con le sue credenziali —
/// sostituisce il cookie di sessione dell'admin, come nella prova di
/// probing già presente in questo file.
#[allow(clippy::expect_used, clippy::unwrap_used)]
async fn create_and_login(server: &TestServer, admin: UserId, username: &str) -> UserId {
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let user_id = keeppix_db::UserRepo::new(&server.db)
        .create(
            &ctx,
            keeppix_domain::NewUser {
                username: keeppix_domain::Username::parse(username).expect("username"),
                email: None,
                display_name: username.to_owned(),
                password_hash: keeppix_domain::hash_password(
                    &keeppix_domain::Password::parse("correct horse battery staple")
                        .expect("password"),
                )
                .expect("hash")
                .as_str()
                .to_owned(),
                role: SystemRole::User,
            },
        )
        .await
        .expect("utente")
        .id;

    let login = server
        .client
        .post(server.url("/api/v1/auth/login"))
        .json(&json!({ "username": username, "password": "correct horse battery staple" }))
        .send()
        .await
        .expect("login");
    assert_eq!(login.status(), 200);

    user_id
}

/// Libreria con una cartella e `filenames.len()` asset dentro, tutti presenti
/// su disco. Ritorna id di libreria, cartella e degli asset (nello stesso
/// ordine di `filenames`), più i percorsi assoluti dei file.
#[allow(clippy::expect_used, clippy::unwrap_used)]
async fn seed_library_with_assets(
    server: &TestServer,
    admin: UserId,
    root: &std::path::Path,
    subdir: &str,
    filenames: &[&str],
) -> (LibraryId, FolderId, Vec<AssetId>, Vec<PathBuf>) {
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let library = LibraryRepo::new(&server.db)
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
        .expect("libreria")
        .id;
    let folder: FolderId = FolderRepo::new(&server.db)
        .ensure_path(library, &[subdir])
        .await
        .expect("cartella")
        .id;
    fs::create_dir_all(root.join(subdir)).expect("cartella su disco");

    let mut asset_ids = Vec::new();
    let mut paths = Vec::new();
    for filename in filenames {
        let path = root.join(subdir).join(filename);
        fs::write(&path, b"contenuto").expect("file su disco");
        let asset_id = AssetRepo::new(&server.db)
            .upsert_discovered(NewAsset {
                folder_id: folder,
                filename: AssetName::parse(filename).expect("nome"),
                size_bytes: 9,
                mtime: Utc.with_ymd_and_hms(2024, 6, 1, 12, 0, 0).unwrap(),
                inode: None,
                kind: AssetKind::Image,
            })
            .await
            .expect("asset")
            .unwrap()
            .id;
        asset_ids.push(asset_id);
        paths.push(path);
    }

    (library, folder, asset_ids, paths)
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn deleting_to_trash_then_restoring_round_trips_the_file() {
    let server = TestServer::start().await;
    let admin = setup_admin(&server).await;
    let root = temp_root();
    let asset_id = seed_asset(&server, admin, &root).await;
    let original = root.join("2024").join("foto.jpg");

    let response = server
        .client
        .delete(server.url(&format!("/api/v1/assets/{asset_id}")))
        .json(&json!({ "disk_action": "moved_to_trash" }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 204);
    assert!(!original.exists(), "il file è passato nel cestino");

    let response = server
        .client
        .post(server.url(&format!("/api/v1/assets/{asset_id}/restore")))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 204);
    assert!(original.is_file(), "il file torna al percorso originale");

    let _ = fs::remove_dir_all(&root);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn an_unrecognised_disk_action_is_a_bad_request() {
    let server = TestServer::start().await;
    let admin = setup_admin(&server).await;
    let root = temp_root();
    let asset_id = seed_asset(&server, admin, &root).await;

    let response = server
        .client
        .delete(server.url(&format!("/api/v1/assets/{asset_id}")))
        .json(&json!({ "disk_action": "delete_forever_please" }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 400);
    let problem: serde_json::Value = response.json().await.unwrap();
    assert_eq!(problem["type"], "keeppix/invalid-disk-action");

    let _ = fs::remove_dir_all(&root);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn probing_someone_elses_asset_is_forbidden_not_found() {
    let server = TestServer::start().await;
    let admin = setup_admin(&server).await;
    let root = temp_root();
    let asset_id = seed_asset(&server, admin, &root).await;

    // Un secondo utente, autenticato ma senza alcuna libreria propria: non
    // vede l'asset dell'admin.
    keeppix_db::UserRepo::new(&server.db)
        .create(
            &AuthContext::user(admin, SystemRole::Admin),
            keeppix_domain::NewUser {
                username: keeppix_domain::Username::parse("mario").unwrap(),
                email: None,
                display_name: "Mario".to_owned(),
                password_hash: keeppix_domain::hash_password(
                    &keeppix_domain::Password::parse("correct horse battery staple").unwrap(),
                )
                .unwrap()
                .as_str()
                .to_owned(),
                role: SystemRole::User,
            },
        )
        .await
        .unwrap();

    // `server.client` porta un cookie store: accedere come mario sostituisce
    // il cookie di sessione dell'admin con il suo, sullo stesso client.
    let login = server
        .client
        .post(server.url("/api/v1/auth/login"))
        .json(&json!({ "username": "mario", "password": "correct horse battery staple" }))
        .send()
        .await
        .unwrap();
    assert_eq!(login.status(), 200);

    let response = server
        .client
        .delete(server.url(&format!("/api/v1/assets/{asset_id}")))
        .json(&json!({ "disk_action": "kept" }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 403);

    let _ = fs::remove_dir_all(&root);
}

/// Task 4: `purged` è la seconda cancello, più stretto di `assert_visible` —
/// un editor vede e modifica, ma non può distruggere dal disco. Sul batch
/// questo deve rifiutare l'intero lotto **prima** di toccare qualunque file,
/// non lasciare un'eliminazione a metà.
#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn batch_delete_purged_by_a_non_owner_editor_rejects_the_whole_batch_untouched() {
    let server = TestServer::start().await;
    let admin = setup_admin(&server).await;
    let root = temp_root();
    let (_library, folder, asset_ids, paths) =
        seed_library_with_assets(&server, admin, &root, "2024", &["a.jpg", "b.jpg"]).await;

    let mario = create_and_login(&server, admin, "mario").await;
    PermissionRepo::new(&server.db)
        .grant(
            &AuthContext::user(admin, SystemRole::Admin),
            NewGrant {
                subject: SubjectType::User,
                subject_id: mario.as_uuid(),
                object: ObjectType::Folder,
                object_id: folder.as_uuid(),
                role: ObjectRole::Editor,
                inherit: true,
            },
        )
        .await
        .expect("permesso editor");

    let response = server
        .client
        .post(server.url("/api/v1/assets/batch/delete"))
        .json(&json!({ "asset_ids": asset_ids, "disk_action": "purged" }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 403);

    for path in &paths {
        assert!(
            path.exists(),
            "nessun file va toccato quando l'autorizzazione fallisce per il lotto"
        );
    }

    let _ = fs::remove_dir_all(&root);
}

/// Task 4: `choose` per elemento — un file già assente non deve bloccare gli
/// altri. `moved_to_trash` non è tollerante come `purged` sul file mancante
/// (`std::fs::rename` fallisce sul serio), quindi è la scelta giusta per
/// osservare il fallimento.
#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn batch_delete_partial_success_when_one_file_is_already_missing() {
    let server = TestServer::start().await;
    let admin = setup_admin(&server).await;
    let root = temp_root();
    let (_library, _folder, asset_ids, paths) = seed_library_with_assets(
        &server,
        admin,
        &root,
        "2024",
        &["present.jpg", "missing.jpg"],
    )
    .await;

    // Il file del secondo asset scompare dal disco senza che il database lo
    // sappia — come un utente che lo cancella da fuori Keeppix.
    fs::remove_file(&paths[1]).expect("rimozione manuale del file");

    let response = server
        .client
        .post(server.url("/api/v1/assets/batch/delete"))
        .json(&json!({ "asset_ids": asset_ids, "disk_action": "moved_to_trash" }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    let body: serde_json::Value = response.json().await.unwrap();
    let succeeded: Vec<String> = body["succeeded"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect();
    assert_eq!(succeeded, vec![asset_ids[0].to_string()]);

    let failed = body["failed"].as_array().unwrap();
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0]["id"], asset_ids[1].to_string());
    assert_eq!(failed[0]["reason"], "file-missing");

    assert!(
        !paths[0].exists(),
        "il primo file è passato nel cestino regolarmente"
    );

    let _ = fs::remove_dir_all(&root);
}

/// Task 4: caso reale più probabile secondo il brief — una cartella del
/// cestino non scrivibile. I due asset vivono in due sottocartelle diverse
/// (`2024/ok`, `2024/blocked`), così il cestino li smista in due directory
/// separate: solo quella di `blocked` è resa di sola lettura, quindi solo
/// quell'asset deve fallire con `permission-denied`.
#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn batch_delete_partial_success_when_the_trash_folder_is_not_writable() {
    let server = TestServer::start().await;
    let admin = setup_admin(&server).await;
    let root = temp_root();

    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let library = LibraryRepo::new(&server.db)
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
        .expect("libreria")
        .id;

    let mut asset_ids = Vec::new();
    for (subdir, filename) in [("2024/ok", "ok.jpg"), ("2024/blocked", "blocked.jpg")] {
        let folder = FolderRepo::new(&server.db)
            .ensure_path(library, &subdir.split('/').collect::<Vec<_>>())
            .await
            .expect("cartella")
            .id;
        fs::create_dir_all(root.join(subdir)).expect("cartella su disco");
        fs::write(root.join(subdir).join(filename), b"contenuto").expect("file su disco");
        let asset_id = AssetRepo::new(&server.db)
            .upsert_discovered(NewAsset {
                folder_id: folder,
                filename: AssetName::parse(filename).expect("nome"),
                size_bytes: 9,
                mtime: Utc.with_ymd_and_hms(2024, 6, 1, 12, 0, 0).unwrap(),
                inode: None,
                kind: AssetKind::Image,
            })
            .await
            .expect("asset")
            .unwrap()
            .id;
        asset_ids.push(asset_id);
    }

    // La sottocartella di cestino di `blocked` esiste già ma di sola
    // lettura: `create_dir_all` la trova e non tenta scritture, ma il
    // successivo `rename()` che ci sposta dentro il file deve fallire.
    let blocked_trash_dir = root.join(".keeppix-trash").join("2024").join("blocked");
    fs::create_dir_all(&blocked_trash_dir).expect("cartella di cestino preesistente");
    let mut perms = fs::metadata(&blocked_trash_dir).unwrap().permissions();
    perms.set_mode(0o555);
    fs::set_permissions(&blocked_trash_dir, perms).expect("chmod sola lettura");

    let response = server
        .client
        .post(server.url("/api/v1/assets/batch/delete"))
        .json(&json!({ "asset_ids": asset_ids, "disk_action": "moved_to_trash" }))
        .send()
        .await
        .unwrap();

    // Ripristina i permessi prima di ogni assert/panic: altrimenti la pulizia
    // della radice temporanea a fine test fallirebbe a sua volta.
    let mut restored = fs::metadata(&blocked_trash_dir).unwrap().permissions();
    restored.set_mode(0o755);
    fs::set_permissions(&blocked_trash_dir, restored).expect("ripristino permessi");

    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();

    let succeeded: Vec<String> = body["succeeded"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect();
    assert_eq!(succeeded, vec![asset_ids[0].to_string()]);

    let failed = body["failed"].as_array().unwrap();
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0]["id"], asset_ids[1].to_string());
    assert_eq!(failed[0]["reason"], "permission-denied");

    let _ = fs::remove_dir_all(&root);
}
