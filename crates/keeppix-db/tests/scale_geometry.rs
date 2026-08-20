#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Prova di scala per `/timeline/geometry` (fase-10, Task 2): 200.000 righe
//! `assets`, tutte indicizzate con `width`/`height` noti, in una sola
//! libreria. Verifica due cose che Task 1bis aveva lasciato in sospeso:
//!
//! 1. la query di `TimelineRepo::geometry` resta sotto un budget esplicito
//!    anche sull'intera vista (nessuna paginazione, a differenza di
//!    `page`/`buckets`);
//! 2. il piano usa `assets_geometry_idx` in **index-only scan** — l'indice di
//!    copertura di 0034, non un seq scan come lo stand-in misurato in
//!    Task 1bis.

mod harness;

use std::time::{Duration, Instant};

use harness::TestDb;
use keeppix_db::{FolderRepo, LibraryRepo, TimelineRepo, VisibilityScope};
use keeppix_domain::{AuthContext, NewLibrary, SystemRole};

const N: i32 = 200_000;

/// Più alto del budget di `page`/`buckets` (300ms) di proposito: qui non c'è
/// `LIMIT`, quindi il costo dominante non è il piano (misurato via `EXPLAIN
/// ANALYZE`: ~110ms server-side, `Index Only Scan` su `assets_geometry_idx`,
/// `Heap Fetches: 0`) ma il trasferimento e la decodifica lato client di
/// 200.000 righe in un colpo — che è esattamente il costo che questo
/// endpoint sostituisce a 1.070 richieste paginate (spec fase-10 §2.2), non
/// uno che aggiunge. Misurato ~600-650ms end-to-end su questo container;
/// 900ms lascia margine senza avvicinarsi al ~2s del seq scan degradato che
/// la spec cita come alternativa senza l'indice di copertura.
const GEOMETRY_BUDGET: Duration = Duration::from_millis(900);

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

    // `VACUUM`, non solo `ANALYZE`: l'index-only scan ha bisogno della
    // visibility map fresca, che `ANALYZE` da solo non aggiorna. Senza
    // questo il piano può ancora fare heap fetch per ogni riga anche con
    // l'indice di copertura presente.
    sqlx::query("VACUUM ANALYZE assets")
        .execute(test.db().pool())
        .await
        .unwrap();
    eprintln!(
        "MEASUREMENT seed geometria {N} assets: {:?}",
        seeded.elapsed()
    );
    (ctx, library.id)
}

#[tokio::test]
async fn geometry_of_two_hundred_thousand_assets_stays_within_budget_and_index_only() {
    let test = TestDb::start().await;
    let (ctx, library_id) = seed_two_hundred_thousand_sized(&test).await;
    let repo = TimelineRepo::new(test.db());

    let t0 = Instant::now();
    let geometry = repo.geometry(&ctx, Some(library_id)).await.unwrap();
    let elapsed = t0.elapsed();
    eprintln!(
        "MEASUREMENT geometry (whole view) {N}: {elapsed:?} ({} record)",
        geometry.records.len()
    );
    assert_eq!(
        geometry.records.len(),
        usize::try_from(N).unwrap(),
        "tutti gli asset indicizzati con taken_at devono comparire nella geometria"
    );
    assert!(
        elapsed < GEOMETRY_BUDGET,
        "geometria di {N} scatti: {elapsed:?} >= {GEOMETRY_BUDGET:?}"
    );

    let plan = explain_geometry(&test, &ctx, library_id).await;
    eprintln!("EXPLAIN geometry:\n{plan}");
    assert!(
        plan.contains("Index Only Scan") && plan.contains("assets_geometry_idx"),
        "la query di /timeline/geometry deve servirsi dal solo assets_geometry_idx, \
         non degradare a seq scan o a heap fetch per riga:\n{plan}"
    );
}

/// Stessa query SQL di `TimelineRepo::geometry`, con lo stesso filtro
/// libreria del test sopra: duplicata qui di proposito, come
/// `explain_page_shared` in `scale_200k.rs`, perché la query vera è privata
/// del repository e l'`EXPLAIN` deve vedere esattamente ciò che gira in
/// produzione.
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
         WHERE {} \
           AND a.status = 'indexed' \
           AND a.taken_at_utc IS NOT NULL \
           AND ($4::uuid IS NULL OR f.library_id = $4) \
         ORDER BY a.taken_at_utc DESC, a.id DESC",
        filter.sql()
    );
    // Stessi bind del test sopra: `Some(library_id)`, non `None` — con
    // `None` il planner sceglie `assets_timeline_idx` invece del nuovo
    // indice di copertura (nessun `folder_id` da restringere), un piano
    // diverso da quello che gira davvero dietro `?library=...`.
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
