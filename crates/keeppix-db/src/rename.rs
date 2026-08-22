//! Rinomina con formula (Fase 9 Task 8-9): risoluzione dei valori per
//! asset, i tre ambiti, la co-rinomina dei file affiancati di una pila, e
//! l'applicazione con audit per l'annullamento. Il motore vero e proprio
//! (`render_base`/`apply_base_to_filename`) è puro, in `keeppix-domain`.
//!
//! **Difetto 1 chiuso qui** (spec §62.3d, rinviato dal Task 7): la
//! collisione è verificata contro l'intero database — anteprima e
//! applicazione entrambe, non solo dentro il gruppo che si sta rinominando.
//!
//! **Ambito**: questo modulo non risolve "tutti gli asset di una cartella"
//! o "tutti quelli di un lotto" da sé — riceve sempre un `&[AssetId]` già
//! esplicito. È la correzione che il piano chiede per "Rinomina cartella…"
//! (spec, Task 8: "L'ambito va reso esplicito nella richiesta e nel testo"):
//! nessuna narrowing silenziosa dentro questa funzione, il chiamante (Task
//! 10/11) decide e dichiara l'elenco esatto prima di chiamare.
//!
//! **Rinvio dichiarato**: `resolve_place_label`'s middle candidate
//! (posizione della cartella) non è mai popolato qui — non esiste alcuna
//! colonna di posizione sulle cartelle nello schema di Keeppix oggi
//! (verificato: nessuna migrazione la introduce). Il fallback al nome del
//! lotto di culling non è wired nemmeno lui in questo commit — richiede la
//! stessa logica di `culling_lot_of` (privata in `culling.rs`), non
//! duplicata qui senza un secondo requisito reale davanti. Entrambi restano
//! `None`: la foto stessa (`assets.place_id`/`asset_overrides.place_id`) è
//! l'unica fonte oggi. Un caso stretto (una foto senza posizione propria,
//! in un lotto appena importato senza nulla di geotaggato) userebbe
//! `{luogo}` vuoto invece del nome del lotto — non un difetto silenzioso,
//! dichiarato qui.

use std::collections::{HashMap, HashSet};

use keeppix_domain::{
    Asset, AssetId, AssetName, AuthContext, BatchId, FolderId, RenameValues,
    apply_base_to_filename, render_base,
};
use uuid::Uuid;

use crate::{AssetRepo, Db, DbError, PermissionRepo};

pub struct RenameRepo<'a> {
    db: &'a Db,
}

/// Il risultato calcolato per **un file fisico** (non per pila): due membri
/// affiancati della stessa pila producono due voci con la stessa base e
/// l'estensione propria di ciascuno.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenamePreviewItem {
    pub asset_id: AssetId,
    pub folder_id: FolderId,
    pub current_name: String,
    pub new_name: String,
    /// `true` se `new_name` coincide, nella stessa cartella, con un altro
    /// asset — di questo lotto **o già presente sul disco/database fuori
    /// dal lotto** (difetto 1). Un nome invariato (`new_name ==
    /// current_name`) non conta mai come collisione con se stesso.
    pub collides: bool,
}

#[derive(Debug)]
pub struct RenameBatchOutcome {
    /// `None` se nessun asset è stato rinominato con successo — nulla da
    /// annullare, nessuna riga scritta in `rename_batches`.
    pub batch_id: Option<BatchId>,
    pub renamed: Vec<Asset>,
    pub failed: Vec<(AssetId, DbError)>,
}

#[derive(Clone)]
struct AssetRow {
    id: Uuid,
    folder_id: Uuid,
    filename: String,
    stack_id: Option<Uuid>,
}

/// Riga grezza condivisa da `load_rows`/`load_stack_members` — stessa forma
/// di [`AssetRow`], separata solo perché deve derivare `sqlx::FromRow` a
/// livello di modulo (clippy `items_after_statements` vieta di dichiararla
/// dentro le funzioni che la usano).
#[derive(sqlx::FromRow)]
struct AssetLookupRow {
    id: Uuid,
    folder_id: Uuid,
    filename: String,
    stack_id: Option<Uuid>,
}

