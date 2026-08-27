//! Fase 9 Task 8: `RenameRepo::preview`/`apply` — i tre ambiti, la
//! co-rinomina delle pile, e la collisione verificata anche fuori dal
//! gruppo (difetto 1 del Task 7, chiuso qui).

mod harness;

use std::fs;
use std::path::PathBuf;

use chrono::{TimeZone, Utc};
use harness::TestDb;
use keeppix_db::{
    AssetRepo, DbError, FolderRepo, LibraryRepo, NewGrant, ObjectType, OperationsRepo,
    PermissionRepo, RenameRepo, StackRepo, SubjectType,
};
use keeppix_domain::{
    AssetId, AssetKind, AssetName, AuthContext, LibraryId, NewAsset, NewLibrary, ObjectRole,
    OperationKind, OperationStatus, SystemRole, UserId,
};

#[allow(clippy::expect_used, clippy::unwrap_used)]
fn temp_library_root(label: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "keeppix-rename-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("orologio di sistema")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("creazione della radice di test");
    root
}

#[allow(clippy::expect_used)]
async fn seed_library_at(test: &TestDb, owner: UserId, root: &std::path::Path) -> LibraryId {
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

#[allow(clippy::expect_used)]
fn discovered(folder: keeppix_domain::FolderId, filename: &str) -> NewAsset {
    NewAsset {
        folder_id: folder,
        filename: AssetName::parse(filename).expect("nome"),
        size_bytes: 9,
        mtime: Utc.with_ymd_and_hms(2024, 6, 1, 12, 0, 0).unwrap(),
        inode: None,
        kind: if filename.to_lowercase().ends_with(".arw") {
            AssetKind::RawImage
        } else {
            AssetKind::Image
        },
    }
}

#[allow(clippy::expect_used, clippy::unwrap_used)]
async fn indexed_asset(
    assets: &AssetRepo<'_>,
    folder: keeppix_domain::FolderId,
    filename: &str,
    taken_at: chrono::DateTime<Utc>,
) -> AssetId {
    let asset = assets
        .upsert_discovered(discovered(folder, filename))
        .await
        .unwrap()
        .unwrap();
    assets
        .set_indexed(asset.id, taken_at, 100, 100)
        .await
        .unwrap();
    asset.id
}

#[allow(clippy::unwrap_used)]
async fn set_exif(test: &TestDb, asset_id: AssetId, camera: &str, lens: &str) {
    sqlx::query(
        "INSERT INTO asset_exif (asset_id, raw, camera_model, lens) VALUES ($1, '{}'::jsonb, $2, $3)",
    )
    .bind(asset_id.as_uuid())
    .bind(camera)
    .bind(lens)
    .execute(test.db().pool())
    .await
    .unwrap();
}

#[allow(clippy::unwrap_used)]
async fn set_title(test: &TestDb, asset_id: AssetId, title: &str) {
    sqlx::query(
        "INSERT INTO asset_overrides (asset_id, title, updated_at) VALUES ($1, $2, now()) \
         ON CONFLICT (asset_id) DO UPDATE SET title = EXCLUDED.title",
    )
    .bind(asset_id.as_uuid())
    .bind(title)
    .execute(test.db().pool())
    .await
    .unwrap();
}

/// Un luogo del catalogo, con un punto qualunque (non è la geometria che
/// conta per questi test, solo il nome).
#[allow(clippy::unwrap_used)]
async fn seed_place(test: &TestDb, id: i64, name: &str) {
    sqlx::query(
        "INSERT INTO places (id, name, ascii_name, location) \
         VALUES ($1, $2, $2, ST_SetSRID(ST_MakePoint(11.0, 43.0), 4326)::geography)",
    )
    .bind(id)
    .bind(name)
    .execute(test.db().pool())
    .await
    .unwrap();
}

#[allow(clippy::unwrap_used)]
async fn set_place(test: &TestDb, asset_id: AssetId, place_id: i64) {
    sqlx::query("UPDATE assets SET place_id = $2 WHERE id = $1")
        .bind(asset_id.as_uuid())
        .bind(place_id)
        .execute(test.db().pool())
        .await
        .unwrap();
}

mod preview {
    use super::*;

    #[tokio::test]
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    async fn a_single_photo_renders_with_index_one() {
        let test = TestDb::start().await;
        let admin = harness::seed_admin(&test).await;
        let root = temp_library_root("single");
        let library = seed_library_at(&test, admin, &root).await;
        let ctx = AuthContext::user(admin, SystemRole::Admin);
        let folders = FolderRepo::new(test.db());
        let assets = AssetRepo::new(test.db());

        let folder = folders.ensure_path(library, &["2024"]).await.unwrap();
        let asset_id = indexed_asset(
            &assets,
            folder.id,
            "DSC08421.jpg",
            Utc.with_ymd_and_hms(2026, 8, 14, 12, 0, 0).unwrap(),
        )
        .await;
        set_title(&test, asset_id, "Tramonto").await;

        let items = RenameRepo::new(test.db())
            .preview(&ctx, &[asset_id], "{data}_{titolo}_{n:3}")
            .await
            .unwrap();

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].new_name, "2026-08-14_Tramonto_001.JPG");
        assert!(!items[0].collides);
    }

    #[tokio::test]
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    async fn a_selection_counts_up_in_array_order() {
        let test = TestDb::start().await;
        let admin = harness::seed_admin(&test).await;
        let root = temp_library_root("selection");
        let library = seed_library_at(&test, admin, &root).await;
        let ctx = AuthContext::user(admin, SystemRole::Admin);
        let folders = FolderRepo::new(test.db());
        let assets = AssetRepo::new(test.db());

        let folder = folders.ensure_path(library, &["2024"]).await.unwrap();
        let first = indexed_asset(
            &assets,
            folder.id,
            "a.jpg",
            Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
        )
        .await;
        let second = indexed_asset(
            &assets,
            folder.id,
            "b.jpg",
            Utc.with_ymd_and_hms(2026, 1, 2, 0, 0, 0).unwrap(),
        )
        .await;

        // L'ordine dell'ambito è quello dell'array passato, non l'ordine di
        // creazione: passo `second` prima di `first`.
        let items = RenameRepo::new(test.db())
            .preview(&ctx, &[second, first], "IMG_{n:2}")
            .await
            .unwrap();

        assert_eq!(items.len(), 2);
        let by_id = |id: AssetId| items.iter().find(|i| i.asset_id == id).unwrap();
        assert_eq!(by_id(second).new_name, "IMG_01.JPG");
        assert_eq!(by_id(first).new_name, "IMG_02.JPG");
    }

    #[tokio::test]
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    async fn stack_siblings_are_pulled_in_and_share_one_counter_slot() {
        let test = TestDb::start().await;
        let admin = harness::seed_admin(&test).await;
        let root = temp_library_root("stack");
        let library = seed_library_at(&test, admin, &root).await;
        let ctx = AuthContext::user(admin, SystemRole::Admin);
        let folders = FolderRepo::new(test.db());
        let assets = AssetRepo::new(test.db());

        let folder = folders.ensure_path(library, &["2024"]).await.unwrap();
        let taken_at = Utc.with_ymd_and_hms(2026, 8, 14, 0, 0, 0).unwrap();
        let raw = indexed_asset(&assets, folder.id, "DSC_0042.ARW", taken_at).await;
        let jpeg = indexed_asset(&assets, folder.id, "DSC_0042.JPG", taken_at).await;
        StackRepo::new(test.db())
            .regroup_folder(folder.id)
            .await
            .unwrap();
        // Un terzo asset, indipendente: verifica che la pila non "consumi"
        // più di un posto nel contatore condiviso con foto non impilate.
        let other = indexed_asset(&assets, folder.id, "c.jpg", taken_at).await;
        set_title(&test, raw, "Alba").await;
        set_title(&test, jpeg, "Non dovrebbe contare: solo il primario").await;

        // Solo il RAW è passato esplicitamente: il JPEG affiancato deve
        // comparire comunque nel risultato, con lo stesso nome base.
        let items = RenameRepo::new(test.db())
            .preview(&ctx, &[raw, other], "{titolo}_{n:2}")
            .await
            .unwrap();

        assert_eq!(items.len(), 3, "raw + jpeg affiancato + other");
        let by_id = |id: AssetId| items.iter().find(|i| i.asset_id == id).unwrap();
        assert_eq!(
            by_id(raw).new_name,
            "Alba_01.ARW",
            "il RAW è il primario della pila: il suo titolo vince"
        );
        assert_eq!(
            by_id(jpeg).new_name,
            "Alba_01.JPG",
            "stessa base del RAW, propria estensione — non il proprio titolo"
        );
        assert_eq!(
            by_id(other).new_name,
            "02.JPG",
            "'other' occupa lo slot 2, non 3: la pila ha contato come un solo elemento \
             (titolo vuoto lascia solo il numero, il Task 7 rifila il separatore orfano davanti)"
        );
    }

    #[tokio::test]
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    async fn place_resolves_through_the_places_catalog() {
        let test = TestDb::start().await;
        let admin = harness::seed_admin(&test).await;
        let root = temp_library_root("place");
        let library = seed_library_at(&test, admin, &root).await;
        let ctx = AuthContext::user(admin, SystemRole::Admin);
        let folders = FolderRepo::new(test.db());
        let assets = AssetRepo::new(test.db());

        let folder = folders.ensure_path(library, &["2024"]).await.unwrap();
        let asset_id = indexed_asset(
            &assets,
            folder.id,
            "a.jpg",
            Utc.with_ymd_and_hms(2026, 8, 14, 0, 0, 0).unwrap(),
        )
        .await;
        seed_place(&test, 1, "Val d'Orcia").await;
        set_place(&test, asset_id, 1).await;

        let items = RenameRepo::new(test.db())
            .preview(&ctx, &[asset_id], "{luogo}")
            .await
            .unwrap();

        assert_eq!(items[0].new_name, "Val-d'Orcia.JPG");
    }

    #[tokio::test]
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    async fn camera_and_lens_come_from_exif_and_get_slugified() {
        let test = TestDb::start().await;
        let admin = harness::seed_admin(&test).await;
        let root = temp_library_root("exif");
        let library = seed_library_at(&test, admin, &root).await;
        let ctx = AuthContext::user(admin, SystemRole::Admin);
        let folders = FolderRepo::new(test.db());
        let assets = AssetRepo::new(test.db());

        let folder = folders.ensure_path(library, &["2024"]).await.unwrap();
        let asset_id = indexed_asset(
            &assets,
            folder.id,
            "a.jpg",
            Utc.with_ymd_and_hms(2026, 8, 14, 0, 0, 0).unwrap(),
        )
        .await;
        set_exif(&test, asset_id, "Sony A7 IV", "FE 24-70mm f/2.8").await;

        let items = RenameRepo::new(test.db())
            .preview(&ctx, &[asset_id], "{fotocamera}_{obiettivo}")
            .await
            .unwrap();

        assert_eq!(
            items[0].new_name, "Sony-A7-IV_FE-24-70mm-f-28.JPG",
            "slug() elimina anche il punto dentro \"f/2.8\" (spec: rimuove . e ,, non solo gli spazi)"
        );
    }

    #[tokio::test]
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    async fn two_assets_landing_on_the_same_name_collide_within_the_group() {
        let test = TestDb::start().await;
        let admin = harness::seed_admin(&test).await;
        let root = temp_library_root("collide-group");
        let library = seed_library_at(&test, admin, &root).await;
        let ctx = AuthContext::user(admin, SystemRole::Admin);
        let folders = FolderRepo::new(test.db());
        let assets = AssetRepo::new(test.db());

        let folder = folders.ensure_path(library, &["2024"]).await.unwrap();
        let taken_at = Utc.with_ymd_and_hms(2026, 8, 14, 0, 0, 0).unwrap();
        let a = indexed_asset(&assets, folder.id, "a.jpg", taken_at).await;
        let b = indexed_asset(&assets, folder.id, "b.jpg", taken_at).await;

        // Schema fisso, nessun {n}: entrambe le foto producono lo stesso nome.
        let items = RenameRepo::new(test.db())
            .preview(&ctx, &[a, b], "{data}")
            .await
            .unwrap();

        assert!(items.iter().all(|i| i.collides));
    }

    #[tokio::test]
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    async fn colliding_with_an_asset_outside_the_group_is_still_flagged() {
        // Difetto 1 (spec §62.3d): il prototipo controllava solo dentro il
        // gruppo. Qui "outside" non è nemmeno passato a preview.
        let test = TestDb::start().await;
        let admin = harness::seed_admin(&test).await;
        let root = temp_library_root("collide-outside");
        let library = seed_library_at(&test, admin, &root).await;
        let ctx = AuthContext::user(admin, SystemRole::Admin);
        let folders = FolderRepo::new(test.db());
        let assets = AssetRepo::new(test.db());

        let folder = folders.ensure_path(library, &["2024"]).await.unwrap();
        let taken_at = Utc.with_ymd_and_hms(2026, 8, 14, 0, 0, 0).unwrap();
        // Estensione maiuscola: è sempre come render_filename la produce
        // (spec §62.3b punto 1), quindi è la forma esatta con cui un
        // asset "target.JPG" già presente collide con lo schema "target".
        let _outside = indexed_asset(&assets, folder.id, "target.JPG", taken_at).await;
        let moving = indexed_asset(&assets, folder.id, "a.jpg", taken_at).await;

        let items = RenameRepo::new(test.db())
            .preview(&ctx, &[moving], "target")
            .await
            .unwrap();

        assert_eq!(items.len(), 1);
        assert!(
            items[0].collides,
            "target.JPG esiste già nella cartella, fuori da questo ambito"
        );
    }

    #[tokio::test]
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    async fn a_no_op_rename_is_never_flagged_as_its_own_collision() {
        let test = TestDb::start().await;
        let admin = harness::seed_admin(&test).await;
        let root = temp_library_root("no-op");
        let library = seed_library_at(&test, admin, &root).await;
        let ctx = AuthContext::user(admin, SystemRole::Admin);
        let folders = FolderRepo::new(test.db());
        let assets = AssetRepo::new(test.db());

        let folder = folders.ensure_path(library, &["2024"]).await.unwrap();
        // Estensione già maiuscola: uno schema che ricalcola esattamente lo
        // stesso nome è un vero no-op solo se la forma coincide byte per
        // byte con quella (sempre maiuscola) che render_filename produce.
        let asset_id = indexed_asset(
            &assets,
            folder.id,
            "keep.JPG",
            Utc.with_ymd_and_hms(2026, 8, 14, 0, 0, 0).unwrap(),
        )
        .await;

        let items = RenameRepo::new(test.db())
            .preview(&ctx, &[asset_id], "keep")
            .await
            .unwrap();

        assert_eq!(items[0].new_name, items[0].current_name);
        assert!(!items[0].collides);
    }

    #[tokio::test]
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    async fn a_viewer_cannot_preview() {
        let test = TestDb::start().await;
        let admin = harness::seed_admin(&test).await;
        let root = temp_library_root("viewer-preview");
        let library = seed_library_at(&test, admin, &root).await;
        let ctx = AuthContext::user(admin, SystemRole::Admin);
        let folders = FolderRepo::new(test.db());
        let assets = AssetRepo::new(test.db());

        let folder = folders.ensure_path(library, &["2024"]).await.unwrap();
        let asset_id = indexed_asset(
            &assets,
            folder.id,
            "a.jpg",
            Utc.with_ymd_and_hms(2026, 8, 14, 0, 0, 0).unwrap(),
        )
        .await;

        let viewer = harness::seed_user(&test, admin, "viewer").await;
        PermissionRepo::new(test.db())
            .grant(
                &ctx,
                NewGrant {
                    subject: SubjectType::User,
                    subject_id: viewer.as_uuid(),
                    object: ObjectType::Folder,
                    object_id: folder.id.as_uuid(),
                    role: ObjectRole::Viewer,
                    inherit: true,
                },
            )
            .await
            .unwrap();
        let viewer_ctx = AuthContext::user(viewer, SystemRole::User);

        let result = RenameRepo::new(test.db())
            .preview(&viewer_ctx, &[asset_id], "x")
            .await;

        assert!(matches!(result, Err(DbError::Forbidden)), "{result:?}");
    }
}

