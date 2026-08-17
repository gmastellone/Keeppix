#![allow(clippy::unwrap_used, clippy::expect_used)]

mod harness;

use chrono::{TimeZone, Utc};
use harness::TestDb;
use keeppix_db::{
    AssetRepo, DbError, FolderRepo, LibraryRepo, ObjectType, PermissionRepo, SearchNode,
    SearchRepo, SubjectType, TimelineRepo,
};
use keeppix_domain::{
    AssetId, AssetKind, AssetName, AssetStatus, AuthContext, Folder, FolderId, GroupId, LibraryId,
    NewAsset, NewLibrary, ObjectRole, SystemRole, UserId,
};

async fn seed_library(test: &TestDb, owner: UserId, name: &str, path: &str) -> LibraryId {
    LibraryRepo::new(test.db())
        .create(
            &AuthContext::user(owner, SystemRole::Admin),
            NewLibrary {
                name: name.to_owned(),
                owner_id: owner,
                root_path: std::path::PathBuf::from(path),
                exclude_patterns: vec![],
            },
        )
        .await
        .unwrap()
        .id
}

fn photo(folder: FolderId, name: &str) -> NewAsset {
    NewAsset {
        folder_id: folder,
        filename: AssetName::parse(name).unwrap(),
        size_bytes: 10,
        mtime: Utc.with_ymd_and_hms(2024, 7, 1, 0, 0, 0).unwrap(),
        inode: Some(1),
        kind: AssetKind::Image,
    }
}

async fn index_photo(test: &TestDb, folder: FolderId, name: &str) -> AssetId {
    let repo = AssetRepo::new(test.db());
    let asset = repo
        .upsert_discovered(photo(folder, name))
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

async fn grant_folder(
    test: &TestDb,
    granter: UserId,
    subject: SubjectType,
    subject_id: uuid::Uuid,
    folder: FolderId,
    role: ObjectRole,
    inherit: bool,
) {
    PermissionRepo::new(test.db())
        .grant(
            &AuthContext::user(granter, SystemRole::Admin),
            keeppix_db::NewGrant {
                subject,
                subject_id,
                object: ObjectType::Folder,
                object_id: folder.as_uuid(),
                role,
                inherit,
            },
        )
        .await
        .unwrap();
}

async fn insert_group(test: &TestDb, name: &str, created_by: UserId) -> GroupId {
    let id = uuid::Uuid::now_v7();
    sqlx::query("INSERT INTO groups (id, name, created_by) VALUES ($1, $2, $3)")
        .bind(id)
        .bind(name)
        .bind(created_by.as_uuid())
        .execute(test.db().pool())
        .await
        .unwrap();
    GroupId::from_uuid(id)
}

async fn add_member(test: &TestDb, group: GroupId, user: UserId) {
    sqlx::query("INSERT INTO group_members (group_id, user_id) VALUES ($1, $2)")
        .bind(group.as_uuid())
        .bind(user.as_uuid())
        .execute(test.db().pool())
        .await
        .unwrap();
}

async fn remove_member(test: &TestDb, group: GroupId, user: UserId) {
    sqlx::query("DELETE FROM group_members WHERE group_id = $1 AND user_id = $2")
        .bind(group.as_uuid())
        .bind(user.as_uuid())
        .execute(test.db().pool())
        .await
        .unwrap();
}

async fn visible_indexed(test: &TestDb, ctx: &AuthContext) -> i64 {
    AssetRepo::new(test.db())
        .count_by_status(ctx, AssetStatus::Indexed)
        .await
        .unwrap()
}

fn folder_names(folders: &[Folder]) -> Vec<&str> {
    folders.iter().map(|f| f.name.as_str()).collect()
}

/// Il fallimento peggiore possibile di una funzione di visibilità è
/// APRIRSI invece di chiudersi. Va testato per primo, e su ogni repository.
#[tokio::test]
async fn a_user_with_no_permissions_sees_zero_assets_everywhere() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let stranger = harness::seed_user(&test, admin, "estraneo").await;
    let library = seed_library(&test, admin, "Foto", "/mnt/nas/foto").await;
    let folder = FolderRepo::new(test.db())
        .ensure_path(library, &["2024"])
        .await
        .unwrap();
    for i in 0..20 {
        index_photo(&test, folder.id, &format!("DSC_{i:04}.ARW")).await;
    }

    let ctx = AuthContext::user(stranger, SystemRole::User);
    assert_eq!(visible_indexed(&test, &ctx).await, 0);
    assert!(
        TimelineRepo::new(test.db())
            .buckets(&ctx, None)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        SearchRepo::new(test.db())
            .run(&ctx, &SearchNode::And { args: vec![] }, None, 200)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        FolderRepo::new(test.db()).tree(&ctx).await.unwrap().len(),
        0
    );
}

