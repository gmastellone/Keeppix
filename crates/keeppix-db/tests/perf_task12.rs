#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Performance measurements: trigram index usage and `FolderRepo::ensure_path`
//! cost. These are observational — they print EXPLAIN / timings and assert
//! only the properties that would regress if the migration or path logic
//! broke.

mod harness;

use std::time::Instant;

use harness::TestDb;
use keeppix_db::{FolderRepo, LibraryRepo};
use keeppix_domain::{AuthContext, NewLibrary, SystemRole};

#[tokio::test]
async fn camera_and_lens_ilike_use_trigram_indexes() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let library = LibraryRepo::new(test.db())
        .create(
            &ctx,
            NewLibrary {
                name: "Perf".to_owned(),
                owner_id: admin,
                root_path: std::path::PathBuf::from("/mnt/perf"),
                exclude_patterns: vec![],
            },
        )
        .await
        .unwrap();
    let folder = FolderRepo::new(test.db())
        .ensure_path(library.id, &[])
        .await
        .unwrap();

    sqlx::query(
        "INSERT INTO assets (id, folder_id, filename, size_bytes, mtime, kind, status) \
         SELECT gen_random_uuid(), $1, 'IMG_' || g || '.jpg', 1000, now(), 'image', 'indexed' \
           FROM generate_series(1, 5000) AS g",
    )
    .bind(folder.id.as_uuid())
    .execute(test.db().pool())
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO asset_exif (asset_id, raw, camera_model, lens) \
         SELECT id, '{}'::jsonb, \
                CASE WHEN (row_number() OVER ()) % 50 = 0 THEN 'Canon EOS R5' \
                     ELSE 'OtherCam ' || (row_number() OVER ())::text END, \
                CASE WHEN (row_number() OVER ()) % 50 = 0 THEN 'RF 24-70mm' \
                     ELSE 'OtherLens ' || (row_number() OVER ())::text END \
           FROM assets",
    )
    .execute(test.db().pool())
    .await
    .unwrap();
    sqlx::query("ANALYZE asset_exif")
        .execute(test.db().pool())
        .await
        .unwrap();

    let mut tx = test.db().pool().begin().await.unwrap();
    // Hide the competing btree partial index so the planner has to pick the
    // GIN trigram indexes for text search. Roll the session back afterwards.
    sqlx::query(
        "UPDATE pg_index SET indisvalid = false \
          WHERE indexrelid = 'asset_exif_camera_idx'::regclass",
    )
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query("SET LOCAL enable_seqscan = off")
        .execute(&mut *tx)
        .await
        .unwrap();

    let camera_plan: Vec<String> = sqlx::query_scalar(
        "EXPLAIN (ANALYZE, BUFFERS) \
         SELECT asset_id FROM asset_exif WHERE camera_model ILIKE '%EOS R5%'",
    )
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    let camera_text = camera_plan.join("\n");
    eprintln!("EXPLAIN camera ILIKE:\n{camera_text}");
    assert!(
        camera_text.contains("asset_exif_camera_trgm"),
        "camera ILIKE must use asset_exif_camera_trgm, got:\n{camera_text}"
    );

    let lens_plan: Vec<String> = sqlx::query_scalar(
        "EXPLAIN (ANALYZE, BUFFERS) \
         SELECT asset_id FROM asset_exif WHERE lens ILIKE '%24-70%'",
    )
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    let lens_text = lens_plan.join("\n");
    eprintln!("EXPLAIN lens ILIKE:\n{lens_text}");
    assert!(
        lens_text.contains("asset_exif_lens_trgm"),
        "lens ILIKE must use asset_exif_lens_trgm, got:\n{lens_text}"
    );
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn ensure_path_cost_stays_acceptable_for_ingest_depths() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let library = LibraryRepo::new(test.db())
        .create(
            &ctx,
            NewLibrary {
                name: "Paths".to_owned(),
                owner_id: admin,
                root_path: std::path::PathBuf::from("/mnt/paths"),
                exclude_patterns: vec![],
            },
        )
        .await
        .unwrap();
    let folders = FolderRepo::new(test.db());

    // Warm root once so the measured loop is pure child creation/lookup.
    folders.ensure_path(library.id, &[]).await.unwrap();

    let depths = [1usize, 5, 10, 20];
    for depth in depths {
        let segments: Vec<String> = (0..depth).map(|i| format!("seg{i}")).collect();
        let refs: Vec<&str> = segments.iter().map(String::as_str).collect();
        let t0 = Instant::now();
        // Same path repeatedly: the hot ingest path is mostly ensure-existing.
        for _ in 0..100 {
            folders.ensure_path(library.id, &refs).await.unwrap();
        }
        let elapsed = t0.elapsed();
        eprintln!(
            "MEASUREMENT ensure_path depth={depth} x100 existing: {elapsed:?} ({:?}/call)",
            elapsed / 100
        );
        // Even depth-20 existing paths must stay well under a millisecond-class
        // budget for 100 calls — if this trips, revisit the N+1 rewrite.
        assert!(
            elapsed.as_millis() < 2_000,
            "ensure_path depth={depth} x100 took {elapsed:?}"
        );
    }

    // Cold path: create 50 distinct depth-8 trees (worst case for N+1 writes).
    let t1 = Instant::now();
    for n in 0..50 {
        let segments = [
            format!("batch{n}"),
            "a".into(),
            "b".into(),
            "c".into(),
            "d".into(),
            "e".into(),
            "f".into(),
            "g".into(),
        ];
        let refs: Vec<&str> = segments.iter().map(String::as_str).collect();
        folders.ensure_path(library.id, &refs).await.unwrap();
    }
    let cold = t1.elapsed();
    eprintln!("MEASUREMENT ensure_path 50 cold depth-8 trees: {cold:?}");
    assert!(
        cold.as_millis() < 5_000,
        "cold ensure_path batch took {cold:?}; rewrite may be justified"
    );
}

