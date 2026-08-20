mod harness;

use harness::TestDb;

#[tokio::test]
#[allow(clippy::expect_used)]
async fn migrations_apply_to_an_empty_database() {
    let test = TestDb::start().await;
    test.db().ping().await.expect("il database risponde");
}

#[tokio::test]
#[allow(clippy::expect_used)]
async fn migrations_are_idempotent() {
    let test = TestDb::start().await;
    // Rieseguire il migratore su un database già migrato non deve fallire.
    test.db().migrate().await.expect("seconda esecuzione");
}

#[tokio::test]
#[allow(clippy::expect_used)]
async fn expected_tables_exist() {
    let test = TestDb::start().await;

    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT table_name FROM information_schema.tables \
         WHERE table_schema = 'public' ORDER BY table_name",
    )
    .fetch_all(test.db().pool())
    .await
    .expect("elenco tabelle");

    for expected in [
        "idempotency_keys",
        "users",
        "groups",
        "group_members",
        "map_regions",
        "places",
        "sessions",
        "system_settings",
        "tz_boundaries",
    ] {
        assert!(
            tables.contains(&expected.to_owned()),
            "manca la tabella {expected}"
        );
    }
}

/// Le due estensioni devono essere attive già dalla `0001`. `postgis` non è
/// *trusted*: crearla richiede il superuser, quindi va fatto sul database
/// vuoto appena creato dall'amministratore e non nella migrazione della Fase 4,
/// quando l'istanza gestita è già piena di dati e l'utente applicativo non ha
/// più quel privilegio. La roadmap della Fase 4 dà per assodato che la `0001`
/// l'abbia già abilitata: questo test è ciò che lo rende vero.
#[tokio::test]
#[allow(clippy::expect_used)]
async fn required_extensions_are_enabled() {
    let test = TestDb::start().await;

    let extensions: Vec<String> = sqlx::query_scalar("SELECT extname FROM pg_extension")
        .fetch_all(test.db().pool())
        .await
        .expect("elenco estensioni");

    for expected in ["pg_trgm", "postgis"] {
        assert!(
            extensions.contains(&expected.to_owned()),
            "l'estensione {expected} deve essere abilitata dalla migrazione 0001"
        );
    }
}

#[tokio::test]
#[allow(clippy::expect_used)]
async fn performance_indexes_exist() {
    let test = TestDb::start().await;

    let indexes: Vec<String> = sqlx::query_scalar(
        "SELECT indexname FROM pg_indexes WHERE schemaname = 'public' ORDER BY indexname",
    )
    .fetch_all(test.db().pool())
    .await
    .expect("elenco indici");

    for expected in [
        "album_assets_added_by_idx",
        "asset_exif_camera_trgm",
        "asset_exif_lens_trgm",
        "stacks_primary_asset_idx",
    ] {
        assert!(
            indexes.contains(&expected.to_owned()),
            "manca l'indice {expected}"
        );
    }
}

#[tokio::test]
#[allow(clippy::expect_used)]
async fn assets_autovacuum_scale_factor_is_aggressive() {
    let test = TestDb::start().await;

    let reloptions: Option<Vec<String>> = sqlx::query_scalar(
        "SELECT c.reloptions FROM pg_class c \
         JOIN pg_namespace n ON n.oid = c.relnamespace \
         WHERE n.nspname = 'public' AND c.relname = 'assets'",
    )
    .fetch_one(test.db().pool())
    .await
    .expect("assets table");

    let opts = reloptions.unwrap_or_default();
    assert!(
        opts.iter()
            .any(|o| o == "autovacuum_vacuum_scale_factor=0.05"),
        "assets must vacuum at 5% dead tuples, got {opts:?}"
    );
}

#[tokio::test]
#[allow(clippy::expect_used)]
async fn usernames_are_unique_case_insensitively() {
    let test = TestDb::start().await;
    let pool = test.db().pool();

    let insert = "INSERT INTO users (id, username, display_name, password_hash, role) \
                  VALUES ($1, $2, 'X', 'hash', 'user')";

    sqlx::query(insert)
        .bind(uuid::Uuid::now_v7())
        .bind("giovanni")
        .execute(pool)
        .await
        .expect("primo inserimento");

    let second = sqlx::query(insert)
        .bind(uuid::Uuid::now_v7())
        .bind("GIOVANNI")
        .execute(pool)
        .await;

    assert!(
        second.is_err(),
        "l'indice unico deve rifiutare il duplicato"
    );
}

/// Manual EXPLAIN capture for Task 1bis ledger (`--ignored --nocapture`).
#[tokio::test]
#[ignore = "ledger evidence: cargo test -p keeppix-db explain_guc -- --ignored --nocapture"]
#[allow(clippy::expect_used, clippy::unwrap_used)]
async fn explain_guc_plans_for_ledger() {
    use keeppix_db::{FolderRepo, LibraryRepo};
    use keeppix_domain::{AuthContext, NewLibrary, SystemRole};

    let test = TestDb::start().await;
    let pool = test.db().pool();
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let library = LibraryRepo::new(test.db())
        .create(
            &ctx,
            NewLibrary {
                name: "Explain".to_owned(),
                owner_id: admin,
                root_path: std::path::PathBuf::from("/mnt/explain"),
                exclude_patterns: vec![],
            },
        )
        .await
        .unwrap();
    let folder = FolderRepo::new(test.db())
        .ensure_path(library.id, &["2015"])
        .await
        .unwrap();

    sqlx::query("ALTER TABLE assets DISABLE TRIGGER assets_month_counts")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO assets (id, folder_id, filename, size_bytes, mtime, kind, status, \
                            taken_at_utc, width, height) \
         SELECT gen_random_uuid(), $1, 'IMG_' || lpad(g::text, 6, '0') || '.jpg', \
                200000, timestamptz '2015-01-01' + make_interval(hours => g), \
                'image', 'indexed', \
                timestamptz '2015-01-01' + make_interval(hours => g), 4000, 3000 \
           FROM generate_series(1, 15000) AS g",
    )
    .bind(folder.id.as_uuid())
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("ANALYZE assets").execute(pool).await.unwrap();

    let timeline_sql = "SELECT a.id FROM assets a \
        JOIN folders f ON f.id = a.folder_id \
        WHERE a.status = 'indexed' AND a.kind <> 'unknown' \
          AND a.taken_at_utc >= timestamptz '2015-06-01' \
          AND a.taken_at_utc < timestamptz '2015-07-01' \
        ORDER BY a.taken_at_utc DESC NULLS LAST, a.id DESC LIMIT 200";
    let geometry_sql = "SELECT folder_id, taken_at_utc, id, width, height FROM assets \
        WHERE status = 'indexed' \
        ORDER BY folder_id, taken_at_utc DESC, id DESC LIMIT 200";

    for rpc in [4.0, 1.1] {
        let set = format!("SET random_page_cost = {rpc}");
        sqlx::query(&set).execute(pool).await.unwrap();
        println!("\n=== random_page_cost={rpc} ===");
        println!("-- timeline page");
        for line in explain_lines(pool, timeline_sql).await {
            println!("{line}");
        }
        println!("-- geometry stand-in");
        for line in explain_lines(pool, geometry_sql).await {
            println!("{line}");
        }
    }
}

async fn explain_lines(pool: &sqlx::PgPool, sql: &str) -> Vec<String> {
    let explain = format!("EXPLAIN (FORMAT TEXT) {sql}");
    sqlx::query_scalar::<_, String>(&explain)
        .fetch_all(pool)
        .await
        .unwrap_or_default()
}
