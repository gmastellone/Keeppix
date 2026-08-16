mod harness;

use chrono::{TimeZone, Utc};
use harness::TestDb;
use keeppix_db::{AssetRepo, DbError, FlagRepo, FolderRepo, LibraryRepo, UserRepo};
use keeppix_domain::{
    AssetFlags, AssetId, AssetKind, AssetName, AuthContext, LibraryId, NewAsset, NewLibrary,
    NewUser, Pick, Rating, SystemRole, UserId, Username, hash_password,
};

/// Un secondo amministratore. Nella Fase 2 non esiste ancora la
/// condivisione (Fase 3): per provare che il rating è per utente su un
/// **singolo** asset, i due chiamanti devono vedere la stessa libreria, e
/// nella Fase 2 solo un admin vede librerie che non possiede.
#[allow(clippy::expect_used)]
async fn seed_second_admin(test: &TestDb, admin: UserId, username: &str) -> UserId {
    use keeppix_domain::Password;

    let password = Password::parse("correct horse battery staple").expect("password valida");
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    UserRepo::new(test.db())
        .create(
            &ctx,
            NewUser {
                username: Username::parse(username).expect("username valido"),
                email: None,
                display_name: username.to_owned(),
                password_hash: hash_password(&password).expect("hash").as_str().to_owned(),
                role: SystemRole::Admin,
            },
        )
        .await
        .expect("creazione admin")
        .id
}

#[allow(clippy::expect_used)]
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
        .expect("libreria")
        .id
}

#[allow(clippy::expect_used)]
fn discovered(folder: keeppix_domain::FolderId, filename: &str) -> NewAsset {
    NewAsset {
        folder_id: folder,
        filename: AssetName::parse(filename).expect("nome"),
        size_bytes: 1000,
        mtime: Utc.with_ymd_and_hms(2024, 6, 1, 12, 0, 0).unwrap(),
        inode: Some(1),
        kind: AssetKind::Image,
    }
}

#[allow(clippy::expect_used)]
async fn seed_asset(test: &TestDb, owner: UserId, filename: &str) -> AssetId {
    let library = seed_library(test, owner).await;
    let folder = FolderRepo::new(test.db())
        .ensure_path(library, &["2024"])
        .await
        .expect("cartella");
    AssetRepo::new(test.db())
        .upsert_discovered(discovered(folder.id, filename))
        .await
        .expect("asset")
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
    // Sia admin che mario devono vedere l'asset: la libreria è dell'admin,
    // che quindi resta l'unico "proprietario" — mario vota tramite un
    // amministratore che promuove la visibilità non è nello scope di questa
    // fase, quindi qui i due voti arrivano entrambi da amministratori sullo
    // stesso asset per isolare la sola regola "per utente".
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
        "il voto dell'admin non deve essere sovrascritto da quello di luca"
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
            .unwrap();
        ids.push(asset.id);
    }

    let repo = FlagRepo::new(test.db());
    let flags = AssetFlags {
        rating: None,
        pick: Pick::Reject,
        color_label: None,
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