mod apply {
    use super::*;

    #[tokio::test]
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    async fn renames_the_file_on_disk_and_records_a_batch() {
        let test = TestDb::start().await;
        let admin = harness::seed_admin(&test).await;
        let root = temp_library_root("apply-basic");
        let library = seed_library_at(&test, admin, &root).await;
        let ctx = AuthContext::user(admin, SystemRole::Admin);
        let folders = FolderRepo::new(test.db());
        let assets = AssetRepo::new(test.db());

        let folder = folders.ensure_path(library, &["2024"]).await.unwrap();
        fs::create_dir_all(root.join("2024")).unwrap();
        fs::write(root.join("2024").join("a.jpg"), b"x").unwrap();
        let asset_id = indexed_asset(
            &assets,
            folder.id,
            "a.jpg",
            Utc.with_ymd_and_hms(2026, 8, 14, 0, 0, 0).unwrap(),
        )
        .await;
        set_title(&test, asset_id, "Tramonto").await;

        let outcome = RenameRepo::new(test.db())
            .apply(&ctx, &[asset_id], "{titolo}", None)
            .await
            .unwrap();

        assert_eq!(outcome.renamed.len(), 1);
        assert!(outcome.failed.is_empty());
        assert!(outcome.batch_id.is_some());
        assert!(root.join("2024").join("Tramonto.JPG").is_file());
        assert!(!root.join("2024").join("a.jpg").exists());

        let by_id = assets.find_by_id(&ctx, asset_id).await.unwrap();
        assert_eq!(by_id.filename.as_str(), "Tramonto.JPG");

        let _ = fs::remove_dir_all(&root);
    }

