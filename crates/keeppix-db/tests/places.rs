mod harness;

use std::path::PathBuf;

use chrono::{TimeZone as _, Utc};
use harness::TestDb;
use keeppix_db::{AssetRepo, FolderRepo, LibraryRepo, OverrideRepo, PlaceRepo};
use keeppix_domain::{
    AssetKind, AssetName, AuthContext, GeoPoint, NewAsset, NewLibrary, OverridePatch, Place,
    SystemRole, UserId,
};

fn fixture() -> Vec<Place> {
    vec![
        place(
            2_867_714, "München", "Munich", "DE", 48.137_154, 11.576_124, 1_260_391,
        ),
        place(
            1_816_670, "北京", "Beijing", "CN", 39.907_5, 116.397_23, 11_716_620,
        ),
        place(
            2_655_602, "Zürich", "Zurich", "CH", 47.366_67, 8.55, 341_730,
        ),
        place(
            3_169_070, "Roma", "Rome", "IT", 41.891_93, 12.511_33, 2_318_895,
        ),
        place(
            3_173_433, "Firenze", "Florence", "IT", 43.779_25, 11.246_26, 349_296,
        ),
        place(2_889_624, "Köln", "Cologne", "DE", 50.933_33, 6.95, 963_395),
        place(
            3_448_439,
            "São Paulo",
            "Sao Paulo",
            "BR",
            -23.547_5,
            -46.636_11,
            12_400_232,
        ),
        place(
            3_091_943, "Kraków", "Krakow", "PL", 50.061_43, 19.936_58, 755_050,
        ),
        place(
            1_857_910, "Kyoto", "Kyoto", "JP", 35.021_07, 135.753_85, 1_459_640,
        ),
        place(
            3_167_353, "Sorrento", "Sorrento", "IT", 40.626_78, 14.377_71, 14_950,
        ),
        place(
            5_397_099,
            "Sorrento Valley",
            "Sorrento Valley",
            "US",
            32.899_77,
            -117.194_26,
            20_000,
        ),
        place(
            2_146_280, "Sydney", "Sydney", "AU", -33.867_85, 151.207_32, 4_627_345,
        ),
    ]
}

fn place(
    id: i64,
    name: &str,
    ascii_name: &str,
    country_code: &str,
    lat: f64,
    lon: f64,
    population: i32,
) -> Place {
    Place {
        id,
        name: name.to_owned(),
        ascii_name: ascii_name.to_owned(),
        country_code: Some(country_code.to_owned()),
        admin1: None,
        admin2: None,
        location: GeoPoint { lat, lon },
        population,
    }
}

#[allow(clippy::expect_used)]
async fn seed(repo: &PlaceRepo<'_>) {
    for place in fixture() {
        repo.upsert(&place).await.expect("fixture place");
    }
}

fn context(user: UserId) -> AuthContext {
    AuthContext::user(user, SystemRole::Admin)
}

#[tokio::test]
#[allow(clippy::expect_used)]
async fn nearest_returns_the_closest_fixture_place() {
    let test = TestDb::start().await;
    let repo = PlaceRepo::new(test.db());
    seed(&repo).await;

    let found = repo
        .nearest(GeoPoint {
            lat: 48.14,
            lon: 11.58,
        })
        .await
        .expect("nearest query")
        .expect("nearby fixture place");

    assert_eq!(found.id, 2_867_714);
    assert_eq!(found.name, "München");
}

#[tokio::test]
#[allow(clippy::expect_used)]
async fn search_uses_ascii_trigrams_and_preserves_the_original_name() {
    let test = TestDb::start().await;
    let repo = PlaceRepo::new(test.db());
    seed(&repo).await;
    let ctx = context(UserId::new());

    let munich = repo
        .search(&ctx, "Munch", false, 10)
        .await
        .expect("Munich search");
    assert_eq!(munich.first().expect("Munich result").name, "München");
    assert_eq!(munich.first().expect("Munich result").ascii_name, "Munich");

    let beijing = repo
        .search(&ctx, "Beijing", false, 10)
        .await
        .expect("Beijing search");
    assert_eq!(beijing.first().expect("Beijing result").name, "北京");
    assert_eq!(
        beijing.first().expect("Beijing result").ascii_name,
        "Beijing"
    );
}

