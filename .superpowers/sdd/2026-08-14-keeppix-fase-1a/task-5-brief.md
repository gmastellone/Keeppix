## Task 5: `FolderRepo` e l'albero `ltree`

Il task più delicato della fase: qui si decide come si naviga e si sposta un
albero di decine di migliaia di nodi.

**Files:**
- Create: `crates/keeppix-db/src/folders.rs`, `crates/keeppix-db/tests/folders.rs`
- Modify: `crates/keeppix-db/src/lib.rs`

**Interfaces:**
- Consumes: `LibraryRepo` (per i test), `Folder`, `FolderPath`, `FolderId`, `LibraryId`, `AuthContext`.
- Produces `FolderRepo` con:
  - `new(db: &Db)`
  - `ensure_root(&self, library_id: LibraryId) -> Result<Folder, DbError>` — idempotente.
  - `ensure_child(&self, parent: &Folder, name: &str) -> Result<Folder, DbError>` — idempotente; crea o restituisce l'esistente.
  - `ensure_path(&self, library_id: LibraryId, relative: &[&str]) -> Result<Folder, DbError>` — crea l'intera catena in una transazione.
  - `children(&self, ctx: &AuthContext, folder_id: FolderId) -> Result<Vec<Folder>, DbError>`
  - `subtree(&self, ctx: &AuthContext, folder_id: FolderId) -> Result<Vec<Folder>, DbError>` — usa `path <@`.
  - `find_by_id(&self, ctx: &AuthContext, id: FolderId) -> Result<Folder, DbError>`
  - `move_subtree(&self, ctx: &AuthContext, folder_id: FolderId, new_parent: FolderId) -> Result<(), DbError>` — riscrive i percorsi dell'intero sottoalbero con **una** query.
  - `absolute_path(&self, ctx: &AuthContext, folder_id: FolderId) -> Result<PathBuf, DbError>` — ricostruisce il percorso su disco risalendo l'albero.

Le funzioni `ensure_*` non prendono `AuthContext` perché le chiama lo
scanner. Documentarlo come per `mark_scanned`.

- [ ] **Step 1: Scrivere i test che falliscono**

`crates/keeppix-db/tests/folders.rs`:

```rust
mod harness;

use harness::TestDb;
use keeppix_db::{DbError, FolderRepo, LibraryRepo};
use keeppix_domain::{AuthContext, LibraryId, NewLibrary, SystemRole, UserId};

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

    let root = FolderRepo::new(test.db()).ensure_root(library).await.unwrap();

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
    let again = repo.ensure_path(library, &["2024", "Grecia", "Santorini"]).await.unwrap();
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

    repo.ensure_path(library, &["2024", "Grecia", "Santorini"]).await.unwrap();
    repo.ensure_path(library, &["2024", "Italia"]).await.unwrap();
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

    repo.ensure_path(library, &["2024", "Grecia", "Santorini"]).await.unwrap();
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
    repo.ensure_path(library, &["2024", "Grecia", "Santorini"]).await.unwrap();
    let archive = repo.ensure_path(library, &["Archivio"]).await.unwrap();

    let root = repo.ensure_root(library).await.unwrap();
    let y2024 = repo.ensure_child(&root, "2024").await.unwrap();
    let greece = repo.ensure_child(&y2024, "Grecia").await.unwrap();

    repo.move_subtree(&ctx, greece.id, archive.id).await.unwrap();

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

    let leaf = repo.ensure_path(library, &["2024", "Grecia"]).await.unwrap();
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

    let leaf = repo.ensure_path(library, &["2024", "Grecia", "Santorini"]).await.unwrap();

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
    assert!(matches!(repo.find_by_id(&mario_ctx, folder.id).await, Err(DbError::Forbidden)));
    assert!(matches!(repo.children(&mario_ctx, folder.id).await, Err(DbError::Forbidden)));
    assert!(matches!(repo.subtree(&mario_ctx, folder.id).await, Err(DbError::Forbidden)));
}
```

- [ ] **Step 2: Eseguire e verificare il fallimento**

Run: `cargo test -p keeppix-db --test folders -- --test-threads=1`
Expected: FAIL — `unresolved import keeppix_db::FolderRepo`.

- [ ] **Step 3: Implementare `folders.rs`**

Punti su cui il codice deve essere esatto, perché sono quelli che i test
inchiodano:

- Le etichette `ltree` vengono da una **sequenza per libreria**, non da un
  contatore globale: due librerie possono avere entrambe `1.2.3`.
- `ensure_child` deve essere idempotente **sotto concorrenza**: si usa
  `INSERT ... ON CONFLICT (parent_id, name) DO NOTHING` seguito da una
  rilettura, non un `SELECT` seguito da `INSERT`.
- `move_subtree` riscrive tutti i discendenti con **una** query, usando
  `ltree`:

```sql
UPDATE folders
   SET path  = $new_prefix::ltree || subpath(path, nlevel($old_prefix::ltree)),
       depth = nlevel($new_prefix::ltree) + nlevel(path) - nlevel($old_prefix::ltree)
 WHERE library_id = $library AND path <@ $old_prefix::ltree;
```

  Spostare una cartella con 40.000 foto tocca le righe di `folders`, non
  quelle di `assets`: è il motivo per cui nessun asset porta un percorso
  assoluto denormalizzato.

- Il ciclo va rifiutato **prima** dell'UPDATE: se `new_parent.path` discende
  da `folder.path`, restituire `DbError::Conflict`. Senza questo controllo il
  sottoalbero si scollega e non è più raggiungibile da nessuna radice.
- `absolute_path` risale l'albero con una CTE ricorsiva e concatena i `name`
  sotto `libraries.root_path`. I nomi vengono dal database, non dal client.
- I metodi con `AuthContext` risolvono la visibilità dalla libreria
  proprietaria: `Forbidden` prima di `NotFound`, come ovunque.

Aggiungere alla migrazione una sequenza per le etichette:

```sql
-- In 0004: il numero progressivo delle etichette ltree.
ALTER TABLE libraries ADD COLUMN next_folder_seq bigint NOT NULL DEFAULT 1;
```

e incrementarla con `UPDATE libraries SET next_folder_seq = next_folder_seq + 1
RETURNING next_folder_seq - 1` dentro la stessa transazione dell'inserimento.

- [ ] **Step 4: Eseguire i test**

Run: `cargo test -p keeppix-db --test folders -- --test-threads=1`
Expected: PASS — 11 test.

- [ ] **Step 5: Verificare il workspace**

Run: `cargo test --workspace -- --test-threads=1 && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --check`

- [ ] **Step 6: Commit**

```bash
git add crates/keeppix-db
git commit -m "feat(db): add folder repository with ltree subtree operations"
```

---

