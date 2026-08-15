mod harness;

use harness::TestDb;

/// Libreria + radice, il minimo per inserire un asset.
#[allow(clippy::unwrap_used)]
async fn seed_folder(test: &TestDb) -> uuid::Uuid {
    let owner = harness::seed_admin(test).await;
    let library = uuid::Uuid::now_v7();
    sqlx::query("INSERT INTO libraries (id, name, owner_id, root_path) VALUES ($1,'F',$2,'/m')")
        .bind(library)
        .bind(owner.as_uuid())
        .execute(test.db().pool())
        .await
        .unwrap();

    let folder = uuid::Uuid::now_v7();
    sqlx::query(
        "INSERT INTO folders (id, library_id, parent_id, name, path, depth) \
         VALUES ($1, $2, NULL, '', '1'::ltree, 1)",
    )
    .bind(folder)
    .bind(library)
    .execute(test.db().pool())
    .await
    .unwrap();
    folder
}

#[allow(clippy::unwrap_used)]
async fn insert_asset(
    test: &TestDb,
    folder_id: uuid::Uuid,
    filename: &str,
    hash: Option<&[u8]>,
) -> uuid::Uuid {
    let id = uuid::Uuid::now_v7();
    sqlx::query(
        "INSERT INTO assets (id, folder_id, filename, content_hash, size_bytes, mtime) \
         VALUES ($1, $2, $3, $4, 100, now())",
    )
    .bind(id)
    .bind(folder_id)
    .bind(filename)
    .bind(hash)
    .execute(test.db().pool())
    .await
    .unwrap();
    id
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn folder_and_filename_are_the_identity() {
    let test = TestDb::start().await;
    let folder = seed_folder(&test).await;
    insert_asset(&test, folder, "DSC_0042.ARW", None).await;

    let duplicate = sqlx::query(
        "INSERT INTO assets (id, folder_id, filename, size_bytes, mtime) \
         VALUES ($1, $2, 'DSC_0042.ARW', 100, now())",
    )
    .bind(uuid::Uuid::now_v7())
    .bind(folder)
    .execute(test.db().pool())
    .await;

    assert!(
        duplicate.is_err(),
        "due file con lo stesso nome nella stessa cartella sono lo stesso asset"
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn two_assets_may_share_a_content_hash() {
    let test = TestDb::start().await;
    let folder = seed_folder(&test).await;
    let hash = [0xab_u8; 32];

    insert_asset(&test, folder, "a.jpg", Some(&hash)).await;
    insert_asset(&test, folder, "b.jpg", Some(&hash)).await;

    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM assets WHERE content_hash = $1")
        .bind(&hash[..])
        .fetch_one(test.db().pool())
        .await
        .unwrap();
    assert_eq!(
        count, 2,
        "l'hash e indicizzato, non unico: due copie sono due asset"
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn deleting_a_folder_removes_its_assets() {
    let test = TestDb::start().await;
    let folder = seed_folder(&test).await;
    insert_asset(&test, folder, "DSC_0042.ARW", None).await;

    sqlx::query("DELETE FROM folders WHERE id = $1")
        .bind(folder)
        .execute(test.db().pool())
        .await
        .unwrap();

    let remaining: i64 = sqlx::query_scalar("SELECT count(*) FROM assets")
        .fetch_one(test.db().pool())
        .await
        .unwrap();
    assert_eq!(remaining, 0, "gli asset seguono la cartella");
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn deleting_an_asset_removes_its_exif() {
    let test = TestDb::start().await;
    let folder = seed_folder(&test).await;
    let asset = insert_asset(&test, folder, "DSC_0042.ARW", None).await;

    sqlx::query(
        "INSERT INTO asset_exif (asset_id, raw, camera_make, camera_model) \
         VALUES ($1, '{}'::jsonb, 'Sony', 'ILCE-7M4')",
    )
    .bind(asset)
    .execute(test.db().pool())
    .await
    .unwrap();

    sqlx::query("DELETE FROM assets WHERE id = $1")
        .bind(asset)
        .execute(test.db().pool())
        .await
        .unwrap();

    let remaining: i64 = sqlx::query_scalar("SELECT count(*) FROM asset_exif")
        .fetch_one(test.db().pool())
        .await
        .unwrap();
    assert_eq!(remaining, 0, "gli EXIF seguono l'asset, non sopravvivono");
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn an_unknown_kind_is_rejected() {
    let test = TestDb::start().await;
    let folder = seed_folder(&test).await;

    // Senza questo inserimento il test passerebbe anche se `assets` non
    // esistesse: qualsiasi errore soddisfa `is_err()`.
    insert_asset(&test, folder, "ok.jpg", None).await;

    let bad = sqlx::query(
        "INSERT INTO assets (id, folder_id, filename, size_bytes, mtime, kind) \
         VALUES ($1, $2, 'x.pdf', 100, now(), 'pdf')",
    )
    .bind(uuid::Uuid::now_v7())
    .bind(folder)
    .execute(test.db().pool())
    .await;

    let err = bad.expect_err("kind accetta solo image, raw_image, video, unknown");
    let db_err = err.as_database_error().expect("errore postgres");
    assert_eq!(
        db_err.code().as_deref(),
        Some("23514"),
        "deve essere il CHECK, non un'altra violazione"
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn the_timeline_index_exists() {
    let test = TestDb::start().await;

    let present: bool = sqlx::query_scalar(
        "SELECT EXISTS ( \
             SELECT 1 FROM pg_indexes \
              WHERE tablename = 'assets' AND indexname = 'assets_timeline_idx' \
         )",
    )
    .fetch_one(test.db().pool())
    .await
    .unwrap();

    assert!(
        present,
        "la timeline della Fase 1c pagina su (taken_at_utc DESC, id DESC)"
    );
}
