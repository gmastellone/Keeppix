mod harness;

use chrono::{TimeZone, Utc};
use harness::TestDb;
use keeppix_db::{AssetRepo, DbError, FlagRepo, FolderRepo, LibraryRepo, UserRepo};
use keeppix_domain::{
    AssetFlags, AssetId, AssetKind, AssetName, AuthContext, LibraryId, NewAsset, NewLibrary,
    NewUser, Pick, Rating, SystemRole, UserId, Username, hash_password,
};

/// A second administrator. To prove that rating is per-user on a
/// **single** asset, both callers must be able to see the same library,
/// and only an admin can see libraries they don't own.
#[allow(clippy::expect_used, clippy::unwrap_used)]
async fn seed_second_admin(test: &TestDb, admin: UserId, username: &str) -> UserId {
    use keeppix_domain::Password;

    let password = Password::parse("correct horse battery staple").expect("valid password");
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    UserRepo::new(test.db())
        .create(
            &ctx,
            NewUser {
                username: Username::parse(username).expect("valid username"),
                email: None,
                display_name: username.to_owned(),
                password_hash: hash_password(&password).expect("hash").as_str().to_owned(),
                role: SystemRole::Admin,
            },
        )
        .await
        .expect("create admin")
        .id
}

#[allow(clippy::expect_used, clippy::unwrap_used)]
async fn seed_library(test: &TestDb, owner: UserId) -> LibraryId {
    LibraryRepo::new(test.db())
        .create(
            &AuthContext::user(owner, SystemRole::Admin),
            NewLibrary {
                name: "Foto".to_owned(),
                owner_id: owner,
                root_path: std::path::PathBuf::from("/mnt/foto"),
                exclude_patterns: vec![],
            },
        )
        .await
        .expect("library")
        .id
}

#[allow(clippy::expect_used, clippy::unwrap_used)]
fn discovered(folder: keeppix_domain::FolderId, filename: &str) -> NewAsset {
    NewAsset {
        folder_id: folder,
        filename: AssetName::parse(filename).expect("name"),
        size_bytes: 1000,
        mtime: Utc.with_ymd_and_hms(2024, 6, 1, 12, 0, 0).unwrap(),
        inode: Some(1),
        kind: AssetKind::Image,
    }
}

