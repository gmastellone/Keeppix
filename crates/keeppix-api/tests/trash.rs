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
        .expect("setup request");
    let body: serde_json::Value = response.json().await.expect("JSON body");
    body["user"]["id"]
        .as_str()
        .expect("user id")
        .parse()
        .expect("valid uuid")
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
        .expect("library")
        .id;
    let folder: FolderId = FolderRepo::new(&server.db)
        .ensure_path(library, &["2024"])
        .await
        .expect("folder")
        .id;
    fs::create_dir_all(root.join("2024")).expect("folder on disk");
    fs::write(root.join("2024").join("foto.jpg"), b"content").expect("file on disk");

    AssetRepo::new(&server.db)
        .upsert_discovered(NewAsset {
            folder_id: folder,
            filename: AssetName::parse("foto.jpg").expect("name"),
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
    fs::create_dir_all(&root).expect("test root");
    root
}

/// Creates a user and authenticates `server.client` with their
/// credentials — replaces the admin's session cookie, same as the
/// probing test already in this file.
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
        .expect("user")
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

/// A library with one folder and `filenames.len()` assets inside, all
/// present on disk. Returns the library and folder ids and the assets'
/// ids (in the same order as `filenames`), plus the files' absolute
/// paths.
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
        .expect("library")
        .id;
    let folder: FolderId = FolderRepo::new(&server.db)
        .ensure_path(library, &[subdir])
        .await
        .expect("folder")
        .id;
    fs::create_dir_all(root.join(subdir)).expect("folder on disk");

    let mut asset_ids = Vec::new();
    let mut paths = Vec::new();
    for filename in filenames {
        let path = root.join(subdir).join(filename);
        fs::write(&path, b"content").expect("file on disk");
        let asset_id = AssetRepo::new(&server.db)
            .upsert_discovered(NewAsset {
                folder_id: folder,
                filename: AssetName::parse(filename).expect("name"),
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
    assert!(!original.exists(), "the file moved into the trash");

    let response = server
        .client
        .post(server.url(&format!("/api/v1/assets/{asset_id}/restore")))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 204);
    assert!(original.is_file(), "the file returns to its original path");

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

    // A second user, authenticated but with no library of their own:
    // can't see the admin's asset.
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

    // `server.client` carries a cookie store: logging in as mario replaces
    // the admin's session cookie with his, on the same client.
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

/// `purged` is the second gate, stricter than `assert_visible` — an
/// editor can see and modify, but cannot destroy from disk. On a batch
/// this must reject the whole lot **before** touching any file, not leave
/// a half-finished deletion.
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
        .expect("editor permission");

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
            "no file should be touched when authorization fails for the batch"
        );
    }

    let _ = fs::remove_dir_all(&root);
}

/// Per-item `choose` — a file that's already missing must not block the
/// others. `moved_to_trash` is not as tolerant of a missing file as
/// `purged` is (`std::fs::rename` genuinely fails), so it's the right
/// choice to observe the failure.
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

    // The second asset's file disappears from disk without the database
    // knowing — like a user deleting it from outside Keeppix.
    fs::remove_file(&paths[1]).expect("manual file removal");

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
        "the first file moved into the trash normally"
    );

    let _ = fs::remove_dir_all(&root);
}

/// The realistic scenario: a trash folder that's not writable. The two
/// assets live in two different subfolders (`2024/ok`, `2024/blocked`),
/// so trash sorts them into two separate directories: only `blocked`'s is
/// made read-only, so only that asset should fail with
/// `permission-denied`.
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
        .expect("library")
        .id;

    let mut asset_ids = Vec::new();
    for (subdir, filename) in [("2024/ok", "ok.jpg"), ("2024/blocked", "blocked.jpg")] {
        let folder = FolderRepo::new(&server.db)
            .ensure_path(library, &subdir.split('/').collect::<Vec<_>>())
            .await
            .expect("folder")
            .id;
        fs::create_dir_all(root.join(subdir)).expect("folder on disk");
        fs::write(root.join(subdir).join(filename), b"content").expect("file on disk");
        let asset_id = AssetRepo::new(&server.db)
            .upsert_discovered(NewAsset {
                folder_id: folder,
                filename: AssetName::parse(filename).expect("name"),
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

    // `blocked`'s trash subfolder already exists but is read-only:
    // `create_dir_all` finds it and doesn't attempt any writes, but the
    // subsequent `rename()` that moves the file into it must fail.
    let blocked_trash_dir = root.join(".keeppix-trash").join("2024").join("blocked");
    fs::create_dir_all(&blocked_trash_dir).expect("pre-existing trash folder");
    let mut perms = fs::metadata(&blocked_trash_dir).unwrap().permissions();
    perms.set_mode(0o555);
    fs::set_permissions(&blocked_trash_dir, perms).expect("read-only chmod");

    let response = server
        .client
        .post(server.url("/api/v1/assets/batch/delete"))
        .json(&json!({ "asset_ids": asset_ids, "disk_action": "moved_to_trash" }))
        .send()
        .await
        .unwrap();

    // Restore permissions before any assert/panic: otherwise cleaning up
    // the temp root at the end of the test would fail too.
    let mut restored = fs::metadata(&blocked_trash_dir).unwrap().permissions();
    restored.set_mode(0o755);
    fs::set_permissions(&blocked_trash_dir, restored).expect("permission restore");

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