#[tokio::test]
async fn sharing_a_folder_grants_its_whole_subtree() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let mario = harness::seed_user(&test, admin, "mario").await;
    let library = seed_library(&test, admin, "Foto", "/mnt/nas/foto").await;
    let folders = FolderRepo::new(test.db());
    let y2024 = folders.ensure_path(library, &["2024"]).await.unwrap();
    let grecia = folders
        .ensure_path(library, &["2024", "Grecia"])
        .await
        .unwrap();
    let santorini = folders
        .ensure_path(library, &["2024", "Grecia", "Santorini"])
        .await
        .unwrap();
    let y2023 = folders.ensure_path(library, &["2023"]).await.unwrap();

    let in_2024 = index_photo(&test, y2024.id, "a.jpg").await;
    let in_grecia = index_photo(&test, grecia.id, "b.jpg").await;
    let in_santorini = index_photo(&test, santorini.id, "c.jpg").await;
    let in_2023 = index_photo(&test, y2023.id, "d.jpg").await;

    grant_folder(
        &test,
        admin,
        SubjectType::User,
        mario.as_uuid(),
        y2024.id,
        ObjectRole::Viewer,
        true,
    )
    .await;

    let ctx = AuthContext::user(mario, SystemRole::User);
    assert_eq!(visible_indexed(&test, &ctx).await, 3);

    let tree = folders.tree(&ctx).await.unwrap();
    let names = folder_names(&tree);
    assert!(names.contains(&"2024"));
    assert!(names.contains(&"Grecia"));
    assert!(names.contains(&"Santorini"));
    assert!(!names.contains(&"2023"));

    let assets = AssetRepo::new(test.db());
    assert!(assets.find_by_id(&ctx, in_2024).await.is_ok());
    assert!(assets.find_by_id(&ctx, in_grecia).await.is_ok());
    assert!(assets.find_by_id(&ctx, in_santorini).await.is_ok());
    assert!(matches!(
        assets.find_by_id(&ctx, in_2023).await,
        Err(DbError::Forbidden)
    ));
}

#[tokio::test]
async fn a_shared_viewer_cannot_move_the_folder() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let mario = harness::seed_user(&test, admin, "mario").await;
    let library = seed_library(&test, admin, "Foto", "/mnt/nas/foto").await;
    let folders = FolderRepo::new(test.db());
    let y2024 = folders.ensure_path(library, &["2024"]).await.unwrap();
    let grecia = folders
        .ensure_path(library, &["2024", "Grecia"])
        .await
        .unwrap();
    grant_folder(
        &test,
        admin,
        SubjectType::User,
        mario.as_uuid(),
        y2024.id,
        ObjectRole::Viewer,
        true,
    )
    .await;

    let ctx = AuthContext::user(mario, SystemRole::User);
    assert!(matches!(
        folders.move_subtree(&ctx, grecia.id, y2024.id).await,
        Err(DbError::Forbidden)
    ));
}

