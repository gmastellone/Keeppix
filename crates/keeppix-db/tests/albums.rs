#![allow(clippy::unwrap_used, clippy::expect_used)]

mod harness;

use chrono::{TimeZone, Utc};
use harness::TestDb;
use keeppix_db::{
    AlbumRepo, AssetRepo, DbError, FolderRepo, LibraryRepo, NewAlbum, ObjectType, PermissionRepo,
    SearchNode, SubjectType,
};
use keeppix_domain::{
    AlbumId, AssetId, AssetKind, AssetName, AssetStatus, AuthContext, FolderId, LibraryId,
    NewAsset, NewLibrary, ObjectRole, SystemRole, UserId,
};

async fn seed_library(test: &TestDb, owner: UserId, path: &str) -> LibraryId {
    LibraryRepo::new(test.db())
        .create(
            &AuthContext::user(owner, SystemRole::Admin),
            NewLibrary {
                name: "Foto".to_owned(),
                owner_id: owner,
                root_path: std::path::PathBuf::from(path),
                exclude_patterns: vec![],
            },
        )
        .await
        .unwrap()
        .id
}

async fn seed_folder(test: &TestDb, library: LibraryId, name: &str) -> FolderId {
    FolderRepo::new(test.db())
        .ensure_path(library, &[name])
        .await
        .unwrap()
        .id
}

async fn index_photo(test: &TestDb, folder: FolderId, name: &str) -> AssetId {
    let repo = AssetRepo::new(test.db());
    let asset = repo
        .upsert_discovered(NewAsset {
            folder_id: folder,
            filename: AssetName::parse(name).unwrap(),
            size_bytes: 10,
            mtime: Utc.with_ymd_and_hms(2024, 7, 1, 0, 0, 0).unwrap(),
            inode: Some(1),
            kind: AssetKind::Image,
        })
        .await
        .unwrap()
        .unwrap();
    repo.set_indexed(
        asset.id,
        Utc.with_ymd_and_hms(2024, 7, 2, 12, 0, 0).unwrap(),
        1,
        1,
    )
    .await
    .unwrap();
    asset.id
}

async fn grant_album(
    test: &TestDb,
    granter: UserId,
    recipient: UserId,
    album_id: AlbumId,
    role: ObjectRole,
) {
    PermissionRepo::new(test.db())
        .grant(
            &AuthContext::user(granter, SystemRole::Admin),
            keeppix_db::NewGrant {
                subject: SubjectType::User,
                subject_id: recipient.as_uuid(),
                object: ObjectType::Album,
                object_id: album_id.as_uuid(),
                role,
                inherit: true,
            },
        )
        .await
        .unwrap();
}

/// A photo can live in many albums without being duplicated on disk.
#[tokio::test]
async fn an_asset_can_live_in_many_albums() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);

    let lib = seed_library(&test, admin, "/mnt/nas").await;
    let folder = seed_folder(&test, lib, "2024").await;
    let photo = index_photo(&test, folder, "photo.jpg").await;

    let repo = AlbumRepo::new(test.db());
    let album_a = repo
        .create(
            &ctx,
            NewAlbum {
                name: "Vacanze".into(),
                description: String::new(),
                rule: None,
            },
        )
        .await
        .unwrap();
    let album_b = repo
        .create(
            &ctx,
            NewAlbum {
                name: "Famiglia".into(),
                description: String::new(),
                rule: None,
            },
        )
        .await
        .unwrap();

    repo.add_asset(&ctx, album_a.id, photo).await.unwrap();
    repo.add_asset(&ctx, album_b.id, photo).await.unwrap();

    let in_a = repo.list_assets(&ctx, album_a.id).await.unwrap();
    let in_b = repo.list_assets(&ctx, album_b.id).await.unwrap();
    assert_eq!(in_a.len(), 1);
    assert_eq!(in_b.len(), 1);
    assert_eq!(in_a[0].asset.id, photo);
    assert_eq!(in_b[0].asset.id, photo);

    // The asset exists exactly once in the assets table.
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM assets WHERE id = $1")
        .bind(photo.as_uuid())
        .fetch_one(test.db().pool())
        .await
        .unwrap();
    assert_eq!(count, 1, "the asset must exist exactly once");
}