#[tokio::test]
#[allow(clippy::expect_used)]
async fn upsert_updates_an_existing_geoname_id_without_duplication() {
    let test = TestDb::start().await;
    let repo = PlaceRepo::new(test.db());
    seed(&repo).await;

    let mut updated = fixture().remove(0);
    updated.name = "München updated".to_owned();
    updated.population = 1_600_000;
    repo.upsert(&updated).await.expect("update place");

    let results = repo
        .search(&context(UserId::new()), "Munich", false, 10)
        .await
        .expect("updated search");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, updated.id);
    assert_eq!(results[0].name, "München updated");
    assert_eq!(results[0].population, 1_600_000);
}

#[tokio::test]
#[allow(clippy::expect_used)]
async fn normalized_csv_seeds_an_empty_table_only_once() {
    let test = TestDb::start().await;
    let repo = PlaceRepo::new(test.db());
    let path = temporary_csv_path();
    tokio::fs::write(
        &path,
        "2867714\tMünchen\tMunich\tDE\tBavaria\tUpper Bavaria\t48.137154\t11.576124\t1260391\n\
         1816670\t北京\tBeijing\tCN\tBeijing\t\t39.9075\t116.39723\t11716620\n",
    )
    .await
    .expect("write fixture CSV");

    assert_eq!(
        repo.seed_from_csv_if_empty(&path)
            .await
            .expect("first seed"),
        2
    );

    tokio::fs::write(
        &path,
        "2867714\tChanged\tMunich\tDE\tBavaria\tUpper Bavaria\t48.137154\t11.576124\t1\n",
    )
    .await
    .expect("replace fixture CSV");
    assert_eq!(
        repo.seed_from_csv_if_empty(&path)
            .await
            .expect("second seed"),
        0
    );

    let results = repo
        .search(&context(UserId::new()), "Munich", false, 10)
        .await
        .expect("seeded search");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "München");
    let _ = tokio::fs::remove_file(path).await;
}

#[tokio::test]
#[allow(clippy::expect_used)]
async fn a_missing_normalized_csv_is_a_noop() {
    let test = TestDb::start().await;
    let repo = PlaceRepo::new(test.db());
    let path = temporary_csv_path();

    assert_eq!(
        repo.seed_from_csv_if_empty(&path)
            .await
            .expect("missing file is allowed"),
        0
    );
}

fn temporary_csv_path() -> PathBuf {
    std::env::temp_dir().join(format!("keeppix-places-{}.csv", uuid::Uuid::now_v7()))
}

#[tokio::test]
#[allow(clippy::expect_used)]
async fn reverse_uses_each_localitys_population_weighted_radius() {
    let test = TestDb::start().await;
    let repo = PlaceRepo::new(test.db());
    repo.upsert(&place(
        10,
        "Tiny village",
        "Tiny village",
        "IT",
        45.0,
        10.041,
        600,
    ))
    .await
    .expect("tiny village");
    repo.upsert(&place(
        11,
        "Large city",
        "Large city",
        "IT",
        45.18,
        10.0,
        500_000,
    ))
    .await
    .expect("large city");

    let found = repo
        .nearest(GeoPoint {
            lat: 45.0,
            lon: 10.0,
        })
        .await
        .expect("reverse query")
        .expect("large city is inside its radius");

    assert_eq!(found.id, 11);
}

#[tokio::test]
#[allow(clippy::expect_used)]
async fn reverse_falls_back_to_region_then_country_catalog_rows() {
    let test = TestDb::start().await;
    let repo = PlaceRepo::new(test.db());
    let mut region = place(20, "Campania", "Campania", "IT", 40.84, 14.25, 0);
    region.admin1 = Some("Campania".to_owned());
    repo.upsert(&region).await.expect("region");
    let country = place(21, "Italia", "Italy", "IT", 42.5, 12.5, 0);
    repo.upsert(&country).await.expect("country");

    let region_found = repo
        .nearest(GeoPoint {
            lat: 40.0,
            lon: 14.25,
        })
        .await
        .expect("region fallback")
        .expect("region row");
    assert_eq!(region_found.id, region.id);

    sqlx::query("DELETE FROM places WHERE id = $1")
        .bind(region.id)
        .execute(test.db().pool())
        .await
        .expect("remove region");
    let country_found = repo
        .nearest(GeoPoint {
            lat: 40.0,
            lon: 14.25,
        })
        .await
        .expect("country fallback")
        .expect("country row");
    assert_eq!(country_found.id, country.id);
}

