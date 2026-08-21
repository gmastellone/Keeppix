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

/// Una foto può stare in molti album senza essere duplicata su disco.
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

    // L'asset esiste una sola volta nella tabella assets.
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM assets WHERE id = $1")
        .bind(photo.as_uuid())
        .fetch_one(test.db().pool())
        .await
        .unwrap();
    assert_eq!(count, 1, "l'asset deve esistere una sola volta");
}

/// Rimuovere una foto da un album non cancella la foto.
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

    // L'album è vuoto.
    let in_album = repo.list_assets(&ctx, album.id).await.unwrap();
    assert!(
        in_album.is_empty(),
        "l'album deve essere vuoto dopo la rimozione"
    );

    // L'asset ancora esiste.
    let asset = AssetRepo::new(test.db()).find_by_id(&ctx, photo).await;
    assert!(
        asset.is_ok(),
        "l'asset deve esistere ancora dopo la rimozione dall'album"
    );
    assert_eq!(
        AssetRepo::new(test.db())
            .count_by_status(&ctx, AssetStatus::Indexed)
            .await
            .unwrap(),
        1,
        "l'asset deve essere ancora indexed"
    );
}

/// Condividere un album con un utente gli concede la visibilità degli asset
/// in quell'album, ma non espone la cartella in cui quei file risiedono.
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

    // Prima di condividere: mario non vede nulla.
    assert_eq!(
        AssetRepo::new(test.db())
            .count_by_status(&mario_ctx, AssetStatus::Indexed)
            .await
            .unwrap(),
        0,
        "prima della condivisione mario non deve vedere asset"
    );
    assert!(
        AssetRepo::new(test.db())
            .find_by_id(&mario_ctx, photo)
            .await
            .is_err(),
        "find_by_id deve rispondere Forbidden prima della condivisione"
    );

    // Crea l'album e ci aggiunge la foto.
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

    // Condivide l'album con mario.
    grant_album(&test, admin, mario, album.id, ObjectRole::Viewer).await;

    // Ora mario vede la foto tramite il grant sull'album.
    assert_eq!(
        AssetRepo::new(test.db())
            .count_by_status(&mario_ctx, AssetStatus::Indexed)
            .await
            .unwrap(),
        1,
        "dopo la condivisione mario deve vedere l'asset"
    );
    assert!(
        AssetRepo::new(test.db())
            .find_by_id(&mario_ctx, photo)
            .await
            .is_ok(),
        "find_by_id deve riuscire dopo la condivisione"
    );

    // Ma mario non vede la cartella "Privato".
    let tree = FolderRepo::new(test.db()).tree(&mario_ctx).await.unwrap();
    assert!(
        tree.is_empty(),
        "la cartella non deve essere visibile: mario ha accesso via album, non via cartella"
    );
}

/// Eliminare un album non elimina le foto in esso contenute.
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

    // Le foto sono ancora lì.
    assert!(AssetRepo::new(test.db()).find_by_id(&ctx, p1).await.is_ok());
    assert!(AssetRepo::new(test.db()).find_by_id(&ctx, p2).await.is_ok());
    assert_eq!(
        AssetRepo::new(test.db())
            .count_by_status(&ctx, AssetStatus::Indexed)
            .await
            .unwrap(),
        2,
        "dopo l'eliminazione dell'album devono restare 2 asset indexed"
    );

    // L'album è sparito.
    let albums = repo.list(&ctx).await.unwrap();
    assert!(albums.is_empty(), "l'album deve essere eliminato");
}

/// Un utente senza permesso sull'album riceve Forbidden — non NotFound.
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

    // Mario non ha nessun permesso: deve ricevere Forbidden.
    assert!(matches!(
        repo.get(&ctx_mario, album.id).await,
        Err(DbError::Forbidden)
    ));
    assert!(matches!(
        repo.list_assets(&ctx_mario, album.id).await,
        Err(DbError::Forbidden)
    ));

    // L'album non deve comparire nella lista di mario.
    let list = repo.list(&ctx_mario).await.unwrap();
    assert!(list.is_empty());
}

/// Un album senza `rule` non può essere aggiornato: non è un difetto di
/// autorizzazione, è che non c'è nulla da rilanciare.
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
        "un album senza rule non deve produrre un esito di refresh"
    );
}

/// Il refresh riapplica la `rule` con cui l'album è stato creato: le foto che
/// combaciano entrano, quelle che non combaciano più (o che erano state
/// aggiunte a mano fuori dal filtro) escono. Un secondo refresh senza
/// modifiche al catalogo non deve produrre duplicati né movimenti.
#[tokio::test]
async fn refresh_adds_matches_and_removes_non_matches_and_is_idempotent() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);

    let lib = seed_library(&test, admin, "/mnt/nas").await;
    let folder = seed_folder(&test, lib, "2024").await;
    let jpeg = index_photo(&test, folder, "a.jpg").await;
    let another_jpeg = index_photo(&test, folder, "b.jpg").await;

    // Un video, fuori dal filtro type:image.
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

    // Aggiunto a mano, fuori dal filtro: il refresh deve rimuoverlo.
    repo.add_asset(&ctx, album.id, video.id).await.unwrap();

    let refresh = repo
        .refresh(&ctx, album.id)
        .await
        .unwrap()
        .expect("l'album ha una rule");
    let mut added: Vec<AssetId> = refresh.added.clone();
    added.sort_by_key(AssetId::as_uuid);
    let mut expected_added = vec![jpeg, another_jpeg];
    expected_added.sort_by_key(AssetId::as_uuid);
    assert_eq!(added, expected_added, "le due foto devono essere aggiunte");
    assert_eq!(
        refresh.removed,
        vec![video.id],
        "il video fuori dal filtro deve essere rimosso"
    );

    let members = repo.list_assets(&ctx, album.id).await.unwrap();
    let mut member_ids: Vec<AssetId> = members.iter().map(|m| m.asset.id).collect();
    member_ids.sort_by_key(AssetId::as_uuid);
    assert_eq!(member_ids, expected_added);

    // rule_run_at è stato scritto.
    let rule_run_at: Option<chrono::DateTime<Utc>> =
        sqlx::query_scalar("SELECT rule_run_at FROM albums WHERE id = $1")
            .bind(album.id.as_uuid())
            .fetch_one(test.db().pool())
            .await
            .unwrap();
    assert!(rule_run_at.is_some(), "rule_run_at deve essere impostato");

    // Un secondo refresh, senza cambi nel catalogo, non deve aggiungere né
    // rimuovere nulla: idempotenza.
    let second = repo.refresh(&ctx, album.id).await.unwrap().unwrap();
    assert!(
        second.added.is_empty(),
        "il secondo refresh non deve aggiungere duplicati"
    );
    assert!(
        second.removed.is_empty(),
        "il secondo refresh non deve rimuovere nulla di già coerente"
    );
}

/// Un utente senza permesso sull'album riceve `Forbidden` — mai `NotFound` —
/// anche per il refresh, stesso invariante degli altri endpoint dell'album.
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
