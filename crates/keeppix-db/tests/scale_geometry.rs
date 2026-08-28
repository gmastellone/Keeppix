#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Scale test for `/timeline/geometry`: 200,000 `assets` rows, all indexed
//! with known `width`/`height`, in a single library. Verifies two things:
//!
//! 1. the `TimelineRepo::geometry` query stays under an explicit budget
//!    even on the whole view (no pagination, unlike `page`/`buckets`);
//! 2. the plan uses `assets_geometry_idx` in an **index-only scan** — the
//!    covering index from migration 0034 (extended in 0035 with
//!    `stack_id`/`kind` for the stack-primary filter), not a seq scan like
//!    an earlier stand-in measurement showed.

mod harness;

use std::time::{Duration, Instant};

use harness::TestDb;
use keeppix_db::{FolderRepo, LibraryRepo, TimelineRepo, VisibilityScope};
use keeppix_domain::{AuthContext, NewLibrary, SystemRole};

const N: i32 = 200_000;

/// Deliberately higher than the `page`/`buckets` budget (300ms): there's no
/// `LIMIT` here, so the dominant cost isn't the plan (measured via `EXPLAIN
/// ANALYZE`: ~110ms server-side, `Index Only Scan` on `assets_geometry_idx`,
/// `Heap Fetches: 0`) but the client-side transfer and decoding of 200,000
/// rows in one shot — which is exactly the cost this endpoint replaces
/// 1,070 paginated requests with, not one it adds. Measured ~600-650ms
/// end-to-end on the development container; on GitHub Actions (a shared
/// runner, after the `LEFT JOIN stacks` for the stack-primary filter) it
/// ran ~990ms. 1500ms leaves margin for CI noise without getting close to
/// the ~2s of the degraded seq scan that would happen without the covering
/// index.
const GEOMETRY_BUDGET: Duration = Duration::from_millis(1500);