    #[tokio::test]
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    async fn stack_siblings_are_renamed_together_on_disk() {
        let test = TestDb::start().await;
        let admin = harness::seed_admin(&test).await;
        let root = temp_library_root("apply-stack");
        let library = seed_library_at(&test, admin, &root).await;
        let ctx = AuthContext::user(admin, SystemRole::Admin);
        let folders = FolderRepo::new(test.db());
        let assets = AssetRepo::new(test.db());

        let folder = folders.ensure_path(library, &["2024"]).await.unwrap();
        fs::create_dir_all(root.join("2024")).unwrap();
        fs::write(root.join("2024").join("DSC_0042.ARW"), b"raw").unwrap();
        fs::write(root.join("2024").join("DSC_0042.JPG"), b"jpeg").unwrap();
        let taken_at = Utc.with_ymd_and_hms(2026, 8, 14, 0, 0, 0).unwrap();
        let raw = indexed_asset(&assets, folder.id, "DSC_0042.ARW", taken_at).await;
        let jpeg = indexed_asset(&assets, folder.id, "DSC_0042.JPG", taken_at).await;
        StackRepo::new(test.db())
            .regroup_folder(folder.id)
            .await
            .unwrap();
        set_title(&test, raw, "Alba").await;

        let outcome = RenameRepo::new(test.db())
            .apply(&ctx, &[raw], "{titolo}", None)
            .await
            .unwrap();

        assert_eq!(
            outcome.renamed.len(),
            2,
            "il JPEG affiancato si rinomina insieme al RAW anche se non passato esplicitamente"
        );
        assert!(root.join("2024").join("Alba.ARW").is_file());
        assert!(root.join("2024").join("Alba.JPG").is_file());
        let jpeg_row = assets.find_by_id(&ctx, jpeg).await.unwrap();
        assert_eq!(jpeg_row.filename.as_str(), "Alba.JPG");

        let _ = fs::remove_dir_all(&root);
    }