/// Removing a photo from an album does not delete the photo.
#[tokio::test]
async fn removing_from_an_album_does_not_delete_the_asset() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);

    let lib = seed_library(&test, admin, "/mnt/nas").await;
    let folder = seed_folder(&test, lib, "2024").await;
    let photo = index_photo(&test, folder, "foto.jpg").await;

    let repo = AlbumRepo::new(test.db());
    let album = repo
        .create(
            &ctx,
            NewAlbum {
                name: "Test".into(),
                description: String::new(),
                rule: None,
            },
        )
        .await
        .unwrap();

    repo.add_asset(&ctx, album.id, photo).await.unwrap();
    repo.remove_asset(&ctx, album.id, photo).await.unwrap();

    // The album is empty.
    let in_album = repo.list_assets(&ctx, album.id).await.unwrap();
    assert!(in_album.is_empty(), "the album must be empty after removal");

    // The asset still exists.
    let asset = AssetRepo::new(test.db()).find_by_id(&ctx, photo).await;
    assert!(
        asset.is_ok(),
        "the asset must still exist after removal from the album"
    );
    assert_eq!(
        AssetRepo::new(test.db())
            .count_by_status(&ctx, AssetStatus::Indexed)
            .await
            .unwrap(),
        1,
        "the asset must still be indexed"
    );
}

/// Sharing an album with a user grants them visibility of the assets in
/// that album, but does not expose the folder those files actually live in.
#[tokio::test]
async fn sharing_an_album_grants_its_assets_but_not_their_folders() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let mario = harness::seed_user(&test, admin, "mario").await;

    let owner_ctx = AuthContext::user(admin, SystemRole::Admin);
    let mario_ctx = AuthContext::user(mario, SystemRole::User);

    let lib = seed_library(&test, admin, "/mnt/nas/foto").await;
    let folder = seed_folder(&test, lib, "Privato").await;
    let photo = index_photo(&test, folder, "vacanze.jpg").await;

    // Before sharing: mario sees nothing.
    assert_eq!(
        AssetRepo::new(test.db())
            .count_by_status(&mario_ctx, AssetStatus::Indexed)
            .await
            .unwrap(),
        0,
        "before sharing, mario must not see any asset"
    );
    assert!(
        AssetRepo::new(test.db())
            .find_by_id(&mario_ctx, photo)
            .await
            .is_err(),
        "find_by_id must return Forbidden before sharing"
    );

    // Create the album and add the photo to it.
    let repo = AlbumRepo::new(test.db());
    let album = repo
        .create(
            &owner_ctx,
            NewAlbum {
                name: "Condiviso".into(),
                description: String::new(),
                rule: None,
            },
        )
        .await
        .unwrap();
    repo.add_asset(&owner_ctx, album.id, photo).await.unwrap();

    // Share the album with mario.
    grant_album(&test, admin, mario, album.id, ObjectRole::Viewer).await;

    // Now mario sees the photo via the grant on the album.
    assert_eq!(
        AssetRepo::new(test.db())
            .count_by_status(&mario_ctx, AssetStatus::Indexed)
            .await
            .unwrap(),
        1,
        "after sharing, mario must see the asset"
    );
    assert!(
        AssetRepo::new(test.db())
            .find_by_id(&mario_ctx, photo)
            .await
            .is_ok(),
        "find_by_id must succeed after sharing"
    );

    // But mario does not see the "Privato" folder.
    let tree = FolderRepo::new(test.db()).tree(&mario_ctx).await.unwrap();
    assert!(
        tree.is_empty(),
        "the folder must not be visible: mario has access via the album, not via the folder"
    );
}

