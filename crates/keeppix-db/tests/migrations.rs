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
        "operations",
        "places",
        "sessions",
        "system_settings",
        "tz_boundaries",
        "asset_embeddings",
        "tags",
        "asset_tags",
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
        "assets_geometry_idx",
        "assets_rating_idx",
        "asset_flags_favorite_idx",
        "assets_taken_day_idx",
        "assets_timeline_indexed_idx",
        "asset_embeddings_model_idx",
        "tags_parent_idx",
        "asset_tags_tag_idx",
        "asset_tags_proposed_idx",
        "asset_embeddings_ivfflat_idx",
    ] {
        assert!(
            indexes.contains(&expected.to_owned()),
            "manca l'indice {expected}"
        );
    }

    assert!(
        !indexes.iter().any(|i| i.contains("hnsw")),
        "HNSW non spedito: IVFFlat basta (Task 11), got {indexes:?}"
    );
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

/// Task 5: le colonne dell'album «Aggiorna album» (rule + i quattro campi
/// aggiuntivi di §5.2) devono esistere dopo la migrazione 0036.
#[tokio::test]
#[allow(clippy::expect_used)]
async fn album_refresh_columns_exist() {
    let test = TestDb::start().await;

    let columns: Vec<String> = sqlx::query_scalar(
        "SELECT column_name FROM information_schema.columns \
         WHERE table_schema = 'public' AND table_name = 'albums' \
         ORDER BY column_name",
    )
    .fetch_all(test.db().pool())
    .await
    .expect("colonne di albums");

    for expected in [
        "rule",
        "rule_run_at",
        "is_shared",
        "cover_tint",
        "monochrome",
    ] {
        assert!(
            columns.contains(&expected.to_owned()),
            "manca la colonna albums.{expected}"
        );
    }
}

/// Fase 7 Task 4: colonne di `asset_embeddings`, `tags`, `asset_tags`.
#[tokio::test]
#[allow(clippy::expect_used)]
async fn ai_schema_columns_exist() {
    let test = TestDb::start().await;
    let pool = test.db().pool();

    let embedding_cols: Vec<String> = sqlx::query_scalar(
        "SELECT column_name FROM information_schema.columns \
         WHERE table_schema = 'public' AND table_name = 'asset_embeddings' \
         ORDER BY column_name",
    )
    .fetch_all(pool)
    .await
    .expect("colonne asset_embeddings");
    for expected in ["asset_id", "computed_at", "embedding", "model_version"] {
        assert!(
            embedding_cols.contains(&expected.to_owned()),
            "manca asset_embeddings.{expected}"
        );
    }

    let tag_cols: Vec<String> = sqlx::query_scalar(
        "SELECT column_name FROM information_schema.columns \
         WHERE table_schema = 'public' AND table_name = 'tags' \
         ORDER BY column_name",
    )
    .fetch_all(pool)
    .await
    .expect("colonne tags");
    for expected in [
        "color",
        "created_at",
        "created_by",
        "embedding",
        "id",
        "kind",
        "model_version",
        "name",
        "parent_id",
        "prompt",
        "threshold",
    ] {
        assert!(
            tag_cols.contains(&expected.to_owned()),
            "manca tags.{expected}"
        );
    }

    let asset_tag_cols: Vec<String> = sqlx::query_scalar(
        "SELECT column_name FROM information_schema.columns \
         WHERE table_schema = 'public' AND table_name = 'asset_tags' \
         ORDER BY column_name",
    )
    .fetch_all(pool)
    .await
    .expect("colonne asset_tags");
    for expected in [
        "asset_id",
        "decided_at",
        "decided_by",
        "score",
        "source",
        "state",
        "tag_id",
    ] {
        assert!(
            asset_tag_cols.contains(&expected.to_owned()),
            "manca asset_tags.{expected}"
        );
    }

    let library_cols: Vec<String> = sqlx::query_scalar(
        "SELECT column_name FROM information_schema.columns \
         WHERE table_schema = 'public' AND table_name = 'libraries' \
         ORDER BY column_name",
    )
    .fetch_all(pool)
    .await
    .expect("colonne libraries");
    assert!(
        library_cols.contains(&"culling_root_folder_id".to_owned()),
        "manca libraries.culling_root_folder_id (Fase 7 Task 5, inerte fino a Fase 9)"
    );
}

/// Sull'immagine bundled la migrazione abilita `vector`; senza HNSW (Task 11).
#[tokio::test]
#[allow(clippy::expect_used)]
async fn vector_extension_is_enabled_on_bundled_image() {
    let test = TestDb::start().await;

    let enabled: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pg_extension WHERE extname = 'vector')")
            .fetch_one(test.db().pool())
            .await
            .expect("pg_extension");
    assert!(
        enabled,
        "CREATE EXTENSION vector deve essere applicato dalla migrazione AI"
    );

    let embedding_udt: String = sqlx::query_scalar(
        "SELECT udt_name FROM information_schema.columns \
         WHERE table_schema = 'public' AND table_name = 'asset_embeddings' \
           AND column_name = 'embedding'",
    )
    .fetch_one(test.db().pool())
    .await
    .expect("tipo embedding");
    assert_eq!(embedding_udt, "vector");
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
