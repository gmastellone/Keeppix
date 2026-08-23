#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss
)]

//! Fase 9 Task 11: la misura richiesta da `PROSEGUI.md` §5 — "tempo di una
//! rinomina di massa su 500 file", prodotta, non stimata.

mod harness;

use std::fs;
use std::time::{Duration, Instant};

use harness::TestDb;
use keeppix_db::{FolderRepo, LibraryRepo, RenameRepo};
use keeppix_domain::{AuthContext, NewLibrary, SystemRole};

/// Nessun tetto pubblicato in `PROSEGUI.md` per questo numero (a differenza
/// della soglia interattiva di 1s per la scansione vettoriale, Fase 10 §7
/// Task 2) — scelto qui come guardia di regressione: 500 `move_asset`
/// sequenziali, ciascuno una connessione propria più un vero `rename()` su
/// disco, restano ben dentro un'interazione utente su un ambito di quella
/// taglia.
const BUDGET: Duration = Duration::from_secs(5);
const N: i64 = 500;

#[tokio::test]
async fn renaming_500_files_stays_within_budget() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let root = std::env::temp_dir().join(format!(
        "keeppix-scale-rename-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("orologio di sistema")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("radice di test");

    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let library = LibraryRepo::new(test.db())
        .create(
            &ctx,
            NewLibrary {
                name: "Scala".to_owned(),
                owner_id: admin,
                root_path: root.clone(),
                exclude_patterns: vec![],
            },
        )
        .await
        .expect("libreria")
        .id;
    let folder = FolderRepo::new(test.db())
        .ensure_path(library, &["viaggio"])
        .await
        .expect("cartella")
        .id;
    fs::create_dir_all(root.join("viaggio")).expect("cartella su disco");

    // 500 file veri su disco (il contenuto non conta: `move_asset` sposta il
    // percorso, non legge i byte) più le 500 righe `assets` corrispondenti in
    // un solo INSERT — lo stesso stile di `scale_embeddings.rs`, per tenere
    // il seeding fuori dal tempo misurato.
    let seed_files = Instant::now();
    for i in 1..=N {
        fs::write(root.join("viaggio").join(format!("IMG_{i:05}.jpg")), b"x")
            .expect("file su disco");
    }
    eprintln!("seeded {N} files on disk in {:?}", seed_files.elapsed());

    let seed_rows = Instant::now();
    sqlx::query(
        "INSERT INTO assets (id, folder_id, filename, size_bytes, mtime, kind, status, \
                            taken_at_utc, width, height, content_hash) \
         SELECT gen_random_uuid(), $1, \
                'IMG_' || lpad(g::text, 5, '0') || '.jpg', \
                1, now(), 'image', 'indexed', \
                timestamptz '2026-08-14' + make_interval(mins => g::int), \
                100, 100, \
                decode(lpad(to_hex(g), 64, '0'), 'hex') \
           FROM generate_series(1, $2) AS g",
    )
    .bind(folder.as_uuid())
    .bind(N)
    .execute(test.db().pool())
    .await
    .unwrap();
    eprintln!("seeded {N} asset rows in {:?}", seed_rows.elapsed());

    let asset_ids: Vec<keeppix_domain::AssetId> = sqlx::query_scalar::<_, uuid::Uuid>(
        "SELECT id FROM assets WHERE folder_id = $1 ORDER BY filename",
    )
    .bind(folder.as_uuid())
    .fetch_all(test.db().pool())
    .await
    .unwrap()
    .into_iter()
    .map(keeppix_domain::AssetId::from_uuid)
    .collect();
    assert_eq!(asset_ids.len(), N as usize);

    let timed = Instant::now();
    let outcome = RenameRepo::new(test.db())
        .apply(&ctx, &asset_ids, "Viaggio_{n:3}", false)
        .await
        .expect("rinomina di massa");
    let elapsed = timed.elapsed();
    let elapsed_ms = elapsed.as_secs_f64() * 1000.0;
    eprintln!(
        "MEASUREMENT Fase 9 Task 11: rinomina di massa su {N} file: {elapsed_ms:.1} ms \
         ({:.2} ms/file)",
        elapsed_ms / N as f64
    );

    assert_eq!(outcome.renamed.len(), N as usize);
    assert!(outcome.failed.is_empty());
    assert!(
        elapsed < BUDGET,
        "rinomina di 500 file in {elapsed:?}, oltre il budget di {BUDGET:?}"
    );

    let _ = fs::remove_dir_all(&root);
}