/// Deleting an album deletes none of the photos it contains.
#[tokio::test]
async fn deleting_an_album_deletes_no_photo() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);

    let lib = seed_library(&test, admin, "/mnt/nas").await;
    let folder = seed_folder(&test, lib, "2024").await;
    let p1 = index_photo(&test, folder, "a.jpg").await;
    let p2 = index_photo(&test, folder, "b.jpg").await;

    let repo = AlbumRepo::new(test.db());
    let album = repo
        .create(
            &ctx,
            NewAlbum {
                name: "Da eliminare".into(),
                description: String::new(),
                rule: None,
            },
        )
        .await
        .unwrap();
    repo.add_asset(&ctx, album.id, p1).await.unwrap();
    repo.add_asset(&ctx, album.id, p2).await.unwrap();

    repo.delete(&ctx, album.id).await.unwrap();

    // The photos are still there.
    assert!(AssetRepo::new(test.db()).find_by_id(&ctx, p1).await.is_ok());
    assert!(AssetRepo::new(test.db()).find_by_id(&ctx, p2).await.is_ok());
    assert_eq!(
        AssetRepo::new(test.db())
            .count_by_status(&ctx, AssetStatus::Indexed)
            .await
            .unwrap(),
        2,
        "2 indexed assets must remain after the album is deleted"
    );

    // The album is gone.
    let albums = repo.list(&ctx).await.unwrap();
    assert!(albums.is_empty(), "the album must be deleted");
}

/// A user without permission on the album gets Forbidden — not NotFound.
#[tokio::test]
async fn probing_an_album_without_permission_is_forbidden() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let mario = harness::seed_user(&test, admin, "mario").await;
    let ctx_admin = AuthContext::user(admin, SystemRole::Admin);
    let ctx_mario = AuthContext::user(mario, SystemRole::User);

    let lib = seed_library(&test, admin, "/mnt/nas").await;
    let folder = seed_folder(&test, lib, "2024").await;
    let photo = index_photo(&test, folder, "foto.jpg").await;

    let repo = AlbumRepo::new(test.db());
    let album = repo
        .create(
            &ctx_admin,
            NewAlbum {
                name: "Privato".into(),
                description: String::new(),
                rule: None,
            },
        )
        .await
        .unwrap();
    repo.add_asset(&ctx_admin, album.id, photo).await.unwrap();

    // Mario has no permission at all: he must get Forbidden.
    assert!(matches!(
        repo.get(&ctx_mario, album.id).await,
        Err(DbError::Forbidden)
    ));
    assert!(matches!(
        repo.list_assets(&ctx_mario, album.id).await,
        Err(DbError::Forbidden)
    ));

    // The album must not appear in mario's list.
    let list = repo.list(&ctx_mario).await.unwrap();
    assert!(list.is_empty());
}

/// An album without a `rule` cannot be refreshed: this is not an
/// authorization defect, it's simply that there is nothing to re-run.
#[tokio::test]
async fn refreshing_an_album_without_a_rule_returns_none() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);

    let repo = AlbumRepo::new(test.db());
    let album = repo
        .create(
            &ctx,
            NewAlbum {
                name: "Senza filtro".into(),
                description: String::new(),
                rule: None,
            },
        )
        .await
        .unwrap();

    let outcome = repo.refresh(&ctx, album.id).await.unwrap();
    assert!(
        outcome.is_none(),
        "an album without a rule must not produce a refresh outcome"
    );
}

