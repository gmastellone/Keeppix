mod harness;

use harness::TestDb;
use keeppix_db::{PgVectorStatus, probe_pgvector};

/// With `postgis/postgis:17-3.5` (no pgvector package) the probe must
/// report absence — not fail. This is the degraded path for someone
/// pointing at an external Postgres without the extension. Migrations must
/// still succeed there (AI schema skipped if `vector` cannot be installed).
#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn postgis_only_image_reports_vector_unavailable() {
    let test = TestDb::start_postgis_only().await;
    let status = probe_pgvector(test.db()).await.unwrap();

    assert!(
        !status.available,
        "postgis/postgis:17-3.5 does not install pgvector: available must be false"
    );
    assert!(!status.enabled);
    assert!(
        status.message.is_some(),
        "without vector, a readable message is needed for the UI"
    );
    let message = status.message.as_deref().unwrap();
    assert!(
        message.contains("pgvector") || message.contains("vector"),
        "the message must name the extension: {message}"
    );
    assert_eq!(
        status.enable_command.as_deref(),
        Some(PgVectorStatus::ENABLE_SQL),
        "the message must give the SQL command to run after installing"
    );

    let ai_tables: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint FROM information_schema.tables \
         WHERE table_schema = 'public' \
           AND table_name IN ('asset_embeddings', 'tags', 'asset_tags')",
    )
    .fetch_one(test.db().pool())
    .await
    .unwrap();
    assert_eq!(
        ai_tables, 0,
        "without pgvector the AI tables must not exist (degrade, not fail)"
    );
}

/// On the bundled image (`keeppix-db:dev`) the probe sees the extension and
/// the AI migration enables it.
#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn bundled_image_enables_vector_via_migration() {
    let test = TestDb::start().await;
    let status = probe_pgvector(test.db()).await.unwrap();

    assert!(status.available, "keeppix-db:dev must offer pgvector");
    assert!(
        status.enabled,
        "the AI migration must run CREATE EXTENSION vector"
    );
    assert!(status.message.is_none());
    assert!(status.enable_command.is_none());
}

#[test]
#[allow(clippy::expect_used)]
fn missing_status_names_pgvector_and_the_create_extension_command() {
    let status = PgVectorStatus::missing();
    assert!(!status.available);
    assert!(!status.enabled);
    let message = status.message.as_deref().expect("message");
    assert!(message.contains("CREATE EXTENSION IF NOT EXISTS vector"));
    assert!(message.contains("pgvector"));
    assert_eq!(
        status.enable_command.as_deref(),
        Some(PgVectorStatus::ENABLE_SQL)
    );
}

#[test]
fn present_status_has_no_disable_message() {
    let status = PgVectorStatus::present(true);
    assert!(status.available);
    assert!(status.enabled);
    assert!(status.message.is_none());
    assert!(status.enable_command.is_none());
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn persist_pgvector_status_survives_a_reload() {
    let test = TestDb::start_postgis_only().await;
    let status = keeppix_db::persist_pgvector_status(test.db())
        .await
        .unwrap();
    assert!(!status.available);

    let stored = keeppix_db::SettingsRepo::new(test.db())
        .get_json("pgvector")
        .await
        .unwrap()
        .expect("pgvector setting");
    assert_eq!(stored["available"], serde_json::json!(false));
    assert!(
        stored["message"].as_str().unwrap().contains("pgvector"),
        "persisted message must stay readable for the UI"
    );
    assert_eq!(
        stored["enable_command"].as_str(),
        Some(keeppix_db::PgVectorStatus::ENABLE_SQL)
    );
}
