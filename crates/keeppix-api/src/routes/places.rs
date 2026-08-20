use axum::extract::rejection::QueryRejection;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use keeppix_db::PlaceRepo;
use keeppix_domain::Place;
use serde::{Deserialize, Serialize};

use crate::{AppState, Auth, Json, Problem};

#[derive(Deserialize)]
pub struct ReverseQuery {
    lat: f64,
    lon: f64,
}

#[derive(Deserialize)]
pub struct SuggestQuery {
    q: String,
    near_user: Option<bool>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct PlaceView {
    pub id: i64,
    pub name: String,
    pub ascii_name: String,
    pub country_code: Option<String>,
    pub admin1: Option<String>,
    pub admin2: Option<String>,
    pub lat: f64,
    pub lon: f64,
    pub population: i32,
}

impl From<Place> for PlaceView {
    fn from(place: Place) -> Self {
        Self {
            id: place.id,
            name: place.name,
            ascii_name: place.ascii_name,
            country_code: place.country_code,
            admin1: place.admin1,
            admin2: place.admin2,
            lat: place.location.lat,
            lon: place.location.lon,
            population: place.population,
        }
    }
}

/// # Errors
/// `400` se le coordinate non sono WGS84; `401` se non autenticato; `404` se
/// nessuna località o riga amministrativa cade entro la propria soglia.
#[utoipa::path(
    get,
    path = "/api/v1/places/reverse",
    tag = "places",
    operation_id = "places_reverse",
    summary = "Reverse-geocode a place",
    security(("session_cookie" = [])),
    params(
        ("lat" = f64, Query, description = "Latitudine WGS84, -90..=90"),
        ("lon" = f64, Query, description = "Longitudine WGS84, -180..=180")
    ),
    responses(
        (status = 200, description = "Località o area amministrativa", body = PlaceView),
        (status = 400, description = "Coordinate non valide", body = Problem),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 404, description = "Nessuna località entro soglia", body = Problem),
        (status = 500, description = "Errore del database", body = Problem)
    )
)]
pub async fn reverse(
    State(state): State<AppState>,
    Auth(_ctx): Auth,
    query: Result<Query<ReverseQuery>, QueryRejection>,
) -> Result<Json<PlaceView>, Problem> {
    let Query(query) = query.map_err(|rejection| invalid_query(&rejection))?;
    if !query.lat.is_finite()
        || !query.lon.is_finite()
        || !(-90.0..=90.0).contains(&query.lat)
        || !(-180.0..=180.0).contains(&query.lon)
    {
        return Err(Problem::bad_request(
            "invalid-coordinates",
            "Invalid WGS84 coordinates",
        ));
    }

    let place = PlaceRepo::new(&state.db)
        .nearest(keeppix_domain::GeoPoint {
            lat: query.lat,
            lon: query.lon,
        })
        .await?
        .ok_or_else(|| {
            Problem::new(
                StatusCode::NOT_FOUND,
                "place-not-found",
                "No place found near these coordinates",
            )
        })?;
    Ok(Json(place.into()))
}

/// # Errors
/// `400` se `q` ha meno di due caratteri; `401` se non autenticato.
#[utoipa::path(
    get,
    path = "/api/v1/places/suggest",
    tag = "places",
    operation_id = "places_suggest",
    summary = "Suggest places",
    security(("session_cookie" = [])),
    params(
        ("q" = String, Query, description = "Nome, almeno due caratteri"),
        ("near_user" = Option<bool>, Query, description = "Favorisce i luoghi vicini alle ultime 50 assegnazioni")
    ),
    responses(
        (status = 200, description = "Fino a dieci località", body = [PlaceView]),
        (status = 400, description = "Query troppo corta o non valida", body = Problem),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 500, description = "Errore del database", body = Problem)
    )
)]
pub async fn suggest(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    query: Result<Query<SuggestQuery>, QueryRejection>,
) -> Result<Json<Vec<PlaceView>>, Problem> {
    let Query(query) = query.map_err(|rejection| invalid_query(&rejection))?;
    let normalized = query.q.trim();
    if normalized.chars().count() < 2 {
        return Err(Problem::bad_request(
            "place-query-too-short",
            "Place query must contain at least two characters",
        ));
    }

    let places = PlaceRepo::new(&state.db)
        .search(&ctx, normalized, query.near_user.unwrap_or(true), 10)
        .await?;
    Ok(Json(places.into_iter().map(PlaceView::from).collect()))
}

fn invalid_query(rejection: &QueryRejection) -> Problem {
    Problem::bad_request("invalid-query", "Invalid query parameters")
        .with_detail(rejection.body_text())
}