#[tokio::test]
async fn roots_for_a_share_are_the_granted_folder_not_the_library() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let mario = harness::seed_user(&test, admin, "mario").await;
    let library = seed_library(&test, admin, "Foto", "/mnt/nas/foto").await;
    let folders = FolderRepo::new(test.db());
    let y2024 = folders.ensure_path(library, &["2024"]).await.unwrap();
    folders
        .ensure_path(library, &["2024", "Grecia"])
        .await
        .unwrap();
    grant_folder(
        &test,
        admin,
        SubjectType::User,
        mario.as_uuid(),
        y2024.id,
        ObjectRole::Viewer,
        true,
    )
    .await;

    let ctx = AuthContext::user(mario, SystemRole::User);
    let roots = folders.roots(&ctx).await.unwrap();
    let names = folder_names(&roots);
    assert_eq!(names, vec!["2024"]);
    assert!(!names.contains(&"Grecia"));
}

#[tokio::test]
async fn inherit_false_stops_the_subtree_at_that_node() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let mario = harness::seed_user(&test, admin, "mario").await;
    let library = seed_library(&test, admin, "Foto", "/mnt/nas/foto").await;
    let folders = FolderRepo::new(test.db());
    let foto = folders.ensure_path(library, &["Foto"]).await.unwrap();
    let visibile = folders
        .ensure_path(library, &["Foto", "Spiaggia"])
        .await
        .unwrap();
    let privato = folders
        .ensure_path(library, &["Foto", "Privato"])
        .await
        .unwrap();
    let figlio = folders
        .ensure_path(library, &["Foto", "Privato", "Diario"])
        .await
        .unwrap();

    let seen = index_photo(&test, visibile.id, "mare.jpg").await;
    let hidden = index_photo(&test, privato.id, "segreto.jpg").await;
    let hidden_child = index_photo(&test, figlio.id, "diario.jpg").await;

    grant_folder(
        &test,
        admin,
        SubjectType::User,
        mario.as_uuid(),
        foto.id,
        ObjectRole::Viewer,
        true,
    )
    .await;
    grant_folder(
        &test,
        admin,
        SubjectType::User,
        mario.as_uuid(),
        privato.id,
        ObjectRole::Viewer,
        false,
    )
    .await;

    let ctx = AuthContext::user(mario, SystemRole::User);
    let assets = AssetRepo::new(test.db());
    assert!(assets.find_by_id(&ctx, seen).await.is_ok());
    assert!(matches!(
        assets.find_by_id(&ctx, hidden).await,
        Err(DbError::Forbidden)
    ));
    assert!(matches!(
        assets.find_by_id(&ctx, hidden_child).await,
        Err(DbError::Forbidden)
    ));

    let tree = folders.tree(&ctx).await.unwrap();
    let names = folder_names(&tree);
    assert!(names.contains(&"Foto"));
    assert!(names.contains(&"Spiaggia"));
    assert!(!names.contains(&"Privato"));
    assert!(!names.contains(&"Diario"));
}

#[tokio::test]
async fn a_group_permission_applies_to_every_member() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let anna = harness::seed_user(&test, admin, "anna").await;
    let luca = harness::seed_user(&test, admin, "luca").await;
    let library = seed_library(&test, admin, "Foto", "/mnt/nas/foto").await;
    let folder = FolderRepo::new(test.db())
        .ensure_path(library, &["Vacanze"])
        .await
        .unwrap();
    index_photo(&test, folder.id, "a.jpg").await;

    let famiglia = insert_group(&test, "Famiglia", admin).await;
    add_member(&test, famiglia, anna).await;
    add_member(&test, famiglia, luca).await;
    grant_folder(
        &test,
        admin,
        SubjectType::Group,
        famiglia.as_uuid(),
        folder.id,
        ObjectRole::Viewer,
        true,
    )
    .await;

    assert_eq!(
        visible_indexed(&test, &AuthContext::user(anna, SystemRole::User)).await,
        1
    );
    assert_eq!(
        visible_indexed(&test, &AuthContext::user(luca, SystemRole::User)).await,
        1
    );
}