    #[tokio::test]
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    async fn a_within_group_collision_fails_both_and_records_nothing() {
        let test = TestDb::start().await;
        let admin = harness::seed_admin(&test).await;
        let root = temp_library_root("apply-collide");
        let library = seed_library_at(&test, admin, &root).await;
        let ctx = AuthContext::user(admin, SystemRole::Admin);
        let folders = FolderRepo::new(test.db());
        let assets = AssetRepo::new(test.db());

        let folder = folders.ensure_path(library, &["2024"]).await.unwrap();
        fs::create_dir_all(root.join("2024")).unwrap();
        fs::write(root.join("2024").join("a.jpg"), b"a").unwrap();
        fs::write(root.join("2024").join("b.jpg"), b"b").unwrap();
        let taken_at = Utc.with_ymd_and_hms(2026, 8, 14, 0, 0, 0).unwrap();
        let a = indexed_asset(&assets, folder.id, "a.jpg", taken_at).await;
        let b = indexed_asset(&assets, folder.id, "b.jpg", taken_at).await;

        let outcome = RenameRepo::new(test.db())
            .apply(&ctx, &[a, b], "{data}", None)
            .await
            .unwrap();

        // move_asset stesso rifiuta la collisione al secondo tentativo: il
        // primo elaborato riesce a occupare il nome, il secondo la trova
        // già presa — riuscita parziale, non un blocco totale silenzioso.
        assert_eq!(outcome.renamed.len(), 1);
        assert_eq!(outcome.failed.len(), 1);
        assert!(matches!(outcome.failed[0].1, DbError::Collision(_)));

        let _ = fs::remove_dir_all(&root);
    }

