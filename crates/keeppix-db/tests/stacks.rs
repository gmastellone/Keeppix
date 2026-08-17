mod harness;

use chrono::{TimeZone, Utc};
use harness::TestDb;
use keeppix_db::{AssetRepo, FolderRepo, LibraryRepo, StackRepo};
use keeppix_domain::{
    AssetId, AssetKind, AssetName, AuthContext, FolderId, LibraryId, NewAsset, NewLibrary,
    SystemRole, UserId,
};
use uuid::Uuid;

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
        .expect("libreria")
        .id
}

#[allow(clippy::expect_used, clippy::unwrap_used)]
async fn seed_folder(test: &TestDb, owner: UserId) -> FolderId {
    let library = seed_library(test, owner).await;
    FolderRepo::new(test.db())
        .ensure_path(library, &["2024"])
        .await
        .expect("cartella")
        .id
}

#[allow(clippy::expect_used, clippy::unwrap_used)]
fn discovered(folder: FolderId, filename: &str, kind: AssetKind) -> NewAsset {
    NewAsset {
        folder_id: folder,
        filename: AssetName::parse(filename).expect("nome"),
        size_bytes: 1000,
        mtime: Utc.with_ymd_and_hms(2024, 6, 1, 12, 0, 0).unwrap(),
        inode: Some(1),
        kind,
    }
}

#[allow(clippy::expect_used, clippy::unwrap_used)]
async fn seed_asset(test: &TestDb, folder: FolderId, filename: &str, kind: AssetKind) -> AssetId {
    AssetRepo::new(test.db())
        .upsert_discovered(discovered(folder, filename, kind))
        .await
        .expect("asset")
        .unwrap()
        .id
}

#[allow(clippy::expect_used, clippy::unwrap_used)]
async fn stack_id_of(test: &TestDb, asset: AssetId) -> Option<Uuid> {
    sqlx::query_scalar("SELECT stack_id FROM assets WHERE id = $1")
        .bind(asset.as_uuid())
        .fetch_one(test.db().pool())
        .await
        .expect("lettura stack_id")
}

#[allow(clippy::expect_used, clippy::unwrap_used)]
async fn primary_of(test: &TestDb, stack: Uuid) -> Option<Uuid> {
    sqlx::query_scalar("SELECT primary_asset_id FROM stacks WHERE id = $1")
        .bind(stack)
        .fetch_optional(test.db().pool())
        .await
        .expect("lettura primary_asset_id")
}

