#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Prova di scala sintetica: 200.000 righe `assets`, niente file.
//! Misura le query della timeline e della ricerca, non l'I/O di ingestione.

mod harness;

use std::time::{Duration, Instant};

use chrono::NaiveDate;
use harness::TestDb;
use keeppix_db::{
    FolderRepo, LibraryRepo, ObjectType, PermissionRepo, SearchNode, SearchRepo, SubjectType,
    TimelineRepo, VisibilityScope,
};
use keeppix_domain::{AuthContext, NewLibrary, ObjectRole, SystemRole};

const N: i32 = 200_000;
const TIMELINE_BUDGET: Duration = Duration::from_millis(300);
const SEARCH_BUDGET: Duration = Duration::from_millis(500);

async fn seed_two_hundred_thousand(test: &TestDb) -> (AuthContext, keeppix_domain::LibraryId) {
    let admin = harness::seed_admin(test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let library = LibraryRepo::new(test.db())
        .create(
            &ctx,
            NewLibrary {
                name: "Scala".to_owned(),
                owner_id: admin,
                root_path: std::path::PathBuf::from("/mnt/scala"),
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

    sqlx::query(
        "INSERT INTO folder_month_counts (folder_id, month, asset_count) \
         SELECT folder_id, date_trunc('month', taken_at_utc)::date, count(*) \
           FROM assets \
          WHERE status = 'indexed' AND taken_at_utc IS NOT NULL \
          GROUP BY 1, 2",
    )
    .execute(test.db().pool())
    .await
    .unwrap();

    sqlx::query("ALTER TABLE assets ENABLE TRIGGER assets_month_counts")
        .execute(test.db().pool())
        .await
        .unwrap();
    sqlx::query("ANALYZE assets")
        .execute(test.db().pool())
        .await
        .unwrap();
    sqlx::query("ANALYZE folder_month_counts")
        .execute(test.db().pool())
        .await
        .unwrap();
    eprintln!("MEASUREMENT seed {N} assets: {:?}", seeded.elapsed());
    (ctx, library.id)
}

#[tokio::test]
async fn two_hundred_thousand_assets_keep_timeline_and_search_within_budget() {
    let test = TestDb::start().await;
    let (ctx, library_id) = seed_two_hundred_thousand(&test).await;
    let repo = TimelineRepo::new(test.db());

    let t0 = Instant::now();
    let buckets = repo.buckets(&ctx, Some(library_id)).await.unwrap();
    let buckets_elapsed = t0.elapsed();
    eprintln!(
        "MEASUREMENT buckets {N}: {buckets_elapsed:?} ({} mesi)",
        buckets.len()
    );
    assert!(
        buckets_elapsed < TIMELINE_BUDGET,
        "conteggi mese: {buckets_elapsed:?} >= {TIMELINE_BUDGET:?}"
    );

    let newest = buckets.first().expect("almeno un mese").month;
    let t1 = Instant::now();
    let first_page = repo.page(&ctx, newest, None, 200).await.unwrap();
    let first_elapsed = t1.elapsed();
    eprintln!(
        "MEASUREMENT timeline prima pagina {N}: {first_elapsed:?} ({} righe)",
        first_page.len()
    );
    assert!(
        first_elapsed < TIMELINE_BUDGET,
        "prima pagina: {first_elapsed:?} >= {TIMELINE_BUDGET:?}"
    );

    let deep_month = buckets[buckets.len() / 2].month;
    let deep_first = repo.page(&ctx, deep_month, None, 200).await.unwrap();
    let cursor = deep_first
        .last()
        .map(|a| (a.taken_at_utc.expect("indexed ha taken_at"), a.id));
    let t2 = Instant::now();
    let deep_page = repo.page(&ctx, deep_month, cursor, 200).await.unwrap();
    let deep_elapsed = t2.elapsed();
    eprintln!(
        "MEASUREMENT timeline keyset profondo {N}: {deep_elapsed:?} ({} righe)",
        deep_page.len()
    );
    assert!(
        deep_elapsed < TIMELINE_BUDGET,
        "pagina profonda: {deep_elapsed:?} >= {TIMELINE_BUDGET:?}"
    );

    let search = SearchRepo::new(test.db());
    let t3 = Instant::now();
    let hits = search
        .run(
            &ctx,
            &SearchNode::Text {
                value: "IMG_150000".into(),
            },
            None,
            50,
        )
        .await
        .unwrap();
    let search_elapsed = t3.elapsed();
    eprintln!(
        "MEASUREMENT ricerca trgm/ILIKE {N}: {search_elapsed:?} ({} hit)",
        hits.len()
    );
    assert!(!hits.is_empty(), "IMG_150000.jpg deve esistere fra le 200k");
    assert!(
        search_elapsed < SEARCH_BUDGET,
        "ricerca: {search_elapsed:?} >= {SEARCH_BUDGET:?}"
    );

    eprintln!(
        "EXPLAIN buckets:\n{}",
        explain_buckets(test.db().pool(), library_id.as_uuid()).await
    );
    eprintln!(
        "EXPLAIN timeline page:\n{}",
        explain_page(test.db().pool(), newest).await
    );
    eprintln!(
        "EXPLAIN search:\n{}",
        explain_search(test.db().pool()).await
    );
}

/// Stessa query di `TimelineRepo::buckets` (Task 3: conta le pile da
/// `assets` con il filtro di primario, non più `folder_month_counts` — vedi
/// Ruling nel ledger fase-10). Duplicata qui come `explain_page_shared`,
/// perché la query vera è privata del repository.
async fn explain_buckets(pool: &sqlx::PgPool, library_id: uuid::Uuid) -> String {
    let rows: Vec<(String,)> = sqlx::query_as(
        "EXPLAIN (ANALYZE, BUFFERS) \
         SELECT date_trunc('month', a.taken_at_utc)::date AS month, \
                count(*)::bigint AS count \
           FROM assets a \
           JOIN folders f ON f.id = a.folder_id \
           LEFT JOIN stacks s ON s.id = a.stack_id \
          WHERE ($1::uuid[] IS NULL OR f.library_id = ANY($1::uuid[])) \
            AND ($2::uuid IS NULL OR f.library_id = $2) \
            AND a.status = 'indexed' \
            AND a.kind <> 'unknown' \
            AND a.taken_at_utc IS NOT NULL \
            AND (a.stack_id IS NULL OR a.id = s.primary_asset_id) \
          GROUP BY month \
          ORDER BY month DESC",
    )
    .bind(None::<Vec<uuid::Uuid>>)
    .bind(library_id)
    .fetch_all(pool)
    .await
    .unwrap();
    join_plan(rows)
}

async fn explain_page(pool: &sqlx::PgPool, month: NaiveDate) -> String {
    let start = month.and_hms_opt(0, 0, 0).unwrap().and_utc();
    let end = month
        .checked_add_months(chrono::Months::new(1))
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc();
    let rows: Vec<(String,)> = sqlx::query_as(
        "EXPLAIN (ANALYZE, BUFFERS) \
         SELECT a.id FROM assets a \
         JOIN folders f ON f.id = a.folder_id \
         LEFT JOIN stacks s ON s.id = a.stack_id \
         LEFT JOIN LATERAL ( \
             SELECT count(*)::int2 AS stack_size, \
                    CASE WHEN bool_or(m.kind = 'raw_image') AND bool_or(m.kind = 'image') \
                         THEN 'raw+jpeg' \
                         WHEN bool_or(m.kind = 'raw_image') THEN 'raw' \
                         WHEN bool_or(m.kind = 'image') THEN 'jpeg' \
                         ELSE NULL END AS raw_kind \
               FROM assets m WHERE m.stack_id = a.stack_id \
         ) si ON a.stack_id IS NOT NULL \
         WHERE ($6::uuid[] IS NULL OR f.library_id = ANY($6::uuid[])) \
           AND a.status = 'indexed' \
           AND a.kind <> 'unknown' \
           AND a.taken_at_utc >= $1 AND a.taken_at_utc < $2 \
           AND ($3::timestamptz IS NULL \
                OR a.taken_at_utc < $3 \
                OR (a.taken_at_utc = $3 AND a.id < $4)) \
           AND (a.stack_id IS NULL OR a.id = s.primary_asset_id) \
         ORDER BY a.taken_at_utc DESC NULLS LAST, a.id DESC \
         LIMIT $5",
    )
    .bind(start)
    .bind(end)
    .bind(None::<chrono::DateTime<chrono::Utc>>)
    .bind(None::<uuid::Uuid>)
    .bind(200_i64)
    .bind(None::<Vec<uuid::Uuid>>)
    .fetch_all(pool)
    .await
    .unwrap();
    join_plan(rows)
}

async fn explain_search(pool: &sqlx::PgPool) -> String {
    let rows: Vec<(String,)> = sqlx::query_as(
        "EXPLAIN (ANALYZE, BUFFERS) \
         SELECT a.id FROM assets a \
         JOIN folders f ON f.id = a.folder_id \
         LEFT JOIN asset_exif e ON e.asset_id = a.id \
         LEFT JOIN stacks s ON s.id = a.stack_id \
         WHERE ($1::uuid[] IS NULL OR f.library_id = ANY($1::uuid[])) \
           AND a.status = 'indexed' \
           AND a.filename ILIKE '%IMG_150000%' ESCAPE E'\\\\' \
           AND (a.stack_id IS NULL OR a.id = s.primary_asset_id) \
         ORDER BY a.taken_at_utc DESC NULLS LAST, a.id DESC \
         LIMIT 50",
    )
    .bind(None::<Vec<uuid::Uuid>>)
    .fetch_all(pool)
    .await
    .unwrap();
    join_plan(rows)
}

fn join_plan(rows: Vec<(String,)>) -> String {
    rows.into_iter()
        .map(|(line,)| line)
        .collect::<Vec<_>>()
        .join("\n")
}

#[tokio::test]
async fn timeline_with_fifty_permissions_stays_under_budget_at_200k() {
    let test = TestDb::start().await;
    let (admin_ctx, library_id) = seed_two_hundred_thousand(&test).await;
    let admin = admin_ctx.user_id().unwrap();
    let mario = harness::seed_user(&test, admin, "mario").await;

    let folders = FolderRepo::new(test.db());
    let root = folders.ensure_path(library_id, &[]).await.unwrap();
    let mut granted = folders.children(&admin_ctx, root.id).await.unwrap();
    while granted.len() < 50 {
        let n = granted.len();
        let extra = folders
            .ensure_path(library_id, &[&format!("perm{n}")])
            .await
            .unwrap();
        granted.push(extra);
    }
    granted.truncate(50);

    let perms = PermissionRepo::new(test.db());
    for folder in &granted {
        perms
            .grant(
                &admin_ctx,
                keeppix_db::NewGrant {
                    subject: SubjectType::User,
                    subject_id: mario.as_uuid(),
                    object: ObjectType::Folder,
                    object_id: folder.id.as_uuid(),
                    role: ObjectRole::Viewer,
                    inherit: true,
                },
            )
            .await
            .unwrap();
    }

    sqlx::query("ANALYZE folders")
        .execute(test.db().pool())
        .await
        .unwrap();
    sqlx::query("ANALYZE permissions")
        .execute(test.db().pool())
        .await
        .unwrap();
    sqlx::query("ANALYZE assets")
        .execute(test.db().pool())
        .await
        .unwrap();

    let ctx = AuthContext::user(mario, SystemRole::User);
    let repo = TimelineRepo::new(test.db());

    let t0 = Instant::now();
    let buckets = repo.buckets(&ctx, None).await.unwrap();
    let buckets_elapsed = t0.elapsed();
    eprintln!(
        "MEASUREMENT buckets 200k/50perm: {buckets_elapsed:?} ({} mesi)",
        buckets.len()
    );
    assert!(
        buckets_elapsed < TIMELINE_BUDGET,
        "buckets con 50 permessi: {buckets_elapsed:?}"
    );

    let newest = buckets.first().expect("almeno un mese").month;
    let t1 = Instant::now();
    let page = repo.page(&ctx, newest, None, 200).await.unwrap();
    let page_elapsed = t1.elapsed();
    eprintln!(
        "MEASUREMENT timeline 200k/50perm: {page_elapsed:?} ({} righe)",
        page.len()
    );
    assert!(!page.is_empty());
    assert!(
        page_elapsed < TIMELINE_BUDGET,
        "timeline con 50 permessi a 200k: {page_elapsed:?}"
    );

    let scope = VisibilityScope::resolve(test.db(), &ctx).await.unwrap();
    eprintln!(
        "EXPLAIN EXISTS/NOT EXISTS (scelta) 200k/50perm:\n{}",
        explain_page_shared(test.db().pool(), newest, &scope).await
    );
    eprintln!(
        "EXPLAIN CTE ricorsiva (confronto) 200k/50perm:\n{}",
        explain_page_cte(test.db().pool(), newest, &scope).await
    );
}

async fn explain_page_shared(
    pool: &sqlx::PgPool,
    month: NaiveDate,
    scope: &VisibilityScope,
) -> String {
    let start = month.and_hms_opt(0, 0, 0).unwrap().and_utc();
    let end = month
        .checked_add_months(chrono::Months::new(1))
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc();
    let filter = scope.filter("f.path", "f.library_id", "a.id", 6);
    let sql = format!(
        "EXPLAIN (ANALYZE, BUFFERS) \
         SELECT a.id FROM assets a \
         JOIN folders f ON f.id = a.folder_id \
         LEFT JOIN stacks s ON s.id = a.stack_id \
         WHERE {} \
           AND a.status = 'indexed' \
           AND a.kind <> 'unknown' \
           AND a.taken_at_utc >= $1 AND a.taken_at_utc < $2 \
           AND ($3::timestamptz IS NULL \
                OR a.taken_at_utc < $3 \
                OR (a.taken_at_utc = $3 AND a.id < $4)) \
           AND (a.stack_id IS NULL OR a.id = s.primary_asset_id) \
         ORDER BY a.taken_at_utc DESC NULLS LAST, a.id DESC \
         LIMIT $5",
        filter.sql()
    );
    let rows: Vec<(String,)> = sqlx::query_as(&sql)
        .bind(start)
        .bind(end)
        .bind(None::<chrono::DateTime<chrono::Utc>>)
        .bind(None::<uuid::Uuid>)
        .bind(200_i64)
        .bind(filter.bind())
        .bind(filter.holes())
        .bind(filter.assets())
        .fetch_all(pool)
        .await
        .unwrap();
    join_plan(rows)
}

async fn explain_page_cte(
    pool: &sqlx::PgPool,
    month: NaiveDate,
    scope: &VisibilityScope,
) -> String {
    let start = month.and_hms_opt(0, 0, 0).unwrap().and_utc();
    let end = month
        .checked_add_months(chrono::Months::new(1))
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc();
    let grants = scope
        .filter("f.path", "f.library_id", "a.id", 6)
        .bind()
        .unwrap()
        .to_vec();
    let rows: Vec<(String,)> = sqlx::query_as(
        "EXPLAIN (ANALYZE, BUFFERS) \
         WITH RECURSIVE vis AS ( \
             SELECT f.id, f.path, f.library_id FROM folders f \
              WHERE f.id = ANY($6::uuid[]) \
             UNION ALL \
             SELECT c.id, c.path, c.library_id FROM folders c \
               JOIN vis ON c.parent_id = vis.id \
         ) \
         SELECT a.id FROM assets a \
         JOIN vis ON a.folder_id = vis.id \
         WHERE a.status = 'indexed' \
           AND a.kind <> 'unknown' \
           AND a.taken_at_utc >= $1 AND a.taken_at_utc < $2 \
           AND ($3::timestamptz IS NULL \
                OR a.taken_at_utc < $3 \
                OR (a.taken_at_utc = $3 AND a.id < $4)) \
         ORDER BY a.taken_at_utc DESC NULLS LAST, a.id DESC \
         LIMIT $5",
    )
    .bind(start)
    .bind(end)
    .bind(None::<chrono::DateTime<chrono::Utc>>)
    .bind(None::<uuid::Uuid>)
    .bind(200_i64)
    .bind(&grants)
    .fetch_all(pool)
    .await
    .unwrap();
    join_plan(rows)
}
