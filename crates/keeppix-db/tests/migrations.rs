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
        "users",
        "groups",
        "group_members",
        "sessions",
        "system_settings",
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
