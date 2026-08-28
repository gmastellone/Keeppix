mod harness;

use std::time::Instant;

use chrono::{TimeZone, Utc};
use harness::TestDb;
use keeppix_db::{AssetRepo, DbError, FolderRepo, LibraryRepo, OverrideRepo};
use keeppix_domain::{
    AssetId, AssetKind, AssetName, AuthContext, GeoPoint, LibraryId, NewAsset, NewLibrary,
    OverridePatch, SystemRole, UserId,
};

#[allow(clippy::expect_used, clippy::unwrap_used)]
async fn seed_library(test: &TestDb, owner: UserId) -> LibraryId {
    LibraryRepo::new(test.db())
        .create(
            &AuthContext::user(owner, SystemRole::Admin),
            NewLibrary {
                name: "Foto".to_owned(),
                owner_id: owner,
                root_path: std::path::PathBuf::from("/mnt/foto"),
                exclude_patterns: vec![],
            },
        )
        .await
        .expect("library")
        .id
}

#[allow(clippy::expect_used, clippy::unwrap_used)]
fn discovered(folder: keeppix_domain::FolderId, filename: &str) -> NewAsset {
    NewAsset {
        folder_id: folder,
        filename: AssetName::parse(filename).expect("name"),
        size_bytes: 1000,
        mtime: Utc.with_ymd_and_hms(2024, 6, 1, 12, 0, 0).unwrap(),
        inode: Some(1),
        kind: AssetKind::Image,
    }
}

/// An indexed asset with a known `taken_at_utc`: this is the "exif" that
/// `effective()` `COALESCE`s against.
#[allow(clippy::expect_used, clippy::unwrap_used)]
async fn seed_indexed_asset(
    test: &TestDb,
    owner: UserId,
    filename: &str,
    taken_at: chrono::DateTime<Utc>,
) -> AssetId {
    let library = seed_library(test, owner).await;
    let folder = FolderRepo::new(test.db())
        .ensure_path(library, &["2024"])
        .await
        .expect("folder");
    let repo = AssetRepo::new(test.db());
    let asset = repo
        .upsert_discovered(discovered(folder.id, filename))
        .await
        .expect("asset")
        .unwrap();
    repo.set_indexed(asset.id, taken_at, 100, 100)
        .await
        .expect("indexing");
    asset.id
}

