mod harness;

use harness::TestDb;

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn ltree_extension_is_enabled() {
    let test = TestDb::start().await;
    let enabled: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pg_extension WHERE extname = 'ltree')")
            .fetch_one(test.db().pool())
            .await
            .unwrap();
    assert!(enabled, "ltree serve all'albero delle cartelle");
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn a_library_requires_an_existing_owner() {
    let test = TestDb::start().await;
    let orphan = sqlx::query(
        "INSERT INTO libraries (id, name, owner_id, root_path) VALUES ($1, 'X', $2, '/tmp')",
    )
    .bind(uuid::Uuid::now_v7())
    .bind(uuid::Uuid::now_v7())
    .execute(test.db().pool())
    .await;
    assert!(orphan.is_err(), "owner_id deve essere una foreign key");
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn root_path_is_unique() {
    let test = TestDb::start().await;
    let owner = harness::seed_admin(&test).await;

    let insert = "INSERT INTO libraries (id, name, owner_id, root_path) VALUES ($1, $2, $3, $4)";
    sqlx::query(insert)
        .bind(uuid::Uuid::now_v7())
        .bind("Foto")
        .bind(owner.as_uuid())
        .bind("/mnt/foto")
        .execute(test.db().pool())
        .await
        .unwrap();

    let duplicate = sqlx::query(insert)
        .bind(uuid::Uuid::now_v7())
        .bind("Foto bis")
        .bind(owner.as_uuid())
        .bind("/mnt/foto")
        .execute(test.db().pool())
        .await;

    assert!(
        duplicate.is_err(),
        "due librerie non possono indicizzare lo stesso path"
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn deleting_a_library_removes_its_folders() {
    let test = TestDb::start().await;
    let owner = harness::seed_admin(&test).await;
    let library = uuid::Uuid::now_v7();

    sqlx::query("INSERT INTO libraries (id, name, owner_id, root_path) VALUES ($1,'F',$2,'/m')")
        .bind(library)
        .bind(owner.as_uuid())
        .execute(test.db().pool())
        .await
        .unwrap();

    sqlx::query(
        "INSERT INTO folders (id, library_id, parent_id, name, path, depth) \
         VALUES ($1, $2, NULL, '', '1'::ltree, 1)",
    )
    .bind(uuid::Uuid::now_v7())
    .bind(library)
    .execute(test.db().pool())
    .await
    .unwrap();

    sqlx::query("DELETE FROM libraries WHERE id = $1")
        .bind(library)
        .execute(test.db().pool())
        .await
        .unwrap();

    let remaining: i64 = sqlx::query_scalar("SELECT count(*) FROM folders")
        .fetch_one(test.db().pool())
        .await
        .unwrap();
    assert_eq!(remaining, 0, "le cartelle seguono la libreria");
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn sibling_folders_cannot_share_a_name() {
    let test = TestDb::start().await;
    let owner = harness::seed_admin(&test).await;
    let library = uuid::Uuid::now_v7();
    sqlx::query("INSERT INTO libraries (id, name, owner_id, root_path) VALUES ($1,'F',$2,'/m')")
        .bind(library)
        .bind(owner.as_uuid())
        .execute(test.db().pool())
        .await
        .unwrap();

    let root = uuid::Uuid::now_v7();
    sqlx::query(
        "INSERT INTO folders (id, library_id, parent_id, name, path, depth) \
         VALUES ($1, $2, NULL, '', '1'::ltree, 1)",
    )
    .bind(root)
    .bind(library)
    .execute(test.db().pool())
    .await
    .unwrap();

    let child = "INSERT INTO folders (id, library_id, parent_id, name, path, depth) \
                 VALUES ($1, $2, $3, '2024', '1.2'::ltree, 2)";
    sqlx::query(child)
        .bind(uuid::Uuid::now_v7())
        .bind(library)
        .bind(root)
        .execute(test.db().pool())
        .await
        .unwrap();

    let duplicate = sqlx::query(child)
        .bind(uuid::Uuid::now_v7())
        .bind(library)
        .bind(root)
        .execute(test.db().pool())
        .await;

    assert!(
        duplicate.is_err(),
        "due sorelle non possono chiamarsi uguale"
    );
}
