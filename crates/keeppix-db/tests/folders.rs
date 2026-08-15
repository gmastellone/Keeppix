mod harness;

use harness::TestDb;
use keeppix_db::{DbError, FolderRepo, LibraryRepo};
use keeppix_domain::{AuthContext, FolderId, LibraryId, NewLibrary, SystemRole, UserId};

#[allow(clippy::expect_used)]
async fn seed_library(test: &TestDb, owner: UserId) -> LibraryId {
    let ctx = AuthContext::user(owner, SystemRole::Admin);
    LibraryRepo::new(test.db())
        .create(
            &ctx,
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

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn the_root_has_an_empty_name_and_a_single_label() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let library = seed_library(&test, admin).await;

    let root = FolderRepo::new(test.db())
        .ensure_root(library)
        .await
        .unwrap();

    assert_eq!(root.name, "");
    assert!(root.parent_id.is_none());
    assert_eq!(root.path.depth(), 1);
    assert_eq!(root.depth, 1);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn ensure_root_is_idempotent() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let library = seed_library(&test, admin).await;
    let repo = FolderRepo::new(test.db());

    let first = repo.ensure_root(library).await.unwrap();
    let second = repo.ensure_root(library).await.unwrap();

    assert_eq!(first.id, second.id, "una libreria ha una sola radice");
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn children_extend_the_parent_path() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let library = seed_library(&test, admin).await;
    let repo = FolderRepo::new(test.db());

    let root = repo.ensure_root(library).await.unwrap();
    let year = repo.ensure_child(&root, "2024").await.unwrap();
    let event = repo.ensure_child(&year, "Matrimonio Rossi").await.unwrap();

    assert!(event.path.is_descendant_of(&root.path));
    assert!(event.path.is_descendant_of(&year.path));
    assert_eq!(event.depth, 3);
    assert_eq!(event.name, "Matrimonio Rossi", "il nome resta quello vero");
    assert!(
        !event.path.as_str().contains("Matrimonio"),
        "il nome non deve MAI finire nel percorso ltree"
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn ensure_child_is_idempotent() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let library = seed_library(&test, admin).await;
    let repo = FolderRepo::new(test.db());

    let root = repo.ensure_root(library).await.unwrap();
    let a = repo.ensure_child(&root, "2024").await.unwrap();
    let b = repo.ensure_child(&root, "2024").await.unwrap();

    assert_eq!(a.id, b.id, "riscansionare non duplica le cartelle");
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn ensure_path_creates_the_whole_chain() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let library = seed_library(&test, admin).await;
    let repo = FolderRepo::new(test.db());

    let leaf = repo
        .ensure_path(library, &["2024", "Grecia", "Santorini"])
        .await
        .unwrap();

    assert_eq!(leaf.name, "Santorini");
    assert_eq!(leaf.depth, 4, "radice piu tre livelli");

    // Rieseguirla non crea nulla di nuovo.
    let again = repo
        .ensure_path(library, &["2024", "Grecia", "Santorini"])
        .await
        .unwrap();
    assert_eq!(leaf.id, again.id);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn subtree_returns_descendants_including_itself() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let library = seed_library(&test, admin).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let repo = FolderRepo::new(test.db());

    repo.ensure_path(library, &["2024", "Grecia", "Santorini"])
        .await
        .unwrap();
    repo.ensure_path(library, &["2024", "Italia"])
        .await
        .unwrap();
    repo.ensure_path(library, &["2023"]).await.unwrap();

    let root = repo.ensure_root(library).await.unwrap();
    let y2024 = repo.ensure_child(&root, "2024").await.unwrap();

    let under_2024 = repo.subtree(&ctx, y2024.id).await.unwrap();
    let names: Vec<&str> = under_2024.iter().map(|f| f.name.as_str()).collect();

    assert!(names.contains(&"2024"), "ltree <@ include il nodo stesso");
    assert!(names.contains(&"Grecia"));
    assert!(names.contains(&"Santorini"));
    assert!(names.contains(&"Italia"));
    assert!(!names.contains(&"2023"), "un fratello non e un discendente");
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn children_are_direct_only() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let library = seed_library(&test, admin).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let repo = FolderRepo::new(test.db());

    repo.ensure_path(library, &["2024", "Grecia", "Santorini"])
        .await
        .unwrap();
    let root = repo.ensure_root(library).await.unwrap();
    let y2024 = repo.ensure_child(&root, "2024").await.unwrap();

    let direct = repo.children(&ctx, y2024.id).await.unwrap();
    let names: Vec<&str> = direct.iter().map(|f| f.name.as_str()).collect();

    assert_eq!(names, vec!["Grecia"], "solo i figli diretti, non i nipoti");
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn moving_a_subtree_rewrites_every_descendant_path() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let library = seed_library(&test, admin).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let repo = FolderRepo::new(test.db());

    // /2024/Grecia/Santorini  ->  spostiamo Grecia sotto /Archivio
    repo.ensure_path(library, &["2024", "Grecia", "Santorini"])
        .await
        .unwrap();
    let archive = repo.ensure_path(library, &["Archivio"]).await.unwrap();

    let root = repo.ensure_root(library).await.unwrap();
    let y2024 = repo.ensure_child(&root, "2024").await.unwrap();
    let greece = repo.ensure_child(&y2024, "Grecia").await.unwrap();

    repo.move_subtree(&ctx, greece.id, archive.id)
        .await
        .unwrap();

    let moved = repo.find_by_id(&ctx, greece.id).await.unwrap();
    assert_eq!(moved.parent_id, Some(archive.id));
    assert!(moved.path.is_descendant_of(&archive.path));
    assert_eq!(moved.depth, 3);

    // Il nipote deve essere sceso con lui.
    let under_archive = repo.subtree(&ctx, archive.id).await.unwrap();
    let santorini = under_archive
        .iter()
        .find(|f| f.name == "Santorini")
        .expect("Santorini e sceso con Grecia");
    assert!(santorini.path.is_descendant_of(&moved.path));
    assert_eq!(santorini.depth, 4);

    // E non deve piu stare sotto 2024.
    let under_2024 = repo.subtree(&ctx, y2024.id).await.unwrap();
    assert_eq!(under_2024.len(), 1, "sotto 2024 resta solo 2024 stesso");
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn a_folder_cannot_be_moved_inside_itself() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let library = seed_library(&test, admin).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let repo = FolderRepo::new(test.db());

    let leaf = repo
        .ensure_path(library, &["2024", "Grecia"])
        .await
        .unwrap();
    let root = repo.ensure_root(library).await.unwrap();
    let y2024 = repo.ensure_child(&root, "2024").await.unwrap();

    // Spostare 2024 dentro il proprio figlio scollegherebbe il sottoalbero.
    let cycle = repo.move_subtree(&ctx, y2024.id, leaf.id).await;
    assert!(matches!(cycle, Err(DbError::Conflict(_))));
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn absolute_path_reconstructs_the_filesystem_path() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let library = seed_library(&test, admin).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let repo = FolderRepo::new(test.db());

    let leaf = repo
        .ensure_path(library, &["2024", "Grecia", "Santorini"])
        .await
        .unwrap();

    assert_eq!(
        repo.absolute_path(&ctx, leaf.id).await.unwrap(),
        std::path::PathBuf::from("/mnt/foto/2024/Grecia/Santorini")
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn a_plain_user_cannot_read_someone_elses_folders() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let mario = harness::seed_user(&test, admin, "mario").await;
    let library = seed_library(&test, admin).await;
    let repo = FolderRepo::new(test.db());

    let folder = repo.ensure_path(library, &["2024"]).await.unwrap();

    let mario_ctx = AuthContext::user(mario, SystemRole::User);
    assert!(matches!(
        repo.find_by_id(&mario_ctx, folder.id).await,
        Err(DbError::Forbidden)
    ));
    assert!(matches!(
        repo.children(&mario_ctx, folder.id).await,
        Err(DbError::Forbidden)
    ));
    assert!(matches!(
        repo.subtree(&mario_ctx, folder.id).await,
        Err(DbError::Forbidden)
    ));
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn probing_an_unknown_folder_id_is_forbidden() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let mario = harness::seed_user(&test, admin, "mario").await;
    let mario_ctx = AuthContext::user(mario, SystemRole::User);
    let repo = FolderRepo::new(test.db());

    let probe = FolderId::new();
    assert!(matches!(
        repo.find_by_id(&mario_ctx, probe).await,
        Err(DbError::Forbidden)
    ));
    assert!(matches!(
        repo.children(&mario_ctx, probe).await,
        Err(DbError::Forbidden)
    ));
    assert!(matches!(
        repo.subtree(&mario_ctx, probe).await,
        Err(DbError::Forbidden)
    ));
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn two_libraries_can_share_the_same_numeric_path() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let a = seed_library(&test, admin).await;
    let b = LibraryRepo::new(test.db())
        .create(
            &AuthContext::user(admin, SystemRole::Admin),
            NewLibrary {
                name: "Altro".to_owned(),
                owner_id: admin,
                root_path: std::path::PathBuf::from("/mnt/altro"),
                exclude_patterns: vec![],
            },
        )
        .await
        .expect("seconda libreria")
        .id;
    let repo = FolderRepo::new(test.db());

    let leaf_a = repo.ensure_path(a, &["2024", "Grecia"]).await.unwrap();
    let leaf_b = repo.ensure_path(b, &["2024", "Grecia"]).await.unwrap();

    assert_eq!(
        leaf_a.path.as_str(),
        leaf_b.path.as_str(),
        "le etichette sono per libreria, non globali"
    );
    assert_ne!(leaf_a.id, leaf_b.id);
    assert_ne!(leaf_a.library_id, leaf_b.library_id);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn concurrent_ensure_child_does_not_duplicate() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let library = seed_library(&test, admin).await;
    let repo_a = FolderRepo::new(test.db());
    let repo_b = FolderRepo::new(test.db());

    let root = repo_a.ensure_root(library).await.unwrap();
    let (a, b) = tokio::join!(
        repo_a.ensure_child(&root, "2024"),
        repo_b.ensure_child(&root, "2024"),
    );

    let a = a.unwrap();
    let b = b.unwrap();
    assert_eq!(a.id, b.id, "due scan concorrenti non devono duplicare");
}