impl From<AssetLookupRow> for AssetRow {
    fn from(r: AssetLookupRow) -> Self {
        Self {
            id: r.id,
            folder_id: r.folder_id,
            filename: r.filename,
            stack_id: r.stack_id,
        }
    }
}

#[derive(sqlx::FromRow)]
struct ValuesRow {
    id: Uuid,
    taken_at: Option<chrono::DateTime<chrono::Utc>>,
    title: Option<String>,
    camera_model: Option<String>,
    lens: Option<String>,
    place_name: Option<String>,
}

impl<'a> RenameRepo<'a> {
    #[must_use]
    pub const fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// Calcola i nomi risultanti senza scrivere nulla — dischi e database
    /// intatti. `asset_ids` è già l'ambito esplicito (Task 8): questa
    /// funzione non lo allarga né lo restringe, solo espande automaticamente
    /// ogni pila (RAW+JPEG) i cui membri non fossero già tutti presenti,
    /// perché si rinominano sempre insieme.
    ///
    /// # Errors
    /// `Forbidden` se anche un solo asset non è visibile o non modificabile
    /// dal chiamante.
    pub async fn preview(
        &self,
        ctx: &AuthContext,
        asset_ids: &[AssetId],
        schema: &str,
    ) -> Result<Vec<RenamePreviewItem>, DbError> {
        let items = self.compute(ctx, asset_ids, schema).await?;
        Ok(items)
    }

    /// Come [`Self::preview`], poi rinomina per davvero via
    /// [`crate::AssetRepo::move_asset`]. Il permesso resta un cancello
    /// **unico per l'intera chiamata** (come `OverrideRepo::apply_batch`,
    /// non la sua variante `_partial`): se anche un solo asset dell'ambito
    /// non è modificabile, `compute` rifiuta tutto prima di tentare il
    /// primo `move_asset` — non ha senso rinominare metà di un gruppo
    /// scelto dall'utente per un problema di permesso su un altro membro.
    /// Le **collisioni**, invece, sono per natura note solo al momento
    /// della scrittura (una corsa fra due chiamate concorrenti, o due voci
    /// del gruppo destinate allo stesso nome): quelle finiscono in
    /// `failed` senza bloccare il resto — riuscita parziale sul dato, non
    /// sul permesso. Registra un `rename_batches` **solo** per gli asset
    /// rinominati con successo — un fallimento parziale non lascia
    /// annullabile ciò che non è mai cambiato.
    ///
    /// # Errors
    /// `Forbidden` se il chiamante non è autenticato. Gli errori per
    /// singolo asset finiscono in `failed`, non propagati.
    pub async fn apply(
        &self,
        ctx: &AuthContext,
        asset_ids: &[AssetId],
        schema: &str,
    ) -> Result<RenameBatchOutcome, DbError> {
        let Some(actor) = ctx.user_id() else {
            return Err(DbError::Forbidden);
        };
        let items = self.compute(ctx, asset_ids, schema).await?;

        let assets = AssetRepo::new(self.db);
        let mut renamed = Vec::new();
        let mut failed = Vec::new();
        let mut previous = serde_json::Map::new();

        for item in items {
            if item.current_name == item.new_name {
                continue;
            }
            let new_filename = match AssetName::parse(&item.new_name) {
                Ok(name) => name,
                Err(err) => {
                    failed.push((
                        item.asset_id,
                        DbError::Conflict(format!("computed filename is invalid: {err}")),
                    ));
                    continue;
                }
            };
            match assets
                .move_asset(ctx, item.asset_id, item.folder_id, new_filename)
                .await
            {
                Ok(asset) => {
                    previous.insert(
                        item.asset_id.to_string(),
                        serde_json::json!({
                            "folder_id": item.folder_id.as_uuid(),
                            "filename": item.current_name,
                        }),
                    );
                    renamed.push(asset);
                }
                Err(err) => failed.push((item.asset_id, err)),
            }
        }

        let batch_id = if renamed.is_empty() {
            None
        } else {
            let id = BatchId::new();
            sqlx::query("INSERT INTO rename_batches (id, actor_id, previous) VALUES ($1, $2, $3)")
                .bind(id.as_uuid())
                .bind(actor.as_uuid())
                .bind(serde_json::Value::Object(previous))
                .execute(self.db.pool())
                .await?;
            Some(id)
        };

        Ok(RenameBatchOutcome {
            batch_id,
            renamed,
            failed,
        })
    }