/// Refresh re-applies the `rule` the album was created with: photos that
/// match are added, ones that no longer match (or that were added by hand
/// outside the filter) are removed. A second refresh with no catalog
/// changes must not produce duplicates or movements.
#[tokio::test]
async fn refresh_adds_matches_and_removes_non_matches_and_is_idempotent() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);

    let lib = seed_library(&test, admin, "/mnt/nas").await;
    let folder = seed_folder(&test, lib, "2024").await;
    let jpeg = index_photo(&test, folder, "a.jpg").await;
    let another_jpeg = index_photo(&test, folder, "b.jpg").await;

    // A video, outside the type:image filter.
    let video_repo = AssetRepo::new(test.db());
    let video = video_repo
        .upsert_discovered(NewAsset {
            folder_id: folder,
            filename: AssetName::parse("c.mov").unwrap(),
            size_bytes: 10,
            mtime: Utc.with_ymd_and_hms(2024, 7, 1, 0, 0, 0).unwrap(),
            inode: Some(2),
            kind: AssetKind::Video,
        })
        .await
        .unwrap()
        .unwrap();
    video_repo
        .set_indexed(
            video.id,
            Utc.with_ymd_and_hms(2024, 7, 2, 12, 0, 0).unwrap(),
            1,
            1,
        )
        .await
        .unwrap();

    let repo = AlbumRepo::new(test.db());
    let album = repo
        .create(
            &ctx,
            NewAlbum {
                name: "Solo foto".into(),
                description: String::new(),
                rule: Some(SearchNode::Type {
                    value: "image".to_owned(),
                }),
            },
        )
        .await
        .unwrap();

    // Added by hand, outside the filter: refresh must remove it.
    repo.add_asset(&ctx, album.id, video.id).await.unwrap();

    let refresh = repo
        .refresh(&ctx, album.id)
        .await
        .unwrap()
        .expect("the album has a rule");
    let mut added: Vec<AssetId> = refresh.added.clone();
    added.sort_by_key(AssetId::as_uuid);
    let mut expected_added = vec![jpeg, another_jpeg];
    expected_added.sort_by_key(AssetId::as_uuid);
    assert_eq!(added, expected_added, "the two photos must be added");
    assert_eq!(
        refresh.removed,
        vec![video.id],
        "the video outside the filter must be removed"
    );

    let members = repo.list_assets(&ctx, album.id).await.unwrap();
    let mut member_ids: Vec<AssetId> = members.iter().map(|m| m.asset.id).collect();
    member_ids.sort_by_key(AssetId::as_uuid);
    assert_eq!(member_ids, expected_added);

    // rule_run_at has been written.
    let rule_run_at: Option<chrono::DateTime<Utc>> =
        sqlx::query_scalar("SELECT rule_run_at FROM albums WHERE id = $1")
            .bind(album.id.as_uuid())
            .fetch_one(test.db().pool())
            .await
            .unwrap();
    assert!(rule_run_at.is_some(), "rule_run_at must be set");

    // A second refresh, with no catalog changes, must add or remove
    // nothing: idempotence.
    let second = repo.refresh(&ctx, album.id).await.unwrap().unwrap();
    assert!(
        second.added.is_empty(),
        "the second refresh must not add duplicates"
    );
    assert!(
        second.removed.is_empty(),
        "the second refresh must not remove anything already consistent"
    );
}

/// A user without permission on the album gets `Forbidden` — never
/// `NotFound` — for refresh too, the same invariant as the album's other
/// endpoints.
#[tokio::test]
async fn refreshing_a_foreign_album_is_forbidden() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let mario = harness::seed_user(&test, admin, "mario").await;
    let ctx_admin = AuthContext::user(admin, SystemRole::Admin);
    let ctx_mario = AuthContext::user(mario, SystemRole::User);

    let repo = AlbumRepo::new(test.db());
    let album = repo
        .create(
            &ctx_admin,
            NewAlbum {
                name: "Privato".into(),
                description: String::new(),
                rule: Some(SearchNode::Type {
                    value: "image".to_owned(),
                }),
            },
        )
        .await
        .unwrap();

    assert!(matches!(
        repo.refresh(&ctx_mario, album.id).await,
        Err(DbError::Forbidden)
    ));
}

// The reverse direction of `list_assets` (used by the ALBUM section of the
// lightbox info panel): given an asset, which albums it already belongs to.

