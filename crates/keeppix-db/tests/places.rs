mod harness;

use std::path::PathBuf;

use harness::TestDb;
use keeppix_db::PlaceRepo;
use keeppix_domain::{GeoPoint, Place};

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

    let munich = repo.search("Munch", 10).await.expect("Munich search");
    assert_eq!(munich.first().expect("Munich result").name, "München");
    assert_eq!(munich.first().expect("Munich result").ascii_name, "Munich");

    let beijing = repo.search("Beijing", 10).await.expect("Beijing search");
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

    let results = repo.search("Munich", 10).await.expect("updated search");
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

    let results = repo.search("Munich", 10).await.expect("seeded search");
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