    #[tokio::test]
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    async fn a_single_unwritable_asset_in_the_scope_rejects_the_whole_call() {
        let test = TestDb::start().await;
        let admin = harness::seed_admin(&test).await;
        let root = temp_library_root("apply-forbidden");
        let library = seed_library_at(&test, admin, &root).await;
        let ctx = AuthContext::user(admin, SystemRole::Admin);
        let folders = FolderRepo::new(test.db());
        let assets = AssetRepo::new(test.db());

        let editable = folders.ensure_path(library, &["Editabile"]).await.unwrap();
        let locked = folders.ensure_path(library, &["Bloccata"]).await.unwrap();
        fs::create_dir_all(root.join("Editabile")).unwrap();
        fs::create_dir_all(root.join("Bloccata")).unwrap();
        fs::write(root.join("Editabile").join("a.jpg"), b"a").unwrap();
        fs::write(root.join("Bloccata").join("b.jpg"), b"b").unwrap();
        let taken_at = Utc.with_ymd_and_hms(2026, 8, 14, 0, 0, 0).unwrap();
        let a = indexed_asset(&assets, editable.id, "a.jpg", taken_at).await;
        let b = indexed_asset(&assets, locked.id, "b.jpg", taken_at).await;

        let editor = harness::seed_user(&test, admin, "editor").await;
        PermissionRepo::new(test.db())
            .grant(
                &ctx,
                NewGrant {
                    subject: SubjectType::User,
                    subject_id: editor.as_uuid(),
                    object: ObjectType::Folder,
                    object_id: editable.id.as_uuid(),
                    role: ObjectRole::Editor,
                    inherit: true,
                },
            )
            .await
            .unwrap();
        PermissionRepo::new(test.db())
            .grant(
                &ctx,
                NewGrant {
                    subject: SubjectType::User,
                    subject_id: editor.as_uuid(),
                    object: ObjectType::Folder,
                    object_id: locked.id.as_uuid(),
                    role: ObjectRole::Viewer,
                    inherit: true,
                },
            )
            .await
            .unwrap();
        let editor_ctx = AuthContext::user(editor, SystemRole::User);

        // compute() pretende editor su tutto l'ambito (assert_can_edit_assets):
        // un solo asset di sola visione basta a rifiutare l'intera chiamata,
        // prima ancora di tentare il primo move_asset.
        let result = RenameRepo::new(test.db())
            .apply(&editor_ctx, &[a, b], "x", None)
            .await;

        assert!(matches!(result, Err(DbError::Forbidden)), "{result:?}");
        assert!(
            root.join("Editabile").join("a.jpg").is_file(),
            "niente si muove"
        );

        let _ = fs::remove_dir_all(&root);
    }
}

mod undo {
    use super::*;

    #[tokio::test]
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    async fn restores_the_previous_filename_on_disk_and_in_the_row() {
        let test = TestDb::start().await;
        let admin = harness::seed_admin(&test).await;
        let root = temp_library_root("undo-basic");
        let library = seed_library_at(&test, admin, &root).await;
        let ctx = AuthContext::user(admin, SystemRole::Admin);
        let folders = FolderRepo::new(test.db());
        let assets = AssetRepo::new(test.db());

        let folder = folders.ensure_path(library, &["2024"]).await.unwrap();
        fs::create_dir_all(root.join("2024")).unwrap();
        fs::write(root.join("2024").join("a.jpg"), b"contenuto").unwrap();
        let asset_id = indexed_asset(
            &assets,
            folder.id,
            "a.jpg",
            Utc.with_ymd_and_hms(2026, 8, 14, 0, 0, 0).unwrap(),
        )
        .await;
        set_title(&test, asset_id, "Tramonto").await;

        let repo = RenameRepo::new(test.db());
        let applied = repo
            .apply(&ctx, &[asset_id], "{titolo}", None)
            .await
            .unwrap();
        let batch_id = applied.batch_id.unwrap();
        assert!(root.join("2024").join("Tramonto.JPG").is_file());

        let undone = repo.undo(&ctx, batch_id, false).await.unwrap();

        assert!(!undone.already_undone);
        assert_eq!(undone.restored.len(), 1);
        assert!(undone.failed.is_empty());
        assert!(root.join("2024").join("a.jpg").is_file());
        assert_eq!(
            fs::read(root.join("2024").join("a.jpg")).unwrap(),
            b"contenuto"
        );
        assert!(!root.join("2024").join("Tramonto.JPG").exists());
        let row = assets.find_by_id(&ctx, asset_id).await.unwrap();
        assert_eq!(row.filename.as_str(), "a.jpg");

        let _ = fs::remove_dir_all(&root);
    }