#[tokio::test]
async fn for_asset_lists_every_album_the_asset_is_a_member_of() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);

    let lib = seed_library(&test, admin, "/mnt/for-asset").await;
    let folder = seed_folder(&test, lib, "2024").await;
    let photo = index_photo(&test, folder, "photo.jpg").await;
    let other_photo = index_photo(&test, folder, "other.jpg").await;

    let repo = AlbumRepo::new(test.db());
    let album_a = repo
        .create(
            &ctx,
            NewAlbum {
                name: "Vacanze".into(),
                description: String::new(),
                rule: None,
            },
        )
        .await
        .unwrap();
    let album_b = repo
        .create(
            &ctx,
            NewAlbum {
                name: "Famiglia".into(),
                description: String::new(),
                rule: None,
            },
        )
        .await
        .unwrap();
    // A third album the photo is NOT part of: it must not show up.
    repo.create(
        &ctx,
        NewAlbum {
            name: "Altro".into(),
            description: String::new(),
            rule: None,
        },
    )
    .await
    .unwrap();

    repo.add_asset(&ctx, album_a.id, photo).await.unwrap();
    repo.add_asset(&ctx, album_b.id, photo).await.unwrap();
    repo.add_asset(&ctx, album_a.id, other_photo).await.unwrap();

    let albums = repo.for_asset(&ctx, photo).await.unwrap();
    let names: Vec<&str> = albums.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(
        names,
        vec!["Famiglia", "Vacanze"],
        "ordered by name, only the two the photo is a member of"
    );
}

#[tokio::test]
async fn for_asset_is_empty_when_the_asset_is_in_no_album() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);

    let lib = seed_library(&test, admin, "/mnt/for-asset-empty").await;
    let folder = seed_folder(&test, lib, "2024").await;
    let photo = index_photo(&test, folder, "solo.jpg").await;

    let albums = AlbumRepo::new(test.db())
        .for_asset(&ctx, photo)
        .await
        .unwrap();
    assert!(albums.is_empty());
}

#[tokio::test]
async fn for_asset_hides_albums_the_caller_cannot_see_but_still_lists_the_shared_one() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let mario = harness::seed_user(&test, admin, "mario-albums").await;
    let ctx_admin = AuthContext::user(admin, SystemRole::Admin);
    let ctx_mario = AuthContext::user(mario, SystemRole::User);

    let lib = seed_library(&test, admin, "/mnt/for-asset-shared").await;
    let folder = seed_folder(&test, lib, "2024").await;
    let photo = index_photo(&test, folder, "condivisa.jpg").await;

    let repo = AlbumRepo::new(test.db());
    let private = repo
        .create(
            &ctx_admin,
            NewAlbum {
                name: "Privato".into(),
                description: String::new(),
                rule: None,
            },
        )
        .await
        .unwrap();
    let shared = repo
        .create(
            &ctx_admin,
            NewAlbum {
                name: "Condiviso".into(),
                description: String::new(),
                rule: None,
            },
        )
        .await
        .unwrap();
    repo.add_asset(&ctx_admin, private.id, photo).await.unwrap();
    repo.add_asset(&ctx_admin, shared.id, photo).await.unwrap();
    grant_album(&test, admin, mario, shared.id, ObjectRole::Viewer).await;

    let albums = repo.for_asset(&ctx_mario, photo).await.unwrap();
    let names: Vec<&str> = albums.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(
        names,
        vec!["Condiviso"],
        "only the album shared with the caller, never someone else's private one"
    );
}

#[tokio::test]
async fn for_asset_is_forbidden_on_an_asset_the_caller_cannot_see_at_all() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let stranger = harness::seed_user(&test, admin, "estraneo-albums").await;
    let ctx_admin = AuthContext::user(admin, SystemRole::Admin);
    let ctx_stranger = AuthContext::user(stranger, SystemRole::User);

    let lib = seed_library(&test, admin, "/mnt/for-asset-forbidden").await;
    let folder = seed_folder(&test, lib, "2024").await;
    let photo = index_photo(&test, folder, "privata.jpg").await;

    // Asset visibility is checked **before** album visibility: even if
    // `stranger` had their own album to slot this id into (not the case
    // here, the album belongs to admin), membership in an album must
    // never reveal the existence of an asset that would otherwise be
    // invisible to the caller.
    let repo = AlbumRepo::new(test.db());
    let album = repo
        .create(
            &ctx_admin,
            NewAlbum {
                name: "Vacanze".into(),
                description: String::new(),
                rule: None,
            },
        )
        .await
        .unwrap();
    repo.add_asset(&ctx_admin, album.id, photo).await.unwrap();

    assert!(matches!(
        repo.for_asset(&ctx_stranger, photo).await,
        Err(DbError::Forbidden)
    ));
}
