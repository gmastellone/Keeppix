#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Prova di scala sintetica: 200.000 righe `assets`, niente file.
//! Misura le query della timeline e della ricerca, non l'I/O di ingestione.

mod harness;

use std::time::{Duration, Instant};

use chrono::NaiveDate;
use harness::TestDb;
use keeppix_db::{FolderRepo, LibraryRepo, SearchNode, SearchRepo, TimelineRepo};
use keeppix_domain::{AuthContext, NewLibrary, SystemRole};

const N: i32 = 200_000;
const TIMELINE_BUDGET: Duration = Duration::from_millis(300);
const SEARCH_BUDGET: Duration = Duration::from_millis(500);

async fn seed_two_hundred_thousand(
    test: &TestDb,
) -> (AuthContext, keeppix_domain::LibraryId) {
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

async fn explain_buckets(pool: &sqlx::PgPool, library_id: uuid::Uuid) -> String {
    let rows: Vec<(String,)> = sqlx::query_as(
        "EXPLAIN (ANALYZE, BUFFERS) \
         SELECT fmc.month, sum(fmc.asset_count)::bigint AS count \
           FROM folder_month_counts fmc \
           JOIN folders f ON f.id = fmc.folder_id \
          WHERE ($1::uuid[] IS NULL OR f.library_id = ANY($1::uuid[])) \
            AND ($2::uuid IS NULL OR f.library_id = $2) \
          GROUP BY fmc.month \
          ORDER BY fmc.month DESC",
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
         WHERE ($6::uuid[] IS NULL OR f.library_id = ANY($6::uuid[])) \
           AND a.status = 'indexed' \
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
         WHERE ($1::uuid[] IS NULL OR f.library_id = ANY($1::uuid[])) \
           AND a.status = 'indexed' \
           AND a.filename ILIKE '%IMG_150000%' ESCAPE E'\\\\' \
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