#[allow(clippy::expect_used, clippy::unwrap_used)]
async fn seed_asset(test: &TestDb, owner: UserId, filename: &str) -> AssetId {
    let library = seed_library(test, owner).await;
    let folder = FolderRepo::new(test.db())
        .ensure_path(library, &["2024"])
        .await
        .expect("folder");
    AssetRepo::new(test.db())
        .upsert_discovered(discovered(folder.id, filename))
        .await
        .expect("asset")
        .unwrap()
        .id
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn a_user_can_rate_and_read_their_own_flags() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let asset = seed_asset(&test, admin, "DSC_0001.ARW").await;
    let repo = FlagRepo::new(test.db());

    let flags = AssetFlags {
        rating: Some(Rating::parse(4).unwrap()),
        pick: Pick::Pick,
        color_label: Some("red".to_owned()),
        favorite: true,
    };
    repo.set(&ctx, asset, &flags).await.unwrap();

    let read = repo.get(&ctx, asset).await.unwrap();
    assert_eq!(read, flags);
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn get_returns_defaults_when_the_caller_never_voted() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let asset = seed_asset(&test, admin, "DSC_0002.ARW").await;

    let flags = FlagRepo::new(test.db()).get(&ctx, asset).await.unwrap();
    assert_eq!(flags, AssetFlags::default());
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn two_users_rating_the_same_asset_do_not_overwrite_each_other() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    // Both callers must be able to see the asset: the library belongs to
    // admin, so admin stays the sole "owner" — having the second caller vote
    // via a shared-visibility grant is out of scope here, so both votes come
    // from admins on the same asset, purely to isolate the "per user" rule.
    let luca = seed_second_admin(&test, admin, "luca").await;
    let asset = seed_asset(&test, admin, "DSC_0003.ARW").await;
    let repo = FlagRepo::new(test.db());

    let admin_ctx = AuthContext::user(admin, SystemRole::Admin);
    let luca_ctx = AuthContext::user(luca, SystemRole::Admin);

    repo.set(
        &admin_ctx,
        asset,
        &AssetFlags {
            rating: Some(Rating::parse(5).unwrap()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    repo.set(
        &luca_ctx,
        asset,
        &AssetFlags {
            rating: Some(Rating::parse(2).unwrap()),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    assert_eq!(
        repo.get(&admin_ctx, asset).await.unwrap().rating,
        Some(Rating::parse(5).unwrap()),
        "the admin's rating must not be overwritten by luca's"
    );
    assert_eq!(
        repo.get(&luca_ctx, asset).await.unwrap().rating,
        Some(Rating::parse(2).unwrap())
    );

    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM asset_flags WHERE asset_id = $1")
        .bind(asset.as_uuid())
        .fetch_one(test.db().pool())
        .await
        .unwrap();
    assert_eq!(count, 2, "una riga per utente, non una condivisa");
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn batch_set_applies_the_same_flags_to_many_assets_at_once() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let library = seed_library(&test, admin).await;
    let folder = FolderRepo::new(test.db())
        .ensure_path(library, &["2024"])
        .await
        .unwrap();
    let assets_repo = AssetRepo::new(test.db());

    let mut ids = Vec::new();
    for i in 0..50 {
        let asset = assets_repo
            .upsert_discovered(discovered(folder.id, &format!("DSC_{i:04}.ARW")))
            .await
            .unwrap()
            .unwrap();
        ids.push(asset.id);
    }

    let repo = FlagRepo::new(test.db());
    let flags = AssetFlags {
        rating: None,
        pick: Pick::Reject,
        color_label: None,
        favorite: false,
    };
    repo.batch_set(&ctx, &ids, &flags).await.unwrap();

    for id in ids {
        assert_eq!(repo.get(&ctx, id).await.unwrap().pick, Pick::Reject);
    }
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn a_plain_user_cannot_set_flags_on_someone_elses_asset() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let mario = harness::seed_user(&test, admin, "mario").await;
    let asset = seed_asset(&test, admin, "DSC_0004.ARW").await;
    let repo = FlagRepo::new(test.db());
    let mario_ctx = AuthContext::user(mario, SystemRole::User);

    assert!(matches!(
        repo.set(&mario_ctx, asset, &AssetFlags::default()).await,
        Err(DbError::Forbidden)
    ));
    assert!(matches!(
        repo.get(&mario_ctx, asset).await,
        Err(DbError::Forbidden)
    ));
}

/// `favorite` is not a reuse of `Pick`: they are separate columns. Skipping
/// a shot in culling (`pick = Reject`) must not clear `favorite`, and
/// conversely setting `favorite` must not touch `pick`.
#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn favorite_and_pick_are_independent_axes() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let asset = seed_asset(&test, admin, "DSC_0005.ARW").await;
    let repo = FlagRepo::new(test.db());

    repo.set(
        &ctx,
        asset,
        &AssetFlags {
            favorite: true,
            pick: Pick::None,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert!(repo.get(&ctx, asset).await.unwrap().favorite);

    // "Skip in culling": pick moves to Reject, favorite stays true.
    repo.set(
        &ctx,
        asset,
        &AssetFlags {
            favorite: true,
            pick: Pick::Reject,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let after_discard = repo.get(&ctx, asset).await.unwrap();
    assert_eq!(after_discard.pick, Pick::Reject);
    assert!(
        after_discard.favorite,
        "skipping in culling must not clear the favorite flag"
    );

    // And conversely: unfavoriting must not touch pick.
    repo.set(
        &ctx,
        asset,
        &AssetFlags {
            favorite: false,
            pick: Pick::Reject,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let after_unfavorite = repo.get(&ctx, asset).await.unwrap();
    assert!(!after_unfavorite.favorite);
    assert_eq!(
        after_unfavorite.pick,
        Pick::Reject,
        "togliere il preferito non deve toccare pick"
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn favorites_among_returns_only_the_callers_own_favorites_in_the_given_set() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let luca = seed_second_admin(&test, admin, "luca").await;
    let admin_ctx = AuthContext::user(admin, SystemRole::Admin);
    let luca_ctx = AuthContext::user(luca, SystemRole::Admin);
    let repo = FlagRepo::new(test.db());

    let library = seed_library(&test, admin).await;
    let folder = FolderRepo::new(test.db())
        .ensure_path(library, &["2024"])
        .await
        .unwrap();
    let assets_repo = AssetRepo::new(test.db());
    let loved = assets_repo
        .upsert_discovered(discovered(folder.id, "DSC_0006.ARW"))
        .await
        .unwrap()
        .unwrap()
        .id;
    let plain = assets_repo
        .upsert_discovered(discovered(folder.id, "DSC_0007.ARW"))
        .await
        .unwrap()
        .unwrap()
        .id;

    repo.set(
        &admin_ctx,
        loved,
        &AssetFlags {
            favorite: true,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    // luca favorites `plain`, but it must never show up in admin's set.
    repo.set(
        &luca_ctx,
        plain,
        &AssetFlags {
            favorite: true,
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let favorites = repo
        .favorites_among(&admin_ctx, &[loved, plain])
        .await
        .unwrap();
    assert_eq!(favorites, std::collections::HashSet::from([loved]));
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn probing_a_nonexistent_asset_id_is_forbidden_not_not_found() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let mario = harness::seed_user(&test, admin, "mario").await;
    let mario_ctx = AuthContext::user(mario, SystemRole::User);
    let repo = FlagRepo::new(test.db());
    let ghost = AssetId::new();

    assert!(matches!(
        repo.get(&mario_ctx, ghost).await,
        Err(DbError::Forbidden)
    ));
    assert!(matches!(
        repo.set(&mario_ctx, ghost, &AssetFlags::default()).await,
        Err(DbError::Forbidden)
    ));
}