#[tokio::test]
#[allow(clippy::expect_used)]
async fn reverse_returns_none_when_every_threshold_is_missed() {
    let test = TestDb::start().await;
    let repo = PlaceRepo::new(test.db());
    repo.upsert(&place(
        30, "Rome", "Rome", "IT", 41.891_93, 12.511_33, 2_318_895,
    ))
    .await
    .expect("Rome");

    let found = repo
        .nearest(GeoPoint {
            lat: -30.0,
            lon: -140.0,
        })
        .await
        .expect("ocean reverse");

    assert!(found.is_none());
}

#[tokio::test]
#[allow(clippy::expect_used, clippy::unwrap_used)]
async fn search_boosts_sorrento_near_the_callers_last_fifty_overrides() {
    let test = TestDb::start().await;
    let user = harness::seed_admin(&test).await;
    let ctx = context(user);
    let repo = PlaceRepo::new(test.db());
    let campania = place(
        40, "Sorrento", "Sorrento", "IT", 40.626_78, 14.377_71, 14_950,
    );
    let california = place(
        41,
        "Sorrento Valley",
        // The normalized names deliberately tie, so removing personalization
        // makes the larger Californian result win and this test fail.
        "Sorrento",
        "US",
        32.899_77,
        -117.194_26,
        20_000,
    );
    repo.upsert(&campania).await.expect("Campania Sorrento");
    repo.upsert(&california).await.expect("California Sorrento");
    seed_override_history(&test, user, 50).await;

    let results = repo
        .search(&ctx, "sorren", true, 10)
        .await
        .expect("personalized search");

    assert_eq!(results.first().expect("Sorrento result").id, campania.id);
}

#[tokio::test]
#[allow(clippy::expect_used)]
async fn search_without_location_history_falls_back_to_population() {
    let test = TestDb::start().await;
    let repo = PlaceRepo::new(test.db());
    repo.upsert(&place(
        50,
        "Small Springfield",
        "Springfield",
        "US",
        39.8,
        -89.6,
        10_000,
    ))
    .await
    .expect("small Springfield");
    repo.upsert(&place(
        51,
        "Large Springfield",
        "Springfield",
        "US",
        44.0,
        -123.0,
        100_000,
    ))
    .await
    .expect("large Springfield");

    let results = repo
        .search(&context(UserId::new()), "spring", true, 10)
        .await
        .expect("unpersonalized search");

    assert_eq!(results.first().expect("Springfield result").id, 51);
}

#[tokio::test]
#[allow(clippy::expect_used)]
async fn search_does_not_treat_sql_wildcards_as_match_all() {
    let test = TestDb::start().await;
    let repo = PlaceRepo::new(test.db());
    seed(&repo).await;

    let results = repo
        .search(&context(UserId::new()), "%%", false, 10)
        .await
        .expect("wildcard search");

    assert!(results.is_empty());
}

#[allow(clippy::expect_used, clippy::unwrap_used)]
async fn seed_override_history(test: &TestDb, user: UserId, count: usize) {
    let ctx = context(user);
    let library = LibraryRepo::new(test.db())
        .create(
            &ctx,
            NewLibrary {
                name: "Campania history".to_owned(),
                owner_id: user,
                root_path: PathBuf::from("/mnt/campania"),
                exclude_patterns: vec![],
            },
        )
        .await
        .expect("history library");
    let folder = FolderRepo::new(test.db())
        .ensure_path(library.id, &[])
        .await
        .expect("history folder");
    let assets = AssetRepo::new(test.db());
    let overrides = OverrideRepo::new(test.db());
    for index in 0..count {
        let asset = assets
            .upsert_discovered(NewAsset {
                folder_id: folder.id,
                filename: AssetName::parse(&format!("campania-{index:02}.jpg"))
                    .expect("asset name"),
                size_bytes: 1,
                mtime: Utc.with_ymd_and_hms(2026, 8, 1, 12, 0, 0).unwrap(),
                inode: None,
                kind: AssetKind::Image,
            })
            .await
            .expect("history asset")
            .expect("new history asset");
        overrides
            .apply(
                &ctx,
                asset.id,
                &OverridePatch {
                    location: Some(Some(GeoPoint {
                        lat: 40.75,
                        lon: 14.5,
                    })),
                    ..OverridePatch::default()
                },
            )
            .await
            .expect("history override");
    }
}