#[tokio::test]
async fn adding_a_member_grants_access_immediately() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let quarto = harness::seed_user(&test, admin, "quarto").await;
    let library = seed_library(&test, admin, "Foto", "/mnt/nas/foto").await;
    let folder = FolderRepo::new(test.db())
        .ensure_path(library, &["Vacanze"])
        .await
        .unwrap();
    index_photo(&test, folder.id, "a.jpg").await;

    let famiglia = insert_group(&test, "Famiglia", admin).await;
    grant_folder(
        &test,
        admin,
        SubjectType::Group,
        famiglia.as_uuid(),
        folder.id,
        ObjectRole::Viewer,
        true,
    )
    .await;

    let ctx = AuthContext::user(quarto, SystemRole::User);
    assert_eq!(visible_indexed(&test, &ctx).await, 0);
    add_member(&test, famiglia, quarto).await;
    assert_eq!(visible_indexed(&test, &ctx).await, 1);
}

/// Smaschera un elenco di gruppi trasportato nel token invece che derivato.
#[tokio::test]
async fn removing_a_member_revokes_access_immediately() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let mario = harness::seed_user(&test, admin, "mario").await;
    let library = seed_library(&test, admin, "Foto", "/mnt/nas/foto").await;
    let folder = FolderRepo::new(test.db())
        .ensure_path(library, &["Vacanze"])
        .await
        .unwrap();
    index_photo(&test, folder.id, "a.jpg").await;

    let famiglia = insert_group(&test, "Famiglia", admin).await;
    add_member(&test, famiglia, mario).await;
    grant_folder(
        &test,
        admin,
        SubjectType::Group,
        famiglia.as_uuid(),
        folder.id,
        ObjectRole::Viewer,
        true,
    )
    .await;

    let ctx = AuthContext::user(mario, SystemRole::User);
    assert_eq!(visible_indexed(&test, &ctx).await, 1);
    remove_member(&test, famiglia, mario).await;
    assert_eq!(
        visible_indexed(&test, &ctx).await,
        0,
        "stesso AuthContext, niente nuovo login: i gruppi non stanno nel token"
    );
}

#[tokio::test]
async fn the_highest_role_wins() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let mario = harness::seed_user(&test, admin, "mario").await;
    let library = seed_library(&test, admin, "Foto", "/mnt/nas/foto").await;
    let folder = FolderRepo::new(test.db())
        .ensure_path(library, &["Studio"])
        .await
        .unwrap();

    let famiglia = insert_group(&test, "Famiglia", admin).await;
    add_member(&test, famiglia, mario).await;
    grant_folder(
        &test,
        admin,
        SubjectType::Group,
        famiglia.as_uuid(),
        folder.id,
        ObjectRole::Viewer,
        true,
    )
    .await;
    grant_folder(
        &test,
        admin,
        SubjectType::User,
        mario.as_uuid(),
        folder.id,
        ObjectRole::Editor,
        true,
    )
    .await;

    let role = PermissionRepo::new(test.db())
        .effective_role(&AuthContext::user(mario, SystemRole::User), folder.id)
        .await
        .unwrap();
    assert_eq!(role, Some(ObjectRole::Editor));
}

#[tokio::test]
async fn a_shared_folder_never_exposes_the_real_disk_path() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let mario = harness::seed_user(&test, admin, "mario").await;
    let library = seed_library(&test, admin, "Foto", "/mnt/nas/foto").await;
    let folders = FolderRepo::new(test.db());
    let vacanze = folders.ensure_path(library, &["Vacanze"]).await.unwrap();
    folders
        .ensure_path(library, &["Vacanze", "2024", "Grecia"])
        .await
        .unwrap();
    grant_folder(
        &test,
        admin,
        SubjectType::User,
        mario.as_uuid(),
        vacanze.id,
        ObjectRole::Viewer,
        true,
    )
    .await;

    let ctx = AuthContext::user(mario, SystemRole::User);
    let tree = folders.tree(&ctx).await.unwrap();
    assert!(!tree.is_empty());
    for folder in &tree {
        let blob = format!("{} {} {}", folder.name, folder.path.as_str(), folder.id);
        assert!(
            !blob.contains("/mnt/nas"),
            "il destinatario vede i nomi, mai il path sul disco: {blob}"
        );
        assert!(
            folder
                .path
                .as_str()
                .bytes()
                .all(|b| b.is_ascii_digit() || b == b'.'),
            "ltree numerico, non un percorso filesystem: {}",
            folder.path.as_str()
        );
    }
}