async fn seed_two_hundred_thousand_sized(
    test: &TestDb,
) -> (AuthContext, keeppix_domain::LibraryId) {
    let admin = harness::seed_admin(test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let library = LibraryRepo::new(test.db())
        .create(
            &ctx,
            NewLibrary {
                name: "Scala geometria".to_owned(),
                owner_id: admin,
                root_path: std::path::PathBuf::from("/mnt/scala-geo"),
                exclude_patterns: vec![],
            },
        )
        .await
        .unwrap();

    let mut folder_ids = Vec::new();
    for year in 2016..=2025 {
        let folder = FolderRepo::new(test.db())
            .ensure_path(library.id, &[&year.to_string()])
            .await
            .unwrap();
        folder_ids.push(folder.id.as_uuid());
    }

    sqlx::query("ALTER TABLE assets DISABLE TRIGGER assets_month_counts")
        .execute(test.db().pool())
        .await
        .unwrap();

    let seeded = Instant::now();
    sqlx::query(
        "INSERT INTO assets (id, folder_id, filename, size_bytes, mtime, kind, status, \
                            taken_at_utc, width, height) \
         SELECT gen_random_uuid(), \
                ($1::uuid[])[1 + ((g - 1) % 10)], \
                'IMG_' || lpad(g::text, 6, '0') || '.jpg', \
                200000, \
                timestamptz '2010-01-01' + make_interval(hours => g), \
                'image', 'indexed', \
                timestamptz '2010-01-01' + make_interval(hours => g), \
                4000, 3000 \
           FROM generate_series(1, $2) AS g",
    )
    .bind(&folder_ids)
    .bind(N)
    .execute(test.db().pool())
    .await
    .unwrap();

    sqlx::query("ALTER TABLE assets ENABLE TRIGGER assets_month_counts")
        .execute(test.db().pool())
        .await
        .unwrap();

    // `VACUUM`, not just `ANALYZE`: the index-only scan needs a fresh
    // visibility map, which `ANALYZE` alone doesn't update. Without this
    // the plan can still do a heap fetch for every row even with the
    // covering index in place.
    sqlx::query("VACUUM ANALYZE assets")
        .execute(test.db().pool())
        .await
        .unwrap();
    eprintln!(
        "MEASUREMENT seed geometry {N} assets: {:?}",
        seeded.elapsed()
    );
    (ctx, library.id)
}

#[tokio::test]
async fn geometry_of_two_hundred_thousand_assets_stays_within_budget_and_index_only() {
    let test = TestDb::start().await;
    let (ctx, library_id) = seed_two_hundred_thousand_sized(&test).await;
    let repo = TimelineRepo::new(test.db());

    // Check the plan before the budget: a regression to a seq scan must be
    // flagged as such, not masked by a transfer timeout on noisy runners.
    let plan = explain_geometry(&test, &ctx, library_id).await;
    eprintln!("EXPLAIN geometry:\n{plan}");
    assert!(
        plan.contains("Index Only Scan") && plan.contains("assets_geometry_idx"),
        "the /timeline/geometry query must be served from assets_geometry_idx alone, \
         not degrade to a seq scan or a heap fetch per row:\n{plan}"
    );

    // A cold run warms the pool/plan; the measurement that counts is the next one.
    let _warmup = repo.geometry(&ctx, Some(library_id), None).await.unwrap();

    let t0 = Instant::now();
    let geometry = repo.geometry(&ctx, Some(library_id), None).await.unwrap();
    let elapsed = t0.elapsed();
    eprintln!(
        "MEASUREMENT geometry (whole view) {N}: {elapsed:?} ({} records)",
        geometry.records.len()
    );
    assert_eq!(
        geometry.records.len(),
        usize::try_from(N).unwrap(),
        "every indexed asset with a taken_at must appear in the geometry"
    );
    assert!(
        elapsed < GEOMETRY_BUDGET,
        "geometry of {N} shots: {elapsed:?} >= {GEOMETRY_BUDGET:?}"
    );
}

/// Same SQL query as `TimelineRepo::geometry`, with the same library filter
/// as the test above: duplicated here on purpose, like `explain_page_shared`
/// in `scale_200k.rs`, because the real query is private to the repository
/// and `EXPLAIN` must see exactly what runs in production.
async fn explain_geometry(
    test: &TestDb,
    ctx: &AuthContext,
    library_id: keeppix_domain::LibraryId,
) -> String {
    let scope = VisibilityScope::resolve(test.db(), ctx).await.unwrap();
    let filter = scope.filter("f.path", "f.library_id", "a.id", 1);
    let sql = format!(
        "EXPLAIN (ANALYZE, BUFFERS) \
         SELECT a.width, a.height, a.taken_at_utc FROM assets a \
         JOIN folders f ON f.id = a.folder_id \
         LEFT JOIN stacks s ON s.id = a.stack_id \
         WHERE {} \
           AND a.status = 'indexed' \
           AND a.kind <> 'unknown' \
           AND a.taken_at_utc IS NOT NULL \
           AND ($4::uuid IS NULL OR f.library_id = $4) \
           AND (a.stack_id IS NULL OR a.id = s.primary_asset_id) \
         ORDER BY a.taken_at_utc DESC, a.id DESC",
        filter.sql()
    );
    // Same binds as the test above: `Some(library_id)`, not `None` — with
    // `None` the planner picks `assets_timeline_idx` instead of the new
    // covering index (no `folder_id` to restrict on), a different plan
    // from what actually runs behind `?library=...`.
    let rows: Vec<(String,)> = sqlx::query_as(&sql)
        .bind(filter.bind())
        .bind(filter.holes())
        .bind(filter.assets())
        .bind(Some(library_id.as_uuid()))
        .fetch_all(test.db().pool())
        .await
        .unwrap();
    rows.into_iter()
        .map(|(line,)| line)
        .collect::<Vec<_>>()
        .join("\n")
}