    /// Il calcolo condiviso da `preview`/`apply`: cancello di permesso,
    /// espansione delle pile, risoluzione dei valori, la base per pila, e
    /// il controllo delle collisioni (difetto 1: dentro il gruppo **e**
    /// contro il resto del database).
    async fn compute(
        &self,
        ctx: &AuthContext,
        asset_ids: &[AssetId],
        schema: &str,
    ) -> Result<Vec<RenamePreviewItem>, DbError> {
        if asset_ids.is_empty() {
            return Ok(Vec::new());
        }
        AssetRepo::new(self.db)
            .assert_visible(ctx, asset_ids)
            .await?;
        PermissionRepo::new(self.db)
            .assert_can_edit_assets(ctx, asset_ids)
            .await?;

        let ids: Vec<Uuid> = asset_ids.iter().map(AssetId::as_uuid).collect();
        let requested = self.load_rows(&ids).await?;
        let requested_by_id: HashMap<Uuid, &AssetRow> =
            requested.iter().map(|r| (r.id, r)).collect();

        let stack_ids: Vec<Uuid> = requested
            .iter()
            .filter_map(|r| r.stack_id)
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        let primaries = self.load_stack_primaries(&stack_ids).await?;
        let members_by_stack = self.load_stack_members(&stack_ids).await?;

        // Gruppi logici nell'ordine di `asset_ids` (spec: il contatore
        // "segue l'ordine dell'array delle foto dell'ambito"): un asset in
        // pila rappresenta l'intero gruppo alla sua prima apparizione, le
        // apparizioni successive (sue o di un fratello) sono ignorate.
        let mut seen: HashSet<Uuid> = HashSet::new();
        let mut groups: Vec<(Uuid, Vec<AssetRow>)> = Vec::new();
        for &id in &ids {
            if seen.contains(&id) {
                continue;
            }
            let Some(row) = requested_by_id.get(&id) else {
                continue; // non visibile/trashed: assert_visible sopra lo avrebbe già rifiutato per un utente non-admin.
            };
            if let Some(stack_id) = row.stack_id {
                let members = members_by_stack.get(&stack_id).cloned().unwrap_or_default();
                for m in &members {
                    seen.insert(m.id);
                }
                let representative = primaries.get(&stack_id).copied().unwrap_or(id);
                groups.push((representative, members));
            } else {
                seen.insert(id);
                groups.push((id, vec![(*row).clone()]));
            }
        }

        let representative_ids: Vec<Uuid> = groups.iter().map(|(rep, _)| *rep).collect();
        let values_by_id = self.load_values(&representative_ids).await?;

        let mut items = Vec::new();
        for (index, (representative, members)) in groups.into_iter().enumerate() {
            let values = values_by_id
                .get(&representative)
                .cloned()
                .unwrap_or_default();
            let base = render_base(schema, &values, index + 1);
            for member in members {
                let new_name = apply_base_to_filename(&base, &member.filename);
                items.push(RenamePreviewItem {
                    asset_id: AssetId::from_uuid(member.id),
                    folder_id: FolderId::from_uuid(member.folder_id),
                    current_name: member.filename.clone(),
                    new_name,
                    collides: false,
                });
            }
        }

        self.flag_collisions(&ids, &mut items).await?;
        Ok(items)
    }

    async fn load_rows(&self, ids: &[Uuid]) -> Result<Vec<AssetRow>, DbError> {
        let rows: Vec<AssetLookupRow> = sqlx::query_as(
            "SELECT id, folder_id, filename, stack_id FROM assets \
              WHERE id = ANY($1) AND status <> 'trashed'",
        )
        .bind(ids)
        .fetch_all(self.db.pool())
        .await?;
        Ok(rows.into_iter().map(AssetRow::from).collect())
    }

    async fn load_stack_primaries(
        &self,
        stack_ids: &[Uuid],
    ) -> Result<HashMap<Uuid, Uuid>, DbError> {
        if stack_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let rows: Vec<(Uuid, Uuid)> =
            sqlx::query_as("SELECT id, primary_asset_id FROM stacks WHERE id = ANY($1)")
                .bind(stack_ids)
                .fetch_all(self.db.pool())
                .await?;
        Ok(rows.into_iter().collect())
    }