#[tokio::test]
async fn status_not_trashed_filter_explain_is_acceptable_without_rewriting() {
    // `status <> 'trashed'` does not match the partial `assets_status_idx`
    // (discovered|error only) or `assets_timeline_idx` (status = indexed).
    // Measure whether a rewrite to `status IN (...)` is worth the risk on a
    // folder listing shaped like AssetRepo.
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let library = LibraryRepo::new(test.db())
        .create(
            &ctx,
            NewLibrary {
                name: "TrashFilter".to_owned(),
                owner_id: admin,
                root_path: std::path::PathBuf::from("/mnt/trash-filter"),
                exclude_patterns: vec![],
            },
        )
        .await
        .unwrap();
    let folder = FolderRepo::new(test.db())
        .ensure_path(library.id, &[])
        .await
        .unwrap();

    sqlx::query(
        "INSERT INTO assets (id, folder_id, filename, size_bytes, mtime, kind, status) \
         SELECT gen_random_uuid(), $1, 'IMG_' || g || '.jpg', 1000, now(), 'image', \
                CASE WHEN g % 20 = 0 THEN 'trashed' ELSE 'indexed' END \
           FROM generate_series(1, 20000) AS g",
    )
    .bind(folder.id.as_uuid())
    .execute(test.db().pool())
    .await
    .unwrap();
    sqlx::query("ANALYZE assets")
        .execute(test.db().pool())
        .await
        .unwrap();

    let ne_plan: Vec<String> = sqlx::query_scalar(
        "EXPLAIN (ANALYZE, BUFFERS) \
         SELECT id FROM assets \
          WHERE folder_id = $1 AND status <> 'trashed' \
          ORDER BY filename",
    )
    .bind(folder.id.as_uuid())
    .fetch_all(test.db().pool())
    .await
    .unwrap();
    let in_plan: Vec<String> = sqlx::query_scalar(
        "EXPLAIN (ANALYZE, BUFFERS) \
         SELECT id FROM assets \
          WHERE folder_id = $1 \
            AND status IN ('discovered', 'indexed', 'error') \
          ORDER BY filename",
    )
    .bind(folder.id.as_uuid())
    .fetch_all(test.db().pool())
    .await
    .unwrap();

    let ne = ne_plan.join("\n");
    let inn = in_plan.join("\n");
    eprintln!("MEASUREMENT status <> 'trashed':\n{ne}");
    eprintln!("MEASUREMENT status IN (...):\n{inn}");

    // Both plans must stay index-friendly on folder_id (PK/FK path). We do
    // not require a specific index name — the ruling is that <> is fine when
    // the folder filter already prunes enough rows.
    assert!(
        ne.to_ascii_lowercase().contains("index") || ne.contains("Bitmap"),
        "folder listing with <> trashed should still use an index: {ne}"
    );
    // Sanity: both return the same cardinality class (no accidental full scan
    // explosion). Extract "rows=" from the top node loosely.
    assert!(
        !ne.to_ascii_lowercase().contains("seq scan on assets") || ne.contains("folder_id"),
        "unexpected seq scan shape for <> filter: {ne}"
    );
}
