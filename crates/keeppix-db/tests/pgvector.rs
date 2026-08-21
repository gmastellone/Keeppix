mod harness;

use harness::TestDb;
use keeppix_db::{PgVectorStatus, probe_pgvector};

/// Con `postgis/postgis:17-3.5` (senza pacchetto pgvector) il probe deve
/// riportare assenza — non fallire. È il percorso degradato di chi punta un
/// Postgres esterno senza l'estensione. Le migrazioni devono comunque
/// riuscirci (schema AI saltato se `vector` non è installabile).
#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn postgis_only_image_reports_vector_unavailable() {
    let test = TestDb::start_postgis_only().await;
    let status = probe_pgvector(test.db()).await.unwrap();

    assert!(
        !status.available,
        "postgis/postgis:17-3.5 non installa pgvector: available deve essere false"
    );
    assert!(!status.enabled);
    assert!(
        status.message.is_some(),
        "senza vector serve un messaggio leggibile per l'interfaccia"
    );
    let message = status.message.as_deref().unwrap();
    assert!(
        message.contains("pgvector") || message.contains("vector"),
        "il messaggio deve nominare l'estensione: {message}"
    );
    assert_eq!(
        status.enable_command.as_deref(),
        Some(PgVectorStatus::ENABLE_SQL),
        "il messaggio deve indicare il comando SQL da eseguire dopo l'installazione"
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
        "senza pgvector le tabelle AI non devono esistere (degrade, non fail)"
    );
}

/// Sull'immagine bundled (`keeppix-db:dev`) il probe vede l'estensione e la
/// migrazione Task 4 la abilita.
#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn bundled_image_enables_vector_via_migration() {
    let test = TestDb::start().await;
    let status = probe_pgvector(test.db()).await.unwrap();

    assert!(status.available, "keeppix-db:dev deve offrire pgvector");
    assert!(
        status.enabled,
        "la migrazione AI deve eseguire CREATE EXTENSION vector"
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
