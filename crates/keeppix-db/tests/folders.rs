mod harness;

use harness::TestDb;
use keeppix_db::{DbError, FolderRepo, LibraryRepo};
use keeppix_domain::{
    AuthContext, CullingRole, FolderId, LibraryId, NewLibrary, SystemRole, UserId,
};

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

/// Fase 9 Task 2: `_taken`/`_skipped` sono riconosciute dalla colonna, non
/// dal nome.
mod culling_role {
    use super::*;

    #[tokio::test]
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    async fn ensure_culling_child_creates_taken_and_skipped_marked_by_role() {
        let test = TestDb::start().await;
        let admin = harness::seed_admin(&test).await;
        let library = seed_library(&test, admin).await;
        let repo = FolderRepo::new(test.db());
        let lot = repo.ensure_path(library, &["Vacanze"]).await.unwrap();

        let taken = repo
            .ensure_culling_child(&lot, CullingRole::Taken)
            .await
            .unwrap();
        let skipped = repo
            .ensure_culling_child(&lot, CullingRole::Skipped)
            .await
            .unwrap();

        assert_eq!(taken.name, "_taken");
        assert_eq!(taken.culling_role, Some(CullingRole::Taken));
        assert_eq!(skipped.name, "_skipped");
        assert_eq!(skipped.culling_role, Some(CullingRole::Skipped));
        assert_eq!(
            lot.culling_role, None,
            "la radice del lotto stesso non porta un ruolo, solo le due sottocartelle"
        );

        // Idempotente: una seconda chiamata restituisce la stessa riga, non
        // ne crea una seconda.
        let again = repo
            .ensure_culling_child(&lot, CullingRole::Taken)
            .await
            .unwrap();
        assert_eq!(again.id, taken.id);
    }

    /// Una cartella chiamata `_taken` creata a mano (o da una versione
    /// precedente della funzione) prima che Keeppix la marcasse — l'
    /// `UPDATE` di auto-guarigione dopo l'`INSERT` ignorato deve comunque
    /// impostarne il ruolo, non lasciarla `NULL` per sempre.
    #[tokio::test]
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    async fn ensure_culling_child_heals_a_role_missing_folder() {
        let test = TestDb::start().await;
        let admin = harness::seed_admin(&test).await;
        let library = seed_library(&test, admin).await;
        let repo = FolderRepo::new(test.db());
        let lot = repo.ensure_path(library, &["Vacanze"]).await.unwrap();

        // Cartella comune, mai passata da `ensure_culling_child`: stesso
        // nome di quella speciale, ma senza ruolo.
        let plain = repo.ensure_child(&lot, "_taken").await.unwrap();
        assert_eq!(plain.culling_role, None);

        let healed = repo
            .ensure_culling_child(&lot, CullingRole::Taken)
            .await
            .unwrap();

        assert_eq!(healed.id, plain.id, "stessa riga, non una seconda");
        assert_eq!(healed.culling_role, Some(CullingRole::Taken));
    }
}

/// Fase 9 Task 2: `LibraryRepo::set_culling_root`.
mod culling_root {
    use super::*;
    use keeppix_db::{NewGrant, ObjectType, PermissionRepo, SubjectType};
    use keeppix_domain::ObjectRole;

    #[tokio::test]
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    async fn owner_can_designate_and_then_clear_the_root() {
        let test = TestDb::start().await;
        let admin = harness::seed_admin(&test).await;
        let library = seed_library(&test, admin).await;
        let ctx = AuthContext::user(admin, SystemRole::User);
        let folders = FolderRepo::new(test.db());
        let culling = folders.ensure_path(library, &["Culling"]).await.unwrap();

        let updated = LibraryRepo::new(test.db())
            .set_culling_root(&ctx, library, Some(culling.id))
            .await
            .unwrap();
        assert_eq!(updated.culling_root_folder_id, Some(culling.id));

        let cleared = LibraryRepo::new(test.db())
            .set_culling_root(&ctx, library, None)
            .await
            .unwrap();
        assert_eq!(cleared.culling_root_folder_id, None);
    }

    #[tokio::test]
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    async fn an_editor_who_is_not_the_owner_cannot_designate_the_root() {
        let test = TestDb::start().await;
        let admin = harness::seed_admin(&test).await;
        let library = seed_library(&test, admin).await;
        let folders = FolderRepo::new(test.db());
        let root = folders.ensure_root(library).await.unwrap();
        let culling = folders.ensure_path(library, &["Culling"]).await.unwrap();

        let editor = harness::seed_user(&test, admin, "editor").await;
        PermissionRepo::new(test.db())
            .grant(
                &AuthContext::user(admin, SystemRole::Admin),
                NewGrant {
                    subject: SubjectType::User,
                    subject_id: editor.as_uuid(),
                    object: ObjectType::Folder,
                    object_id: root.id.as_uuid(),
                    role: ObjectRole::Editor,
                    inherit: true,
                },
            )
            .await
            .unwrap();

        let editor_ctx = AuthContext::user(editor, SystemRole::User);
        let result = LibraryRepo::new(test.db())
            .set_culling_root(&editor_ctx, library, Some(culling.id))
            .await;

        assert!(
            matches!(result, Err(DbError::Forbidden)),
            "solo owner/admin, un editor non basta: {result:?}"
        );
    }

    #[tokio::test]
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    async fn rejects_a_folder_belonging_to_another_library() {
        let test = TestDb::start().await;
        let admin = harness::seed_admin(&test).await;
        let library_a = seed_library(&test, admin).await;
        let library_b = LibraryRepo::new(test.db())
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
        let ctx = AuthContext::user(admin, SystemRole::User);
        let folder_in_b = FolderRepo::new(test.db())
            .ensure_path(library_b, &["Culling"])
            .await
            .unwrap();

        let result = LibraryRepo::new(test.db())
            .set_culling_root(&ctx, library_a, Some(folder_in_b.id))
            .await;

        assert!(
            matches!(result, Err(DbError::Conflict(_))),
            "una cartella di un'altra libreria non può diventare radice: {result:?}"
        );
    }
}