#[allow(clippy::expect_used, clippy::unwrap_used)]
async fn stack_count(test: &TestDb) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM stacks")
        .fetch_one(test.db().pool())
        .await
        .expect("conteggio stack")
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn same_basename_in_the_same_folder_stacks_raw_and_jpeg_together() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let folder = seed_folder(&test, admin).await;

    let raw = seed_asset(&test, folder, "DSC_0042.ARW", AssetKind::RawImage).await;
    let jpeg = seed_asset(&test, folder, "DSC_0042.JPG", AssetKind::Image).await;

    StackRepo::new(test.db())
        .regroup_folder(folder)
        .await
        .unwrap();

    let raw_stack = stack_id_of(&test, raw).await;
    let jpeg_stack = stack_id_of(&test, jpeg).await;
    assert!(raw_stack.is_some(), "il RAW deve finire in uno stack");
    assert_eq!(
        raw_stack, jpeg_stack,
        "stesso nome base nella stessa cartella: devono condividere lo stack"
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn the_raw_is_the_primary_asset_when_present() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let folder = seed_folder(&test, admin).await;

    // Il JPEG è scritto per primo e il suo nome ordina prima del RAW in
    // ordine alfabetico ("JPG" < "NEF"), apposta: né l'ordine di
    // scrittura né l'ordinamento per nome devono decidere il primario,
    // solo il tipo deve farlo. Con ".ARW" (che ordina prima di ".JPG")
    // questo test passerebbe anche con un fallback "primo per nome",
    // senza provare davvero la preferenza per il RAW.
    let jpeg = seed_asset(&test, folder, "DSC_0043.JPG", AssetKind::Image).await;
    let raw = seed_asset(&test, folder, "DSC_0043.NEF", AssetKind::RawImage).await;

    StackRepo::new(test.db())
        .regroup_folder(folder)
        .await
        .unwrap();

    let stack = stack_id_of(&test, jpeg).await.expect("stack");
    let primary = primary_of(&test, stack).await;
    assert_eq!(
        primary,
        Some(raw.as_uuid()),
        "il RAW ha più informazione: deve essere il primario, non il JPEG"
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn a_lone_jpeg_does_not_form_a_stack() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let folder = seed_folder(&test, admin).await;

    let jpeg = seed_asset(&test, folder, "DSC_0099.JPG", AssetKind::Image).await;

    StackRepo::new(test.db())
        .regroup_folder(folder)
        .await
        .unwrap();

    assert_eq!(
        stack_id_of(&test, jpeg).await,
        None,
        "un solo file con quel nome base non deve mai formare uno stack"
    );
    assert_eq!(stack_count(&test).await, 0);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn three_files_with_the_same_basename_but_different_extensions_stack_together() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let folder = seed_folder(&test, admin).await;

    // Estensione del RAW scelta apposta perché ordina *dopo* le altre due
    // alfabeticamente ("HEIC" < "JPG" < "NEF"): la scelta del primario deve
    // venire dal tipo, non da un fallback "primo per nome" che qui
    // sceglierebbe l'HEIC.
    let heic = seed_asset(&test, folder, "DSC_0100.HEIC", AssetKind::Image).await;
    let jpeg = seed_asset(&test, folder, "DSC_0100.JPG", AssetKind::Image).await;
    let raw = seed_asset(&test, folder, "DSC_0100.NEF", AssetKind::RawImage).await;

    StackRepo::new(test.db())
        .regroup_folder(folder)
        .await
        .unwrap();

    let stack = stack_id_of(&test, raw).await.expect("stack");
    assert_eq!(stack_id_of(&test, jpeg).await, Some(stack));
    assert_eq!(stack_id_of(&test, heic).await, Some(stack));
    assert_eq!(primary_of(&test, stack).await, Some(raw.as_uuid()));
    assert_eq!(stack_count(&test).await, 1, "un solo stack per i tre file");
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn deleting_the_primary_promotes_another_member_instead_of_orphaning_the_stack() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let folder = seed_folder(&test, admin).await;

    let raw = seed_asset(&test, folder, "DSC_0200.ARW", AssetKind::RawImage).await;
    let jpeg = seed_asset(&test, folder, "DSC_0200.JPG", AssetKind::Image).await;
    let heic = seed_asset(&test, folder, "DSC_0200.HEIC", AssetKind::Image).await;

    let repo = StackRepo::new(test.db());
    repo.regroup_folder(folder).await.unwrap();
    let stack = stack_id_of(&test, jpeg).await.expect("stack");
    assert_eq!(primary_of(&test, stack).await, Some(raw.as_uuid()));

    // Cancellazione diretta: deve reggere anche senza passare da StackRepo,
    // esattamente come farà il cestino di Task 7.
    sqlx::query("DELETE FROM assets WHERE id = $1")
        .bind(raw.as_uuid())
        .execute(test.db().pool())
        .await
        .unwrap();

    let promoted = primary_of(&test, stack).await;
    assert!(
        promoted == Some(jpeg.as_uuid()) || promoted == Some(heic.as_uuid()),
        "il primario deve essere promosso a un membro sopravvissuto, non {promoted:?}"
    );
    assert_ne!(
        promoted,
        Some(raw.as_uuid()),
        "il RAW appena cancellato non può restare primario"
    );

    // Lo stack non è orfano: i due membri superstiti restano collegati.
    assert_eq!(stack_id_of(&test, jpeg).await, Some(stack));
    assert_eq!(stack_id_of(&test, heic).await, Some(stack));
    assert_eq!(stack_count(&test).await, 1, "lo stack non deve sparire");
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn regrouping_the_same_folder_twice_is_idempotent() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let folder = seed_folder(&test, admin).await;

    let raw = seed_asset(&test, folder, "DSC_0300.ARW", AssetKind::RawImage).await;
    let jpeg = seed_asset(&test, folder, "DSC_0300.JPG", AssetKind::Image).await;

    let repo = StackRepo::new(test.db());
    repo.regroup_folder(folder).await.unwrap();
    let stack_first = stack_id_of(&test, jpeg)
        .await
        .expect("stack dopo il primo giro");
    let primary_first = primary_of(&test, stack_first).await;
    assert_eq!(primary_first, Some(raw.as_uuid()));

    // Una seconda scansione della stessa cartella, immutata: non deve
    // creare un nuovo stack né spostare il primario. Senza questa
    // proprietà ogni riscansione produce uno stack nuovo (piano Fase 2,
    // Task 6).
    repo.regroup_folder(folder).await.unwrap();
    let stack_second = stack_id_of(&test, jpeg)
        .await
        .expect("stack dopo il secondo giro");
    assert_eq!(
        stack_first, stack_second,
        "riscansionare non deve creare un nuovo stack per lo stesso gruppo"
    );
    assert_eq!(primary_of(&test, stack_second).await, primary_first);
    assert_eq!(
        stack_count(&test).await,
        1,
        "una sola riga di stack, non una per ogni scansione"
    );
}