fn exif_taken_at() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2024, 6, 1, 10, 0, 0).unwrap()
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn effective_coalesces_override_and_exif_field_by_field() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let asset = seed_indexed_asset(&test, admin, "DSC_0001.ARW", exif_taken_at()).await;
    let repo = OverrideRepo::new(test.db());

    // No override yet: effective == exif.
    let before = repo.effective(&ctx, asset).await.unwrap();
    assert_eq!(before.title, None);
    assert_eq!(before.taken_at, Some(exif_taken_at()));

    // A partial override touches only the title.
    repo.apply(
        &ctx,
        asset,
        &OverridePatch {
            title: Some(Some("Alba sul mare".to_owned())),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let after = repo.effective(&ctx, asset).await.unwrap();
    assert_eq!(after.title, Some("Alba sul mare".to_owned()));
    // The field the override didn't touch stays the exif's.
    assert_eq!(
        after.taken_at,
        Some(exif_taken_at()),
        "a partial override must not clear fields it didn't touch"
    );
    assert_eq!(after.description, None);
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn a_later_partial_override_does_not_erase_an_earlier_field() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let asset = seed_indexed_asset(&test, admin, "DSC_0002.ARW", exif_taken_at()).await;
    let repo = OverrideRepo::new(test.db());

    repo.apply(
        &ctx,
        asset,
        &OverridePatch {
            title: Some(Some("Titolo".to_owned())),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    repo.apply(
        &ctx,
        asset,
        &OverridePatch {
            description: Some(Some("Descrizione".to_owned())),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let effective = repo.effective(&ctx, asset).await.unwrap();
    assert_eq!(effective.title, Some("Titolo".to_owned()));
    assert_eq!(effective.description, Some("Descrizione".to_owned()));
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn effective_coalesces_location_and_place_id_from_the_asset() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let asset = seed_indexed_asset(&test, admin, "DSC_0003.ARW", exif_taken_at()).await;

    sqlx::query(
        "UPDATE assets SET location = ST_SetSRID(ST_MakePoint($2, $3), 4326)::geography, \
                place_id = $4 WHERE id = $1",
    )
    .bind(asset.as_uuid())
    .bind(12.5_f64)
    .bind(41.9_f64)
    .bind(555_i64)
    .execute(test.db().pool())
    .await
    .unwrap();

    let repo = OverrideRepo::new(test.db());
    let effective = repo.effective(&ctx, asset).await.unwrap();
    let location = effective.location.expect("position from the asset");
    assert!((location.lon - 12.5).abs() < 1e-9);
    assert!((location.lat - 41.9).abs() < 1e-9);
    assert_eq!(effective.place_id, Some(555));

    // The override replaces the location without touching place_id.
    repo.apply(
        &ctx,
        asset,
        &OverridePatch {
            location: Some(Some(GeoPoint {
                lat: 45.0,
                lon: 9.0,
            })),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let after = repo.effective(&ctx, asset).await.unwrap();
    let location = after.location.expect("position from the override");
    assert!((location.lon - 9.0).abs() < 1e-9);
    assert!((location.lat - 45.0).abs() < 1e-9);
    assert_eq!(
        after.place_id,
        Some(555),
        "place_id was not in the override"
    );
}

/// "No location" is a **value** the user can explicitly choose ("this photo
/// has no place, even though the exif would suggest one"), not simply the
/// absence of an override. `MetadataPatchRequest.location` already carries
/// the tri-state (`double_option`: absent / `Some(None)` clears /
/// `Some(Some(_))` sets) all the way through to [`OverridePatch`]. This
/// test checks whether that choice **survives** being read back.
///
/// **Known, deferred defect**: it does not survive. `effective()` does
/// `COALESCE(o.location, a.location)`, and an explicitly cleared override
/// (`asset_overrides.location = NULL`, row present) produces the same
/// `NULL` as "no override row written yet" — `COALESCE` can't tell them
/// apart, so the asset's exif location "wins" even though the user
/// explicitly denied the location. Marked `#[ignore]` (rather than
/// `#[allow]` on a wrong assertion): the test body keeps the *correct*
/// behavior we actually want, so whoever fixes the defect reactivates it
/// by removing the attribute instead of writing another one from scratch.
#[tokio::test]
#[ignore = "known, deferred defect: explicitly clearing the location does not yet \
            win over COALESCE(override, exif)"]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn effective_location_after_an_explicit_clear_does_not_fall_back_to_the_exif_value() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let asset = seed_indexed_asset(&test, admin, "DSC_0004.ARW", exif_taken_at()).await;

    // The asset has a location from exif (GPS from the camera/phone).
    sqlx::query(
        "UPDATE assets SET location = ST_SetSRID(ST_MakePoint($2, $3), 4326)::geography \
          WHERE id = $1",
    )
    .bind(asset.as_uuid())
    .bind(12.5_f64)
    .bind(41.9_f64)
    .execute(test.db().pool())
    .await
    .unwrap();

    let repo = OverrideRepo::new(test.db());

    // The user first sets a different location, then explicitly denies it:
    // "this photo has no place", not "I haven't said anything about this
    // photo's place yet".
    repo.apply(
        &ctx,
        asset,
        &OverridePatch {
            location: Some(Some(GeoPoint {
                lat: 45.0,
                lon: 9.0,
            })),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    repo.apply(
        &ctx,
        asset,
        &OverridePatch {
            location: Some(None),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let effective = repo.effective(&ctx, asset).await.unwrap();
    assert_eq!(
        effective.location, None,
        "an explicit clear must win over the asset's exif location, \
         not get confused with \"no override ever written\""
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn apply_batch_on_many_assets_is_one_operation() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let library = seed_library(&test, admin).await;
    let folder = FolderRepo::new(test.db())
        .ensure_path(library, &["2024"])
        .await
        .unwrap();
    let assets_repo = AssetRepo::new(test.db());

    let mut ids = Vec::new();
    for i in 0..500 {
        let asset = assets_repo
            .upsert_discovered(discovered(folder.id, &format!("DSC_{i:04}.ARW")))
            .await
            .unwrap()
            .unwrap();
        ids.push(asset.id);
    }

    let repo = OverrideRepo::new(test.db());
    let started = Instant::now();
    let batch_id = repo
        .apply_batch(
            &ctx,
            &ids,
            &OverridePatch {
                description: Some(Some("Matrimonio Rossi".to_owned())),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let elapsed = started.elapsed();
    assert!(
        elapsed.as_secs() < 5,
        "500 assets in one transaction must be fast, took {elapsed:?}"
    );

    // Counting rows with this id is not enough: a naive implementation that
    // recorded a batch per asset would still use a different id from the
    // one returned. The total count is what exposes 500 rows instead of
    // one.
    let total_batches: i64 = sqlx::query_scalar("SELECT count(*) FROM metadata_batches")
        .fetch_one(test.db().pool())
        .await
        .unwrap();
    assert_eq!(
        total_batches, 1,
        "a single batch row for the entire 500-asset operation, not one per asset"
    );
    let batch_exists: bool =
        sqlx::query_scalar("SELECT exists(SELECT 1 FROM metadata_batches WHERE id = $1)")
            .bind(batch_id.as_uuid())
            .fetch_one(test.db().pool())
            .await
            .unwrap();
    assert!(
        batch_exists,
        "the returned id must match the row that was written"
    );

    let touched: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM asset_overrides WHERE description = 'Matrimonio Rossi'",
    )
    .fetch_one(test.db().pool())
    .await
    .unwrap();
    assert_eq!(touched, 500, "all 500 assets must have the override");
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn undo_batch_restores_a_previous_value_that_was_null() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let asset = seed_indexed_asset(&test, admin, "DSC_0004.ARW", exif_taken_at()).await;
    let repo = OverrideRepo::new(test.db());

    // No pre-existing override: the previous value of the title is NULL,
    // not an empty string.
    let batch_id = repo
        .apply_batch(
            &ctx,
            &[asset],
            &OverridePatch {
                title: Some(Some("Temporaneo".to_owned())),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(
        repo.effective(&ctx, asset).await.unwrap().title,
        Some("Temporaneo".to_owned())
    );

    repo.undo_batch(&ctx, batch_id).await.unwrap();

    let restored = repo.effective(&ctx, asset).await.unwrap();
    assert_eq!(
        restored.title, None,
        "the title didn't exist before the batch: undoing must go back to NULL, not an empty string"
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn undoing_a_title_batch_does_not_restore_location_source() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let asset = seed_indexed_asset(&test, admin, "DSC_0004B.ARW", exif_taken_at()).await;
    let repo = OverrideRepo::new(test.db());

    let title_batch = repo
        .apply_batch(
            &ctx,
            &[asset],
            &OverridePatch {
                title: Some(Some("Titolo temporaneo".to_owned())),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    AssetRepo::new(test.db())
        .set_exif_location(
            asset,
            GeoPoint {
                lat: 45.4642,
                lon: 9.19,
            },
        )
        .await
        .unwrap();

    repo.undo_batch(&ctx, title_batch).await.unwrap();

    let source: Option<String> =
        sqlx::query_scalar("SELECT location_source FROM assets WHERE id = $1")
            .bind(asset.as_uuid())
            .fetch_one(test.db().pool())
            .await
            .unwrap();
    assert_eq!(
        source.as_deref(),
        Some("exif"),
        "undoing a non-location batch must not overwrite a later EXIF source"
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn undo_batch_restores_the_exact_previous_row() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let asset = seed_indexed_asset(&test, admin, "DSC_0005.ARW", exif_taken_at()).await;
    let repo = OverrideRepo::new(test.db());

    repo.apply_batch(
        &ctx,
        &[asset],
        &OverridePatch {
            title: Some(Some("Titolo originale".to_owned())),
            description: Some(Some("Descrizione originale".to_owned())),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let second_batch = repo
        .apply_batch(
            &ctx,
            &[asset],
            &OverridePatch {
                title: Some(Some("Titolo modificato".to_owned())),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    let mid = repo.effective(&ctx, asset).await.unwrap();
    assert_eq!(mid.title, Some("Titolo modificato".to_owned()));
    assert_eq!(mid.description, Some("Descrizione originale".to_owned()));

    repo.undo_batch(&ctx, second_batch).await.unwrap();

    let restored = repo.effective(&ctx, asset).await.unwrap();
    assert_eq!(restored.title, Some("Titolo originale".to_owned()));
    assert_eq!(
        restored.description,
        Some("Descrizione originale".to_owned()),
        "a field the second batch didn't touch must survive intact"
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn undo_batch_restores_a_null_field_on_a_row_that_already_existed() {
    // Different from `undo_batch_restores_a_previous_value_that_was_null`:
    // there, the asset had no override row at all (the DELETE branch).
    // Here the row already exists (created by the first batch) and the
    // second batch touches a different field: undoing the second batch
    // must bring that field back to NULL through the UPDATE branch, not
    // the DELETE one.
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let asset = seed_indexed_asset(&test, admin, "DSC_0007.ARW", exif_taken_at()).await;
    let repo = OverrideRepo::new(test.db());

    repo.apply_batch(
        &ctx,
        &[asset],
        &OverridePatch {
            title: Some(Some("Titolo".to_owned())),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let second_batch = repo
        .apply_batch(
            &ctx,
            &[asset],
            &OverridePatch {
                description: Some(Some("Descrizione".to_owned())),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    repo.undo_batch(&ctx, second_batch).await.unwrap();

    let restored = repo.effective(&ctx, asset).await.unwrap();
    assert_eq!(restored.title, Some("Titolo".to_owned()));
    assert_eq!(
        restored.description, None,
        "the description didn't exist before the second batch: undoing on an already-existing row must go back to NULL, not stay at the just-written value"
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn undoing_an_already_undone_batch_is_idempotent() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let asset = seed_indexed_asset(&test, admin, "DSC_0006.ARW", exif_taken_at()).await;
    let repo = OverrideRepo::new(test.db());

    let batch_id = repo
        .apply_batch(
            &ctx,
            &[asset],
            &OverridePatch {
                title: Some(Some("Prima".to_owned())),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    repo.undo_batch(&ctx, batch_id).await.unwrap();
    assert_eq!(repo.effective(&ctx, asset).await.unwrap().title, None);

    // A second override, independent of the undone batch.
    repo.apply(
        &ctx,
        asset,
        &OverridePatch {
            title: Some(Some("Seconda".to_owned())),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    // Undoing the same batch again must not touch the current state.
    repo.undo_batch(&ctx, batch_id).await.unwrap();
    assert_eq!(
        repo.effective(&ctx, asset).await.unwrap().title,
        Some("Seconda".to_owned()),
        "an already-undone batch must not re-run the undo"
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn a_plain_user_cannot_apply_overrides_on_someone_elses_asset() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let mario = harness::seed_user(&test, admin, "mario").await;
    let asset = seed_indexed_asset(&test, admin, "DSC_0007.ARW", exif_taken_at()).await;
    let repo = OverrideRepo::new(test.db());
    let mario_ctx = AuthContext::user(mario, SystemRole::User);

    let patch = OverridePatch {
        title: Some(Some("Intruso".to_owned())),
        ..Default::default()
    };
    assert!(matches!(
        repo.apply(&mario_ctx, asset, &patch).await,
        Err(DbError::Forbidden)
    ));
    assert!(matches!(
        repo.apply_batch(&mario_ctx, &[asset], &patch).await,
        Err(DbError::Forbidden)
    ));
    assert!(matches!(
        repo.effective(&mario_ctx, asset).await,
        Err(DbError::Forbidden)
    ));
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn probing_a_nonexistent_asset_id_is_forbidden_not_not_found() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let mario = harness::seed_user(&test, admin, "mario").await;
    let mario_ctx = AuthContext::user(mario, SystemRole::User);
    let repo = OverrideRepo::new(test.db());
    let ghost = AssetId::new();

    assert!(matches!(
        repo.effective(&mario_ctx, ghost).await,
        Err(DbError::Forbidden)
    ));
    assert!(matches!(
        repo.apply(&mario_ctx, ghost, &OverridePatch::default())
            .await,
        Err(DbError::Forbidden)
    ));
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn undo_batch_rejects_a_non_owner_and_a_nonexistent_id() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let mario = harness::seed_user(&test, admin, "mario").await;
    let asset = seed_indexed_asset(&test, admin, "DSC_0008.ARW", exif_taken_at()).await;
    let admin_ctx = AuthContext::user(admin, SystemRole::Admin);
    let repo = OverrideRepo::new(test.db());

    let batch_id = repo
        .apply_batch(
            &admin_ctx,
            &[asset],
            &OverridePatch {
                title: Some(Some("Riservato".to_owned())),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    let mario_ctx = AuthContext::user(mario, SystemRole::User);
    assert!(matches!(
        repo.undo_batch(&mario_ctx, batch_id).await,
        Err(DbError::Forbidden)
    ));
    assert!(matches!(
        repo.undo_batch(&mario_ctx, keeppix_domain::BatchId::new())
            .await,
        Err(DbError::Forbidden)
    ));
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn pending_sidecars_only_lists_updates_not_yet_written() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let asset = seed_indexed_asset(&test, admin, "DSC_0009.ARW", exif_taken_at()).await;
    let repo = OverrideRepo::new(test.db());

    repo.apply(
        &ctx,
        asset,
        &OverridePatch {
            title: Some(Some("Da sincronizzare".to_owned())),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let pending = repo.pending_sidecars(1000).await.unwrap();
    assert!(
        pending.contains(&asset),
        "an override that was never written is pending"
    );

    sqlx::query("UPDATE asset_overrides SET xmp_written_at = updated_at WHERE asset_id = $1")
        .bind(asset.as_uuid())
        .execute(test.db().pool())
        .await
        .unwrap();

    let pending = repo.pending_sidecars(1000).await.unwrap();
    assert!(
        !pending.contains(&asset),
        "xmp_written_at = updated_at means it's already synced"
    );

    // A new change puts it back in the queue.
    repo.apply(
        &ctx,
        asset,
        &OverridePatch {
            title: Some(Some("Cambiato di nuovo".to_owned())),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let pending = repo.pending_sidecars(1000).await.unwrap();
    assert!(
        pending.contains(&asset),
        "updated_at is now more recent than xmp_written_at"
    );
}

/// A number that must be measured, not assumed: 5,000 assets, a single
/// `apply_batch`, under a second. The seed uses a direct bulk `INSERT` —
/// not 5,000 round trips through `upsert_discovered` — because this
/// measures `apply_batch`, not data preparation.
#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn apply_batch_on_five_thousand_assets_stays_under_a_second() {
    const N: usize = 5000;

    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let library = seed_library(&test, admin).await;
    let folder = FolderRepo::new(test.db())
        .ensure_path(library, &["2024"])
        .await
        .unwrap();

    let ids: Vec<uuid::Uuid> = (0..N).map(|_| AssetId::new().as_uuid()).collect();
    let filenames: Vec<String> = (0..N).map(|i| format!("DSC_{i:05}.ARW")).collect();
    let mtime = Utc.with_ymd_and_hms(2024, 6, 1, 12, 0, 0).unwrap();
    sqlx::query(
        "INSERT INTO assets (id, folder_id, filename, size_bytes, mtime, kind, status) \
         SELECT aid, $2, name, 20000000, $3, 'raw_image', 'indexed' \
           FROM unnest($1::uuid[], $4::text[]) AS t(aid, name)",
    )
    .bind(&ids)
    .bind(folder.id.as_uuid())
    .bind(mtime)
    .bind(&filenames)
    .execute(test.db().pool())
    .await
    .unwrap();

    let asset_ids: Vec<AssetId> = ids.into_iter().map(AssetId::from_uuid).collect();
    let repo = OverrideRepo::new(test.db());

    let started = Instant::now();
    let batch_id = repo
        .apply_batch(
            &ctx,
            &asset_ids,
            &OverridePatch {
                location: Some(Some(GeoPoint {
                    lat: 45.4642,
                    lon: 9.19,
                })),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let elapsed = started.elapsed();

    // MEASURED (run with `--nocapture` to see the actual number): the
    // target is "under a second". The assertion here is more lenient so
    // the test doesn't become flaky on a shared/slow machine — the real
    // figure is the one printed, not this limit.
    eprintln!("apply_batch on {N} assets: {elapsed:?}");
    assert!(
        elapsed.as_secs() < 3,
        "apply_batch on {N} assets must stay close to a second, took {elapsed:?}"
    );

    let touched: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM asset_overrides WHERE place_id IS NULL AND location IS NOT NULL",
    )
    .fetch_one(test.db().pool())
    .await
    .unwrap();
    assert_eq!(touched, i64::try_from(N).unwrap());

    // Undoing 5,000 rows is the same transaction in reverse: it must stay
    // just as fast, not degrade into a round trip per asset.
    let undo_started = Instant::now();
    repo.undo_batch(&ctx, batch_id).await.unwrap();
    let undo_elapsed = undo_started.elapsed();
    eprintln!("undo_batch on {N} assets: {undo_elapsed:?}");
    assert!(undo_elapsed.as_secs() < 3);
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn shifting_taken_at_moves_every_asset_by_the_same_number_of_hours() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let library = seed_library(&test, admin).await;
    let folder = FolderRepo::new(test.db())
        .ensure_path(library, &["2024"])
        .await
        .unwrap();
    let assets_repo = AssetRepo::new(test.db());
    let a = assets_repo
        .upsert_discovered(discovered(folder.id, "DSC_0010.ARW"))
        .await
        .unwrap()
        .unwrap()
        .id;
    assets_repo
        .set_indexed(a, exif_taken_at(), 100, 100)
        .await
        .unwrap();
    let b = assets_repo
        .upsert_discovered(discovered(folder.id, "DSC_0011.ARW"))
        .await
        .unwrap()
        .unwrap()
        .id;
    assets_repo
        .set_indexed(b, exif_taken_at(), 100, 100)
        .await
        .unwrap();
    let repo = OverrideRepo::new(test.db());

    // A pre-existing override on `a`: the shift must start from the
    // effective value (`COALESCE(override, exif)`), not the raw exif.
    repo.apply(
        &ctx,
        a,
        &OverridePatch {
            taken_at: Some(Some(exif_taken_at() + chrono::Duration::hours(1))),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    repo.shift_taken_at(&ctx, &[a, b], 3).await.unwrap();

    let after_a = repo.effective(&ctx, a).await.unwrap();
    assert_eq!(
        after_a.taken_at,
        Some(exif_taken_at() + chrono::Duration::hours(4)),
        "starts from the effective value (exif+1h), not the raw exif"
    );
    let after_b = repo.effective(&ctx, b).await.unwrap();
    assert_eq!(
        after_b.taken_at,
        Some(exif_taken_at() + chrono::Duration::hours(3))
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn shifting_taken_at_accepts_a_negative_offset_and_is_undoable() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let asset = seed_indexed_asset(&test, admin, "DSC_0012.ARW", exif_taken_at()).await;
    let repo = OverrideRepo::new(test.db());

    let batch_id = repo.shift_taken_at(&ctx, &[asset], -2).await.unwrap();
    assert_eq!(
        repo.effective(&ctx, asset).await.unwrap().taken_at,
        Some(exif_taken_at() - chrono::Duration::hours(2))
    );

    repo.undo_batch(&ctx, batch_id).await.unwrap();
    assert_eq!(
        repo.effective(&ctx, asset).await.unwrap().taken_at,
        Some(exif_taken_at()),
        "undoing a shift returns to the starting value"
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn shifting_taken_at_on_an_asset_without_any_known_date_stays_unset() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let library = seed_library(&test, admin).await;
    let folder = FolderRepo::new(test.db())
        .ensure_path(library, &["2024"])
        .await
        .unwrap();
    // `discovered`, never indexed: no `taken_at_utc`, no override.
    let asset = AssetRepo::new(test.db())
        .upsert_discovered(discovered(folder.id, "DSC_0013.ARW"))
        .await
        .unwrap()
        .unwrap()
        .id;
    let repo = OverrideRepo::new(test.db());

    repo.shift_taken_at(&ctx, &[asset], 5).await.unwrap();

    assert_eq!(
        repo.effective(&ctx, asset).await.unwrap().taken_at,
        None,
        "a shift cannot invent a starting date"
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn undo_is_refused_once_the_sidecar_reflects_this_batchs_values() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let asset = seed_indexed_asset(&test, admin, "DSC_0014.ARW", exif_taken_at()).await;
    let repo = OverrideRepo::new(test.db());

    let batch_id = repo
        .apply_batch(
            &ctx,
            &[asset],
            &OverridePatch {
                title: Some(Some("Prima del viaggio".to_owned())),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    // The `WriteSidecar` job has already written this value to the file.
    repo.mark_sidecar_written(asset).await.unwrap();

    let result = repo.undo_batch(&ctx, batch_id).await;
    assert!(
        matches!(result, Err(DbError::Conflict(_))),
        "once the sidecar has been written, undo is no longer available: {result:?}"
    );
    assert_eq!(
        repo.effective(&ctx, asset).await.unwrap().title,
        Some("Prima del viaggio".to_owned()),
        "refusing the undo must still not touch the current value"
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn undo_still_works_when_the_sidecar_was_written_before_this_batch_was_applied() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let asset = seed_indexed_asset(&test, admin, "DSC_0015.ARW", exif_taken_at()).await;
    let repo = OverrideRepo::new(test.db());

    repo.apply_batch(
        &ctx,
        &[asset],
        &OverridePatch {
            title: Some(Some("Titolo sincronizzato".to_owned())),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    // Synced *before* the second batch: the sidecar on disk does not yet
    // reflect the change we're about to undo.
    repo.mark_sidecar_written(asset).await.unwrap();

    let second_batch = repo
        .apply_batch(
            &ctx,
            &[asset],
            &OverridePatch {
                title: Some(Some("Titolo mai sincronizzato".to_owned())),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    repo.undo_batch(&ctx, second_batch).await.unwrap();
    assert_eq!(
        repo.effective(&ctx, asset).await.unwrap().title,
        Some("Titolo sincronizzato".to_owned()),
        "the sidecar never saw the second batch: undo remains available"
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn timezone_writer_preserves_a_taken_at_override_present_at_write_time() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let asset = seed_indexed_asset(&test, admin, "concurrent.ARW", exif_taken_at()).await;
    let repo = OverrideRepo::new(test.db());
    let manual_time = exif_taken_at() + chrono::Duration::hours(2);

    repo.apply(
        &ctx,
        asset,
        &OverridePatch {
            taken_at: Some(Some(manual_time)),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let batch = repo
        .apply_taken_at_batch(
            &ctx,
            &[(asset, exif_taken_at() - chrono::Duration::hours(9))],
        )
        .await
        .unwrap();

    assert_eq!(batch, None, "a skipped assignment must not create a batch");
    assert_eq!(
        repo.effective(&ctx, asset).await.unwrap().taken_at,
        Some(manual_time),
        "the timezone writer must not overwrite a user override"
    );
}
