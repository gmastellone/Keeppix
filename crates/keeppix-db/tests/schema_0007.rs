mod harness;

use harness::TestDb;

#[allow(clippy::unwrap_used)]
async fn insert_job(
    test: &TestDb,
    status: &str,
    dedup_key: Option<&str>,
) -> Result<sqlx::postgres::PgQueryResult, sqlx::Error> {
    sqlx::query(
        "INSERT INTO jobs (kind, payload, status, dedup_key) \
         VALUES ('discover_library', '{}'::jsonb, $1, $2)",
    )
    .bind(status)
    .bind(dedup_key)
    .execute(test.db().pool())
    .await
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn pending_dedup_keys_are_unique() {
    let test = TestDb::start().await;
    insert_job(&test, "pending", Some("hash:abc"))
        .await
        .expect("first pending");

    let dup = insert_job(&test, "pending", Some("hash:abc")).await;
    let err = dup.expect_err("two pending jobs with the same key are the same work");
    let db_err = err.as_database_error().expect("postgres error");
    assert_eq!(
        db_err.code().as_deref(),
        Some("23505"),
        "must be the partial unique index, not another violation"
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn a_done_job_does_not_block_a_new_pending() {
    let test = TestDb::start().await;
    insert_job(&test, "done", Some("hash:abc"))
        .await
        .expect("done");
    insert_job(&test, "pending", Some("hash:abc"))
        .await
        .expect("a pending after a done is a new run");
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn an_unknown_status_is_rejected() {
    let test = TestDb::start().await;
    insert_job(&test, "pending", None)
        .await
        .expect("valid pending");

    let bad = insert_job(&test, "queued", None).await;
    let err = bad.expect_err("status only accepts pending, running, done, failed");
    let db_err = err.as_database_error().expect("postgres error");
    assert_eq!(
        db_err.code().as_deref(),
        Some("23514"),
        "must be the CHECK, not another violation"
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn the_claim_index_exists() {
    let test = TestDb::start().await;

    let present: bool = sqlx::query_scalar(
        "SELECT EXISTS ( \
             SELECT 1 FROM pg_indexes \
              WHERE tablename = 'jobs' AND indexname = 'jobs_claim_idx' \
         )",
    )
    .fetch_one(test.db().pool())
    .await
    .unwrap();

    assert!(
        present,
        "the SKIP LOCKED claim paginates on (priority, run_after, id)"
    );
}