    #[tokio::test]
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    async fn restores_stack_siblings_together() {
        let test = TestDb::start().await;
        let admin = harness::seed_admin(&test).await;
        let root = temp_library_root("undo-stack");
        let library = seed_library_at(&test, admin, &root).await;
        let ctx = AuthContext::user(admin, SystemRole::Admin);
        let folders = FolderRepo::new(test.db());
        let assets = AssetRepo::new(test.db());

        let folder = folders.ensure_path(library, &["2024"]).await.unwrap();
        fs::create_dir_all(root.join("2024")).unwrap();
        fs::write(root.join("2024").join("DSC_0042.ARW"), b"raw").unwrap();
        fs::write(root.join("2024").join("DSC_0042.JPG"), b"jpeg").unwrap();
        let taken_at = Utc.with_ymd_and_hms(2026, 8, 14, 0, 0, 0).unwrap();
        let raw = indexed_asset(&assets, folder.id, "DSC_0042.ARW", taken_at).await;
        let jpeg = indexed_asset(&assets, folder.id, "DSC_0042.JPG", taken_at).await;
        StackRepo::new(test.db())
            .regroup_folder(folder.id)
            .await
            .unwrap();
        set_title(&test, raw, "Alba").await;

        let repo = RenameRepo::new(test.db());
        let applied = repo.apply(&ctx, &[raw], "{titolo}", None).await.unwrap();
        let batch_id = applied.batch_id.unwrap();

        let undone = repo.undo(&ctx, batch_id, false).await.unwrap();

        assert_eq!(undone.restored.len(), 2, "raw + jpeg affiancato");
        assert!(root.join("2024").join("DSC_0042.ARW").is_file());
        assert!(root.join("2024").join("DSC_0042.JPG").is_file());
        let jpeg_row = assets.find_by_id(&ctx, jpeg).await.unwrap();
        assert_eq!(jpeg_row.filename.as_str(), "DSC_0042.JPG");

        let _ = fs::remove_dir_all(&root);
    }

    #[tokio::test]
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    async fn undoing_twice_is_a_no_op_not_an_error() {
        let test = TestDb::start().await;
        let admin = harness::seed_admin(&test).await;
        let root = temp_library_root("undo-twice");
        let library = seed_library_at(&test, admin, &root).await;
        let ctx = AuthContext::user(admin, SystemRole::Admin);
        let folders = FolderRepo::new(test.db());
        let assets = AssetRepo::new(test.db());

        let folder = folders.ensure_path(library, &["2024"]).await.unwrap();
        fs::create_dir_all(root.join("2024")).unwrap();
        fs::write(root.join("2024").join("a.jpg"), b"x").unwrap();
        let asset_id = indexed_asset(
            &assets,
            folder.id,
            "a.jpg",
            Utc.with_ymd_and_hms(2026, 8, 14, 0, 0, 0).unwrap(),
        )
        .await;

        let repo = RenameRepo::new(test.db());
        let applied = repo.apply(&ctx, &[asset_id], "b", None).await.unwrap();
        let batch_id = applied.batch_id.unwrap();

        let first = repo.undo(&ctx, batch_id, false).await.unwrap();
        assert!(!first.already_undone);
        let second = repo.undo(&ctx, batch_id, false).await.unwrap();
        assert!(second.already_undone);
        assert!(second.restored.is_empty());

        let _ = fs::remove_dir_all(&root);
    }

    #[tokio::test]
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    async fn only_the_actor_or_an_admin_can_undo() {
        let test = TestDb::start().await;
        let admin = harness::seed_admin(&test).await;
        let root = temp_library_root("undo-forbidden");
        let library = seed_library_at(&test, admin, &root).await;
        let ctx = AuthContext::user(admin, SystemRole::Admin);
        let folders = FolderRepo::new(test.db());
        let assets = AssetRepo::new(test.db());

        let folder = folders.ensure_path(library, &["2024"]).await.unwrap();
        fs::create_dir_all(root.join("2024")).unwrap();
        fs::write(root.join("2024").join("a.jpg"), b"x").unwrap();
        let asset_id = indexed_asset(
            &assets,
            folder.id,
            "a.jpg",
            Utc.with_ymd_and_hms(2026, 8, 14, 0, 0, 0).unwrap(),
        )
        .await;

        let repo = RenameRepo::new(test.db());
        let applied = repo.apply(&ctx, &[asset_id], "b", None).await.unwrap();
        let batch_id = applied.batch_id.unwrap();

        // Un editor con accesso pieno all'asset, ma non è chi ha applicato
        // il batch: l'annullamento resta personale, non basta poter
        // modificare l'asset.
        let editor = harness::seed_user(&test, admin, "editor").await;
        PermissionRepo::new(test.db())
            .grant(
                &ctx,
                NewGrant {
                    subject: SubjectType::User,
                    subject_id: editor.as_uuid(),
                    object: ObjectType::Folder,
                    object_id: folder.id.as_uuid(),
                    role: ObjectRole::Editor,
                    inherit: true,
                },
            )
            .await
            .unwrap();
        let editor_ctx = AuthContext::user(editor, SystemRole::User);

        let result = repo.undo(&editor_ctx, batch_id, false).await;

        assert!(matches!(result, Err(DbError::Forbidden)), "{result:?}");
        assert!(root.join("2024").join("b.JPG").is_file(), "niente si muove");

        let _ = fs::remove_dir_all(&root);
    }