    async fn load_stack_members(
        &self,
        stack_ids: &[Uuid],
    ) -> Result<HashMap<Uuid, Vec<AssetRow>>, DbError> {
        if stack_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let rows: Vec<AssetLookupRow> = sqlx::query_as(
            "SELECT id, folder_id, filename, stack_id FROM assets \
              WHERE stack_id = ANY($1) AND status <> 'trashed'",
        )
        .bind(stack_ids)
        .fetch_all(self.db.pool())
        .await?;
        let mut by_stack: HashMap<Uuid, Vec<AssetRow>> = HashMap::new();
        for r in rows {
            let Some(stack_id) = r.stack_id else {
                continue;
            };
            by_stack.entry(stack_id).or_default().push(r.into());
        }
        Ok(by_stack)
    }

    /// I valori risolti (data/fotocamera/obiettivo/luogo/titolo) per ogni
    /// asset rappresentante di un gruppo — mai per i membri non
    /// rappresentanti, che ereditano la base del loro rappresentante.
    async fn load_values(&self, ids: &[Uuid]) -> Result<HashMap<Uuid, RenameValues>, DbError> {
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        let rows: Vec<ValuesRow> = sqlx::query_as(
            "SELECT a.id, \
                    COALESCE(o.taken_at, a.taken_at_utc) AS taken_at, \
                    o.title, \
                    e.camera_model, e.lens, \
                    p.name AS place_name \
             FROM assets a \
             LEFT JOIN asset_overrides o ON o.asset_id = a.id \
             LEFT JOIN asset_exif e ON e.asset_id = a.id \
             LEFT JOIN places p ON p.id = COALESCE(o.place_id, a.place_id) \
             WHERE a.id = ANY($1)",
        )
        .bind(ids)
        .fetch_all(self.db.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| {
                let values = RenameValues {
                    date: r.taken_at.map(|ts| ts.date_naive()),
                    camera: r.camera_model,
                    lens: r.lens,
                    place: r.place_name,
                    title: r.title,
                };
                (r.id, values)
            })
            .collect())
    }

    /// Marca `collides` su ogni voce: dentro il gruppo (due membri di
    /// questa stessa chiamata puntano allo stesso nome nella stessa
    /// cartella) **e** contro il resto del database — asset non compresi
    /// in questa chiamata che già occupano quel nome (difetto 1, spec
    /// §62.3d, chiuso qui invece che nel Task 7 perché serve l'elenco degli
    /// asset coinvolti, che questo modulo è il primo ad avere).
    async fn flag_collisions(
        &self,
        batch_ids: &[Uuid],
        items: &mut [RenamePreviewItem],
    ) -> Result<(), DbError> {
        let mut within_group: HashMap<(Uuid, String), usize> = HashMap::new();
        for item in items.iter() {
            *within_group
                .entry((item.folder_id.as_uuid(), item.new_name.clone()))
                .or_insert(0) += 1;
        }

        let folder_ids: Vec<Uuid> = items
            .iter()
            .map(|i| i.folder_id.as_uuid())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        let existing: HashSet<(Uuid, String)> = if folder_ids.is_empty() {
            HashSet::new()
        } else {
            let rows: Vec<(Uuid, String)> = sqlx::query_as(
                "SELECT folder_id, filename FROM assets \
                  WHERE folder_id = ANY($1) AND status <> 'trashed' AND NOT (id = ANY($2))",
            )
            .bind(&folder_ids)
            .bind(batch_ids)
            .fetch_all(self.db.pool())
            .await?;
            rows.into_iter().collect()
        };

        for item in items.iter_mut() {
            let key = (item.folder_id.as_uuid(), item.new_name.clone());
            let no_op = item.new_name == item.current_name;
            let duplicated_in_group = within_group.get(&key).copied().unwrap_or(0) > 1;
            let occupied_outside_group = !no_op && existing.contains(&key);
            item.collides = duplicated_in_group || occupied_outside_group;
        }
        Ok(())
    }
}