#[tokio::test]
async fn visibility_is_identical_on_every_channel() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let mario = harness::seed_user(&test, admin, "mario").await;
    let library = seed_library(&test, admin, "Foto", "/mnt/nas/foto").await;
    let folders = FolderRepo::new(test.db());
    let shared = folders.ensure_path(library, &["Condivisa"]).await.unwrap();
    let other = folders.ensure_path(library, &["Altro"]).await.unwrap();
    let a = index_photo(&test, shared.id, "si.jpg").await;
    let b = index_photo(&test, other.id, "no.jpg").await;
    grant_folder(
        &test,
        admin,
        SubjectType::User,
        mario.as_uuid(),
        shared.id,
        ObjectRole::Viewer,
        true,
    )
    .await;

    let ctx = AuthContext::user(mario, SystemRole::User);
    let counted = visible_indexed(&test, &ctx).await;
    let searched = SearchRepo::new(test.db())
        .run(&ctx, &SearchNode::And { args: vec![] }, None, 200)
        .await
        .unwrap();
    let page = TimelineRepo::new(test.db())
        .page(
            &ctx,
            chrono::NaiveDate::from_ymd_opt(2024, 7, 1).unwrap(),
            None,
            200,
        )
        .await
        .unwrap();
    let assets = AssetRepo::new(test.db());

    assert_eq!(counted, 1);
    assert_eq!(searched.len(), 1);
    assert_eq!(page.len(), 1);
    assert_eq!(searched[0].id, a);
    assert_eq!(page[0].id, a);
    assert!(assets.find_by_id(&ctx, a).await.is_ok());
    assert!(matches!(
        assets.find_by_id(&ctx, b).await,
        Err(DbError::Forbidden)
    ));
}

#[tokio::test]
async fn probing_an_asset_outside_the_scope_is_forbidden_not_not_found() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let mario = harness::seed_user(&test, admin, "mario").await;
    let library = seed_library(&test, admin, "Foto", "/mnt/nas/foto").await;
    let folder = FolderRepo::new(test.db())
        .ensure_path(library, &["2024"])
        .await
        .unwrap();
    let hidden = index_photo(&test, folder.id, "segreto.jpg").await;
    let ctx = AuthContext::user(mario, SystemRole::User);
    let assets = AssetRepo::new(test.db());

    assert!(matches!(
        assets.find_by_id(&ctx, hidden).await,
        Err(DbError::Forbidden)
    ));
    assert!(matches!(
        assets.find_by_id(&ctx, AssetId::new()).await,
        Err(DbError::Forbidden)
    ));
}

#[tokio::test]
async fn a_share_does_not_open_another_library_with_the_same_ltree_label() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let mario = harness::seed_user(&test, admin, "mario").await;
    let lib_a = seed_library(&test, admin, "A", "/mnt/nas/a").await;
    let lib_b = seed_library(&test, admin, "B", "/mnt/nas/b").await;
    let folders = FolderRepo::new(test.db());
    let a_2024 = folders.ensure_path(lib_a, &["2024"]).await.unwrap();
    let b_2024 = folders.ensure_path(lib_b, &["2024"]).await.unwrap();
    assert_eq!(
        a_2024.path, b_2024.path,
        "le etichette ltree ripartono da 1 in ogni libreria"
    );
    let visible = index_photo(&test, a_2024.id, "si.jpg").await;
    let hidden = index_photo(&test, b_2024.id, "no.jpg").await;
    grant_folder(
        &test,
        admin,
        SubjectType::User,
        mario.as_uuid(),
        a_2024.id,
        ObjectRole::Viewer,
        true,
    )
    .await;

    let ctx = AuthContext::user(mario, SystemRole::User);
    let assets = AssetRepo::new(test.db());
    assert!(assets.find_by_id(&ctx, visible).await.is_ok());
    assert!(matches!(
        assets.find_by_id(&ctx, hidden).await,
        Err(DbError::Forbidden)
    ));
}
