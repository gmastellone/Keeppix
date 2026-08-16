mod harness;

use std::time::Instant;

use chrono::{TimeZone, Utc};
use harness::TestDb;
use keeppix_db::{AssetRepo, DbError, FolderRepo, LibraryRepo, OverrideRepo};
use keeppix_domain::{
    AssetId, AssetKind, AssetName, AuthContext, GeoPoint, LibraryId, NewAsset, NewLibrary,
    OverridePatch, SystemRole, UserId,
};

#[allow(clippy::expect_used)]
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
        .expect("libreria")
        .id
}

#[allow(clippy::expect_used)]
fn discovered(folder: keeppix_domain::FolderId, filename: &str) -> NewAsset {
    NewAsset {
        folder_id: folder,
        filename: AssetName::parse(filename).expect("nome"),
        size_bytes: 1000,
        mtime: Utc.with_ymd_and_hms(2024, 6, 1, 12, 0, 0).unwrap(),
        inode: Some(1),
        kind: AssetKind::Image,
    }
}

/// Un asset indicizzato, con `taken_at_utc` noto: è l'"exif" con cui
/// `effective()` fa `COALESCE`.
#[allow(clippy::expect_used)]
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
        .expect("cartella");
    let repo = AssetRepo::new(test.db());
    let asset = repo
        .upsert_discovered(discovered(folder.id, filename))
        .await
        .expect("asset");
    repo.set_indexed(asset.id, taken_at, 100, 100)
        .await
        .expect("indicizzazione");
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

    // Nessun override ancora: effective == exif.
    let before = repo.effective(&ctx, asset).await.unwrap();
    assert_eq!(before.title, None);
    assert_eq!(before.taken_at, Some(exif_taken_at()));

    // Un override parziale tocca solo il titolo.
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
    // Il campo non toccato dall'override resta quello dell'exif.
    assert_eq!(
        after.taken_at,
        Some(exif_taken_at()),
        "un override parziale non deve azzerare i campi non toccati"
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
    let location = effective.location.expect("posizione dall'asset");
    assert!((location.lon - 12.5).abs() < 1e-9);
    assert!((location.lat - 41.9).abs() < 1e-9);
    assert_eq!(effective.place_id, Some(555));

    // L'override sostituisce, senza toccare place_id.
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
    let location = after.location.expect("posizione dall'override");
    assert!((location.lon - 9.0).abs() < 1e-9);
    assert!((location.lat - 45.0).abs() < 1e-9);
    assert_eq!(after.place_id, Some(555), "place_id non era nell'override");
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
        "500 asset in una transazione deve essere rapido, impiegati {elapsed:?}"
    );

    // Non basta contare le righe con questo id: un'implementazione ingenua
    // che registrasse un batch per asset userebbe comunque un id diverso da
    // quello restituito. Il conteggio totale è ciò che smaschera 500 righe
    // invece di una.
    let total_batches: i64 = sqlx::query_scalar("SELECT count(*) FROM metadata_batches")
        .fetch_one(test.db().pool())
        .await
        .unwrap();
    assert_eq!(
        total_batches, 1,
        "una sola riga di batch per l'intera operazione su 500 asset, non una per asset"
    );
    let batch_exists: bool =
        sqlx::query_scalar("SELECT exists(SELECT 1 FROM metadata_batches WHERE id = $1)")
            .bind(batch_id.as_uuid())
            .fetch_one(test.db().pool())
            .await
            .unwrap();
    assert!(
        batch_exists,
        "l'id restituito deve corrispondere alla riga scritta"
    );

    let touched: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM asset_overrides WHERE description = 'Matrimonio Rossi'",
    )
    .fetch_one(test.db().pool())
    .await
    .unwrap();
    assert_eq!(
        touched, 500,
        "tutti e 500 gli asset devono avere l'override"
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn undo_batch_restores_a_previous_value_that_was_null() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let asset = seed_indexed_asset(&test, admin, "DSC_0004.ARW", exif_taken_at()).await;
    let repo = OverrideRepo::new(test.db());

    // Nessun override preesistente: il valore precedente del titolo è NULL,
    // non una stringa vuota.
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
        "il titolo non esisteva prima del batch: l'annullamento deve tornare a NULL, non a stringa vuota"
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
        "un campo non toccato dal secondo batch deve sopravvivere intatto"
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn undo_batch_restores_a_null_field_on_a_row_that_already_existed() {
    // Diverso da `undo_batch_restores_a_previous_value_that_was_null`: lì
    // l'asset non aveva alcuna riga di override (ramo DELETE). Qui la riga
    // esiste già (creata dal primo batch) e il secondo batch tocca un
    // campo diverso: l'annullamento del secondo batch deve riportare quel
    // campo a NULL passando per l'UPDATE, non per la DELETE.
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
        "la description non esisteva prima del secondo batch: l'annullamento su una riga già esistente deve tornare a NULL, non restare al valore appena scritto"
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

    // Un secondo override, indipendente dal batch annullato.
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

    // Riannullare lo stesso batch non deve toccare lo stato attuale.
    repo.undo_batch(&ctx, batch_id).await.unwrap();
    assert_eq!(
        repo.effective(&ctx, asset).await.unwrap().title,
        Some("Seconda".to_owned()),
        "un batch già annullato non deve rieseguire l'annullamento"
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
        "un override mai scritto è pendente"
    );

    sqlx::query("UPDATE asset_overrides SET xmp_written_at = updated_at WHERE asset_id = $1")
        .bind(asset.as_uuid())
        .execute(test.db().pool())
        .await
        .unwrap();

    let pending = repo.pending_sidecars(1000).await.unwrap();
    assert!(
        !pending.contains(&asset),
        "xmp_written_at = updated_at significa già sincronizzato"
    );

    // Un nuovo tocco lo rimette in coda.
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
        "updated_at è tornato più recente di xmp_written_at"
    );
}
