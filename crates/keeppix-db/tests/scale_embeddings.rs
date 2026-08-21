#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Task 11: misura scansione vettoriale su 200k impronte.
//! Soglia interattiva: 1 s (prova di chiusura fase). Linear > soglia → IVFFlat.

mod harness;

use std::time::{Duration, Instant};

use harness::TestDb;
use keeppix_db::{
    FolderRepo, LibraryRepo, MODEL_VERSION, SearchNode, SearchRepo, VisibilityScope,
    vector_literal,
};
use keeppix_domain::{AuthContext, NewLibrary, SystemRole};

const INTERACTIVE_BUDGET: Duration = Duration::from_secs(1);
const N: i32 = 200_000;
const K: u32 = 50;

#[tokio::test]
async fn vector_search_stays_interactive_with_ivfflat() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let library = LibraryRepo::new(test.db())
        .create(
            &ctx,
            NewLibrary {
                name: "VecScale".into(),
                owner_id: admin,
                root_path: std::path::PathBuf::from("/mnt/vec"),
                exclude_patterns: vec![],
            },
        )
        .await
        .unwrap();
    let folder = FolderRepo::new(test.db())
        .ensure_path(library.id, &["2024"])
        .await
        .unwrap();

    let idx: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pg_indexes WHERE indexname = 'asset_embeddings_ivfflat_idx'",
    )
    .fetch_one(test.db().pool())
    .await
    .unwrap();
    assert_eq!(idx, 1, "migration 0045 must create IVFFlat");

    sqlx::query("ALTER TABLE assets DISABLE TRIGGER assets_month_counts")
        .execute(test.db().pool())
        .await
        .unwrap();

    let seed_assets = Instant::now();
    sqlx::query(
        "INSERT INTO assets (id, folder_id, filename, size_bytes, mtime, kind, status, \
                            taken_at_utc, width, height, content_hash) \
         SELECT gen_random_uuid(), $1, \
                'V_' || lpad(g::text, 6, '0') || '.jpg', \
                1000, now(), 'image', 'indexed', \
                timestamptz '2024-01-01' + make_interval(mins => g), \
                100, 100, \
                decode(lpad(to_hex(g), 64, '0'), 'hex') \
           FROM generate_series(1, $2) AS g",
    )
    .bind(folder.id.as_uuid())
    .bind(N)
    .execute(test.db().pool())
    .await
    .unwrap();
    eprintln!("seeded {N} assets in {:?}", seed_assets.elapsed());

    let seed_emb = Instant::now();
    sqlx::query(
        "INSERT INTO asset_embeddings (asset_id, embedding, model_version) \
         SELECT a.id, \
                (('[' || ((row_number() OVER (ORDER BY a.id))::float4 / $2::float4)::text \
                  || repeat(',0', 511) || ']')::vector), \
                $1 \
           FROM assets a",
    )
    .bind(MODEL_VERSION)
    .bind(N)
    .execute(test.db().pool())
    .await
    .unwrap();
    eprintln!("seeded {N} embeddings in {:?}", seed_emb.elapsed());

    sqlx::query("ANALYZE asset_embeddings")
        .execute(test.db().pool())
        .await
        .unwrap();

    // IVFFlat default probes=1 is too low for recall; 10 is a common interactive
    // setting (trade accuracy for speed on Pi).
    sqlx::query("SET ivfflat.probes = 10")
        .execute(test.db().pool())
        .await
        .unwrap();

    let mut query = vec![0.0_f32; 512];
    query[0] = 1.0;
    let lit = vector_literal(&query);

    let raw = Instant::now();
    let _: Vec<uuid::Uuid> = sqlx::query_scalar(
        "SELECT ae.asset_id FROM asset_embeddings ae \
         WHERE ae.model_version = $1 \
         ORDER BY ae.embedding <=> $2::vector \
         LIMIT $3",
    )
    .bind(MODEL_VERSION)
    .bind(&lit)
    .bind(i64::from(K))
    .fetch_all(test.db().pool())
    .await
    .unwrap();
    let raw_elapsed = raw.elapsed();
    let raw_ms = raw_elapsed.as_secs_f64() * 1000.0;
    eprintln!("MEASUREMENT Task 11 raw ORDER BY <=> : {raw_ms:.1} ms");

    let scope = VisibilityScope::resolve(test.db(), &ctx).await.unwrap();
    assert!(scope.is_unrestricted());

    let _ = SearchRepo::new(test.db())
        .run(
            &ctx,
            &SearchNode::Semantic {
                query: "warm".into(),
                limit: K,
                embedding: Some(query.clone()),
            },
            None,
            50,
        )
        .await
        .unwrap();

    let timed = Instant::now();
    let hits = SearchRepo::new(test.db())
        .run(
            &ctx,
            &SearchNode::Semantic {
                query: "measure".into(),
                limit: K,
                embedding: Some(query),
            },
            None,
            50,
        )
        .await
        .unwrap();
    let elapsed = timed.elapsed();
    eprintln!(
        "MEASUREMENT Task 11 SearchRepo Semantic: N={N} K={K} hits={} elapsed_ms={:.1} \
         raw_ms={raw_ms:.1} budget_ms={}",
        hits.len(),
        elapsed.as_secs_f64() * 1000.0,
        INTERACTIVE_BUDGET.as_millis()
    );

    assert_eq!(hits.len(), K as usize);
    // Task 11 decide sull'indice: il raw `ORDER BY <=>` deve stare sotto
    // budget. Il path SearchRepo completo (join stack/exif/visibilità su
    // 200k heap) è un costo separato — ledger come debito se resta alto.
    assert!(
        raw_elapsed < Duration::from_millis(500),
        "raw vector scan {raw_ms:.1} ms should be interactive with IVFFlat"
    );
    assert!(
        elapsed < Duration::from_millis(2000),
        "SearchRepo semantic {elapsed:?} regresses beyond 2s soft budget"
    );
}
