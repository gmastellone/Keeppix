//! Superficie HTTP del culling a cartelle (Fase 9 Task 2-5, esposta qui in
//! Fase 11 Task 17). Nessun controllo di permesso proprio: ogni handler
//! propaga [`keeppix_db::DbError`] da [`CullingRepo`], che incorpora già il
//! cancello giusto per ciascuna operazione — owner/admin via
//! `LibraryRepo::find_by_id` per la lettura dei lotti, `editor` su entrambe
//! le cartelle via `AssetRepo::move_asset` per lo spostamento fisico,
//! owner/admin via `TrashRepo::assert_batch_purge_authorized` per lo
//! svuotamento. Reinventare un secondo controllo qui rischierebbe di
//! raccontarne uno diverso da quello che poi decide davvero.

use axum::extract::{Path, State};
use keeppix_db::CullingRepo;
use keeppix_domain::{AssetId, FolderId, LibraryId, Pick};
use serde::{Deserialize, Serialize};

use crate::bulk::BulkOutcome;
use crate::extract::Auth;
use crate::json::Json;
use crate::problem::Problem;
use crate::routes::timeline::AssetView;
use crate::state::AppState;

#[derive(Serialize, utoipa::ToSchema)]
pub struct CullingLotView {
    pub folder_id: String,
    pub name: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub pending: i64,
    pub taken: i64,
    pub skipped: i64,
}

impl CullingLotView {
    fn from_lot(lot: &keeppix_domain::CullingLot) -> Self {
        Self {
            folder_id: lot.folder_id.to_string(),
            name: lot.name.clone(),
            created_at: lot.created_at,
            pending: lot.pending,
            taken: lot.taken,
            skipped: lot.skipped,
        }
    }
}

/// I lotti sotto la radice di culling della libreria (§14). Vuoto — non
/// un errore — se la libreria non ha ancora una radice designata.
///
/// # Errors
/// `401` se non autenticato; `403` se il chiamante non vede la libreria, o
/// la vede ma non ne è proprietario/admin.
#[utoipa::path(
    get,
    path = "/api/v1/libraries/{id}/culling/lots",
    tag = "culling",
    operation_id = "culling_list_lots",
    summary = "List culling lots for a library",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Id della libreria")),
    responses(
        (status = 200, description = "Lotti, più recenti prima", body = [CullingLotView]),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Non owner/admin di questa libreria", body = Problem)
    )
)]
pub async fn list_lots(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Path(id): Path<LibraryId>,
) -> Result<Json<Vec<CullingLotView>>, Problem> {
    let lots = CullingRepo::new(&state.db).list_lots(&ctx, id).await?;
    Ok(Json(lots.iter().map(CullingLotView::from_lot).collect()))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct PickRequest {
    #[schema(value_type = String, example = "pick")]
    pub pick: Pick,
}

/// Scegliere/scartare/annullare un asset (§15). Fuori da un lotto di
/// culling resta solo un voto, come `PUT /assets/{id}/flags`; dentro un
/// lotto lo spostamento fisico in `_taken`/`_skipped` accompagna il voto
/// nella stessa chiamata ([`CullingRepo::set_pick`]). Rotta dedicata invece
/// di estendere `PUT /assets/{id}/flags`: quella rotta è già il percorso
/// caldo del voto ordinario (nessun movimento fisico, nessun ambito di
/// permesso variabile) — mescolarci uno spostamento condizionale l'avrebbe
/// resa più difficile da ragionare per entrambi i casi.
///
/// # Errors
/// `401` se non autenticato; `403` se l'asset non è visibile, o se è dentro
/// un lotto e il chiamante non è editor sia della cartella di origine sia di
/// quella di destinazione; `409` su collisione di nome nella cartella di
/// destinazione.
#[utoipa::path(
    post,
    path = "/api/v1/assets/{id}/pick",
    tag = "culling",
    operation_id = "culling_set_pick",
    summary = "Pick, reject, or clear an asset's culling vote",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Id dell'asset")),
    request_body = PickRequest,
    responses(
        (status = 200, description = "Asset aggiornato (folder_id nuovo se si è mosso)", body = AssetView),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Asset non visibile, o permesso insufficiente per lo spostamento", body = Problem),
        (status = 409, description = "Collisione di nome nella cartella di destinazione", body = Problem)
    )
)]
pub async fn pick(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Path(id): Path<AssetId>,
    Json(body): Json<PickRequest>,
) -> Result<Json<AssetView>, Problem> {
    let asset = CullingRepo::new(&state.db)
        .set_pick(&ctx, id, body.pick)
        .await?;
    Ok(Json(AssetView::from_asset(&asset)))
}

/// "Svuota scartati" (§15): elimina definitivamente dal disco ogni asset
/// oggi in `_skipped` per questo lotto. Riuscita **parziale**: un asset che
/// non si riesce a purgare non impedisce agli altri — vedi
/// [`CullingRepo::empty_skipped`]. L'autorizzazione resta invece
/// tutto-o-niente, verificata prima di toccare qualunque file.
///
/// # Errors
/// `401` se non autenticato; `403` se il chiamante non può distruggere
/// anche un solo asset del lotto (owner/admin richiesti).
#[utoipa::path(
    post,
    path = "/api/v1/culling/lots/{id}/empty-skipped",
    tag = "culling",
    operation_id = "culling_empty_skipped",
    summary = "Permanently delete every asset in a lot's _skipped folder",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Id della cartella del lotto")),
    responses(
        (status = 200, description = "Esito per asset (riuscita parziale ammessa)", body = BulkOutcome),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Non owner/admin di almeno un asset del lotto", body = Problem)
    )
)]
pub async fn empty_skipped(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Path(id): Path<FolderId>,
) -> Result<Json<BulkOutcome>, Problem> {
    let results = CullingRepo::new(&state.db).empty_skipped(&ctx, id).await?;
    let mut succeeded = Vec::new();
    let mut failed = Vec::new();
    for (asset_id, outcome) in results {
        match outcome {
            Ok(()) => succeeded.push(asset_id),
            Err(e) => failed.push((asset_id, e)),
        }
    }
    Ok(Json(BulkOutcome::from_partition(succeeded, &failed, None)))
}