    #[tokio::test]
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    async fn a_collision_at_the_previous_slot_fails_only_that_asset() {
        let test = TestDb::start().await;
        let admin = harness::seed_admin(&test).await;
        let root = temp_library_root("undo-collide");
        let library = seed_library_at(&test, admin, &root).await;
        let ctx = AuthContext::user(admin, SystemRole::Admin);
        let folders = FolderRepo::new(test.db());
        let assets = AssetRepo::new(test.db());

        let folder = folders.ensure_path(library, &["2024"]).await.unwrap();
        fs::create_dir_all(root.join("2024")).unwrap();
        fs::write(root.join("2024").join("a.jpg"), b"a").unwrap();
        let taken_at = Utc.with_ymd_and_hms(2026, 8, 14, 0, 0, 0).unwrap();
        let asset_id = indexed_asset(&assets, folder.id, "a.jpg", taken_at).await;

        let repo = RenameRepo::new(test.db());
        let applied = repo.apply(&ctx, &[asset_id], "b", None).await.unwrap();
        let batch_id = applied.batch_id.unwrap();
        assert!(root.join("2024").join("b.JPG").is_file());

        // Qualcun altro occupa ora "a.jpg", il vecchio nome dell'asset.
        fs::write(root.join("2024").join("a.jpg"), b"intruso").unwrap();
        let _intruder = indexed_asset(&assets, folder.id, "a.jpg", taken_at).await;

        let undone = repo.undo(&ctx, batch_id, false).await.unwrap();

        assert!(undone.restored.is_empty());
        assert_eq!(undone.failed.len(), 1);
        assert!(matches!(undone.failed[0].1, DbError::Collision(_)));
        assert!(
            root.join("2024").join("b.JPG").is_file(),
            "il file resta dov'era, l'annullamento non ha toccato nulla"
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[tokio::test]
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    async fn an_unknown_batch_is_not_found_for_an_admin() {
        let test = TestDb::start().await;
        let admin = harness::seed_admin(&test).await;
        let ctx = AuthContext::user(admin, SystemRole::Admin);

        let result = RenameRepo::new(test.db())
            .undo(&ctx, keeppix_domain::BatchId::new(), false)
            .await;

        assert!(matches!(result, Err(DbError::NotFound)), "{result:?}");
    }
}

mod operation_tracking {
    use super::*;

    #[tokio::test]
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    async fn apply_reports_total_and_successes_then_finishes_done() {
        let test = TestDb::start().await;
        let admin = harness::seed_admin(&test).await;
        let root = temp_library_root("op-apply");
        let library = seed_library_at(&test, admin, &root).await;
        let ctx = AuthContext::user(admin, SystemRole::Admin);
        let folders = FolderRepo::new(test.db());
        let assets = AssetRepo::new(test.db());
        let operations = OperationsRepo::new(test.db());

        let folder = folders.ensure_path(library, &["2024"]).await.unwrap();
        fs::create_dir_all(root.join("2024")).unwrap();
        fs::write(root.join("2024").join("a.jpg"), b"a").unwrap();
        fs::write(root.join("2024").join("b.jpg"), b"b").unwrap();
        let taken_at = Utc.with_ymd_and_hms(2026, 8, 14, 0, 0, 0).unwrap();
        let a = indexed_asset(&assets, folder.id, "a.jpg", taken_at).await;
        let b = indexed_asset(&assets, folder.id, "b.jpg", taken_at).await;

        // Dal 27 agosto `apply` non crea più la propria operazione (l'ha
        // fatta girare in background, `keeppix-jobs::rename_batch`): il
        // chiamante — qui il test, la rotta HTTP in produzione — la crea
        // prima, come farebbe `apply_batch`.
        let op = operations
            .create(&ctx, keeppix_domain::OperationKind::BulkRename)
            .await
            .unwrap();
        let outcome = RenameRepo::new(test.db())
            .apply(&ctx, &[a, b], "{titolo}_{n:2}", Some(op.id))
            .await
            .unwrap();
        assert_eq!(outcome.renamed.len(), 2);
        let op_id = outcome.operation_id.unwrap();
        assert_eq!(op_id, op.id);

        let finished = operations.find(&ctx, op_id).await.unwrap();
        assert_eq!(finished.status, OperationStatus::Done);
        assert_eq!(finished.total, Some(2));
        assert_eq!(finished.done, 2);
        assert_eq!(finished.phase, "renaming");
        let mut succeeded = finished.succeeded.clone();
        succeeded.sort_by_key(AssetId::as_uuid);
        let mut expected = vec![a, b];
        expected.sort_by_key(AssetId::as_uuid);
        assert_eq!(succeeded, expected);

        let _ = fs::remove_dir_all(&root);
    }

    #[tokio::test]
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    async fn apply_stops_partway_when_cancel_is_requested_mid_batch() {
        // Dal 27 agosto il chiamante crea l'operazione prima di invocare
        // apply() (vedi il test sopra), quindi l'id è noto subito — non
        // serve più list_running per scoprirlo. Il test corre apply() su un
        // lotto abbastanza grande da lasciare una finestra reale, e un
        // secondo task interroga find(op_id) finché non vede almeno un
        // successo (`done > 0`, davvero a metà, non annullata prima di
        // iniziare) per chiederne l'annullamento — polling, non un'attesa
        // fissa, per non essere un test instabile.
        let test = TestDb::start().await;
        let admin = harness::seed_admin(&test).await;
        let root = temp_library_root("op-cancel");
        let library = seed_library_at(&test, admin, &root).await;
        let ctx = AuthContext::user(admin, SystemRole::Admin);
        let folders = FolderRepo::new(test.db());
        let assets = AssetRepo::new(test.db());
        let operations = OperationsRepo::new(test.db());

        let folder = folders.ensure_path(library, &["2024"]).await.unwrap();
        fs::create_dir_all(root.join("2024")).unwrap();
        let taken_at = Utc.with_ymd_and_hms(2026, 8, 14, 0, 0, 0).unwrap();
        let mut asset_ids = Vec::new();
        for i in 0..200 {
            let name = format!("a{i:03}.jpg");
            fs::write(root.join("2024").join(&name), b"x").unwrap();
            asset_ids.push(indexed_asset(&assets, folder.id, &name, taken_at).await);
        }

        let op = operations
            .create(&ctx, OperationKind::BulkRename)
            .await
            .unwrap();

        let canceller_ctx = ctx.clone();
        let canceller_db = test.db().clone();
        let op_id = op.id;
        let canceller = tokio::spawn(async move {
            let operations = OperationsRepo::new(&canceller_db);
            for _ in 0..200 {
                if let Ok(op) = operations.find(&canceller_ctx, op_id).await
                    && op.done > 0
                {
                    operations
                        .request_cancel(&canceller_ctx, op_id)
                        .await
                        .unwrap();
                    return true;
                }
                tokio::time::sleep(std::time::Duration::from_millis(2)).await;
            }
            false
        });

        let outcome = RenameRepo::new(test.db())
            .apply(&ctx, &asset_ids, "{n:3}", Some(op.id))
            .await
            .unwrap();
        let found_it = canceller.await.unwrap();

        assert!(
            found_it,
            "il canceller non ha trovato l'operazione in tempo"
        );
        assert!(
            outcome.renamed.len() < asset_ids.len(),
            "l'annullamento deve aver fermato il giro prima della fine: {} rinominati su {}",
            outcome.renamed.len(),
            asset_ids.len()
        );
        let op_id = outcome.operation_id.unwrap();
        let finished = operations.find(&ctx, op_id).await.unwrap();
        assert_eq!(finished.status, OperationStatus::Cancelled);
        assert_eq!(
            finished.done,
            i64::try_from(outcome.renamed.len()).unwrap(),
            "il conteggio dell'operazione e i rinominati restituiti concordano"
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[tokio::test]
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    async fn undo_reports_progress_and_finishes_done() {
        let test = TestDb::start().await;
        let admin = harness::seed_admin(&test).await;
        let root = temp_library_root("op-undo");
        let library = seed_library_at(&test, admin, &root).await;
        let ctx = AuthContext::user(admin, SystemRole::Admin);
        let folders = FolderRepo::new(test.db());
        let assets = AssetRepo::new(test.db());
        let operations = OperationsRepo::new(test.db());

        let folder = folders.ensure_path(library, &["2024"]).await.unwrap();
        fs::create_dir_all(root.join("2024")).unwrap();
        fs::write(root.join("2024").join("a.jpg"), b"a").unwrap();
        let taken_at = Utc.with_ymd_and_hms(2026, 8, 14, 0, 0, 0).unwrap();
        let asset_id = indexed_asset(&assets, folder.id, "a.jpg", taken_at).await;

        let repo = RenameRepo::new(test.db());
        let applied = repo.apply(&ctx, &[asset_id], "b", None).await.unwrap();
        let batch_id = applied.batch_id.unwrap();

        let undone = repo.undo(&ctx, batch_id, true).await.unwrap();
        assert_eq!(undone.restored.len(), 1);

        let op_id = undone.operation_id.unwrap();
        let finished = operations.find(&ctx, op_id).await.unwrap();
        assert_eq!(finished.status, OperationStatus::Done);
        assert_eq!(finished.total, Some(1));
        assert_eq!(finished.done, 1);
        assert_eq!(finished.phase, "undoing");

        let _ = fs::remove_dir_all(&root);
    }
}
