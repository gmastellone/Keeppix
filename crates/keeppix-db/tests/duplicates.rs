#![allow(clippy::unwrap_used)]

mod harness;

use chrono::{TimeZone, Utc};
use harness::TestDb;
use keeppix_db::{AssetRepo, DbError, DuplicateRepo, FolderRepo, LibraryRepo};
use keeppix_domain::{
    AssetKind, AssetName, AuthContext, DiskAction, LibraryId, NewAsset, NewLibrary, SystemRole,
    UserId,
};

async fn seed(test: &TestDb) -> (UserId, LibraryId, keeppix_domain::FolderId) {
    let admin = harness::seed_admin(test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let library = LibraryRepo::new(test.db())
        .create(
            &ctx,
            NewLibrary {
                name: "Foto".to_owned(),
                owner_id: admin,
                root_path: std::path::PathBuf::from("/mnt/foto"),
                exclude_patterns: vec![],
            },
        )
        .await
        .unwrap()
        .id;
    let folder = FolderRepo::new(test.db())
        .ensure_path(library, &[])
        .await
        .unwrap()
        .id;
    (admin, library, folder)
}

fn photo(folder: keeppix_domain::FolderId, name: &str, size: i64) -> NewAsset {
    NewAsset {
        folder_id: folder,
        filename: AssetName::parse(name).unwrap(),
        size_bytes: size,
        mtime: Utc.with_ymd_and_hms(2024, 7, 1, 0, 0, 0).unwrap(),
        inode: Some(1),
        kind: AssetKind::Image,
    }
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn duplicates_report_reclaimable_space_not_the_total() {
    let test = TestDb::start().await;
    let (admin, _library, folder) = seed(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let assets = AssetRepo::new(test.db());
    let a = assets
        .upsert_discovered(photo(folder, "a.jpg", 1000))
        .await
        .unwrap();
    let b = assets
        .upsert_discovered(photo(folder, "b.jpg", 1000))
        .await
        .unwrap();
    let c = assets
        .upsert_discovered(photo(folder, "c.jpg", 1000))
        .await
        .unwrap();
    let hash = [7u8; 32];
    for asset in [&a, &b, &c] {
        assets.set_hash(asset.id, hash).await.unwrap();
        assets
            .set_indexed(
                asset.id,
                Utc.with_ymd_and_hms(2024, 7, 1, 12, 0, 0).unwrap(),
                1,
                1,
            )
            .await
            .unwrap();
    }

    let groups = DuplicateRepo::new(test.db()).groups(&ctx).await.unwrap();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].count, 3);
    // Tre copie da 1000 byte: recuperabile è 2000 (due copie in eccesso),
    // non 3000 (la somma di tutte e tre) — la prima copia è la foto, non
    // spazio da liberare.
    assert_eq!(
        groups[0].reclaimable_bytes(),
        2000,
        "size * (copie - 1), non la somma totale delle copie"
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn a_trashed_copy_does_not_count_as_a_duplicate() {
    let test = TestDb::start().await;
    let (admin, _library, folder) = seed(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let assets = AssetRepo::new(test.db());
    let a = assets
        .upsert_discovered(photo(folder, "a.jpg", 1000))
        .await
        .unwrap();
    let b = assets
        .upsert_discovered(photo(folder, "b.jpg", 1000))
        .await
        .unwrap();
    let hash = [9u8; 32];
    assets.set_hash(a.id, hash).await.unwrap();
    assets.set_hash(b.id, hash).await.unwrap();
    assets
        .set_indexed(
            a.id,
            Utc.with_ymd_and_hms(2024, 7, 1, 12, 0, 0).unwrap(),
            1,
            1,
        )
        .await
        .unwrap();
    assets
        .set_indexed(
            b.id,
            Utc.with_ymd_and_hms(2024, 7, 2, 12, 0, 0).unwrap(),
            1,
            1,
        )
        .await
        .unwrap();

    // Ancora due copie visibili: il gruppo esiste.
    let groups = DuplicateRepo::new(test.db()).groups(&ctx).await.unwrap();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].count, 2);

    // `b` finisce nel cestino: da questo momento in poi non è più una
    // "copia recuperabile" — è già in coda per sparire da sola.
    sqlx::query("UPDATE assets SET status = 'trashed' WHERE id = $1")
        .bind(b.id.as_uuid())
        .execute(test.db().pool())
        .await
        .unwrap();

    let groups = DuplicateRepo::new(test.db()).groups(&ctx).await.unwrap();
    assert!(
        groups.is_empty(),
        "un solo asset non cestinato non è più un duplicato: {groups:?}"
    );

    let members = DuplicateRepo::new(test.db())
        .members(&ctx, &hash)
        .await
        .unwrap();
    assert_eq!(
        members.len(),
        1,
        "members() esclude l'asset cestinato quanto groups()"
    );
    assert_eq!(members[0].id, a.id);
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn resolve_keeps_the_chosen_asset_and_removes_the_rest_from_the_index() {
    let test = TestDb::start().await;
    let (admin, _library, folder) = seed(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let assets = AssetRepo::new(test.db());
    let a = assets
        .upsert_discovered(photo(folder, "a.jpg", 1000))
        .await
        .unwrap();
    let b = assets
        .upsert_discovered(photo(folder, "b.jpg", 1000))
        .await
        .unwrap();
    let c = assets
        .upsert_discovered(photo(folder, "c.jpg", 1000))
        .await
        .unwrap();
    let hash = [3u8; 32];
    for asset in [&a, &b, &c] {
        assets.set_hash(asset.id, hash).await.unwrap();
        assets
            .set_indexed(
                asset.id,
                Utc.with_ymd_and_hms(2024, 7, 1, 12, 0, 0).unwrap(),
                1,
                1,
            )
            .await
            .unwrap();
    }

    let resolved = DuplicateRepo::new(test.db())
        .resolve(&ctx, &hash, a.id, DiskAction::Kept)
        .await
        .unwrap();
    assert_eq!(resolved, 2, "b e c rimossi, a tenuto");

    // `Kept` rimuove solo dall'indice: le righe di b e c sono sparite.
    let remaining: i64 = sqlx::query_scalar("SELECT count(*) FROM assets WHERE id = ANY($1)")
        .bind([b.id.as_uuid(), c.id.as_uuid()].as_slice())
        .fetch_one(test.db().pool())
        .await
        .unwrap();
    assert_eq!(remaining, 0);
    let kept_still_exists: bool =
        sqlx::query_scalar("SELECT exists(SELECT 1 FROM assets WHERE id = $1)")
            .bind(a.id.as_uuid())
            .fetch_one(test.db().pool())
            .await
            .unwrap();
    assert!(kept_still_exists, "l'asset scelto non viene toccato");
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn resolving_with_a_keep_id_outside_the_group_is_forbidden_not_not_found() {
    let test = TestDb::start().await;
    let (admin, _library, folder) = seed(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let assets = AssetRepo::new(test.db());
    let a = assets
        .upsert_discovered(photo(folder, "a.jpg", 1000))
        .await
        .unwrap();
    let b = assets
        .upsert_discovered(photo(folder, "b.jpg", 1000))
        .await
        .unwrap();
    let outsider = assets
        .upsert_discovered(photo(folder, "outsider.jpg", 1000))
        .await
        .unwrap();
    let hash = [5u8; 32];
    assets.set_hash(a.id, hash).await.unwrap();
    assets.set_hash(b.id, hash).await.unwrap();
    for asset in [&a, &b] {
        assets
            .set_indexed(
                asset.id,
                Utc.with_ymd_and_hms(2024, 7, 1, 12, 0, 0).unwrap(),
                1,
                1,
            )
            .await
            .unwrap();
    }

    assert!(matches!(
        DuplicateRepo::new(test.db())
            .resolve(&ctx, &hash, outsider.id, DiskAction::Kept)
            .await,
        Err(DbError::Forbidden)
    ));
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn a_plain_user_does_not_see_another_owners_duplicates() {
    let test = TestDb::start().await;
    let (admin, _library, folder) = seed(&test).await;
    let mario = harness::seed_user(&test, admin, "mario").await;
    let mario_ctx = AuthContext::user(mario, SystemRole::User);
    let assets = AssetRepo::new(test.db());
    let a = assets
        .upsert_discovered(photo(folder, "a.jpg", 1000))
        .await
        .unwrap();
    let b = assets
        .upsert_discovered(photo(folder, "b.jpg", 1000))
        .await
        .unwrap();
    let hash = [11u8; 32];
    assets.set_hash(a.id, hash).await.unwrap();
    assets.set_hash(b.id, hash).await.unwrap();
    for asset in [&a, &b] {
        assets
            .set_indexed(
                asset.id,
                Utc.with_ymd_and_hms(2024, 7, 1, 12, 0, 0).unwrap(),
                1,
                1,
            )
            .await
            .unwrap();
    }

    let groups = DuplicateRepo::new(test.db())
        .groups(&mario_ctx)
        .await
        .unwrap();
    assert!(groups.is_empty());
    let members = DuplicateRepo::new(test.db())
        .members(&mario_ctx, &hash)
        .await
        .unwrap();
    assert!(members.is_empty());
}
