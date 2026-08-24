//! Culling a cartelle (Fase 9 Task 2-5): la radice designata, i lotti sotto
//! di essa, e lo spostamento fisico che accompagna scelto/scartato.

use keeppix_domain::{
    Asset, AssetId, AuthContext, CullingLot, CullingRole, DiskAction, Folder, FolderId, LibraryId,
    Pick,
};

use crate::{AssetRepo, Db, DbError, FlagRepo, FolderRepo, LibraryRepo, TrashRepo};

pub struct CullingRepo<'a> {
    db: &'a Db,
}

#[derive(sqlx::FromRow)]
struct LotRow {
    id: uuid::Uuid,
    name: String,
    created_at: chrono::DateTime<chrono::Utc>,
    pending: i64,
    taken: i64,
    skipped: i64,
}

impl LotRow {
    fn into_domain(self) -> CullingLot {
        CullingLot {
            folder_id: FolderId::from_uuid(self.id),
            name: self.name,
            created_at: self.created_at,
            pending: self.pending,
            taken: self.taken,
            skipped: self.skipped,
        }
    }
}

impl<'a> CullingRepo<'a> {
    #[must_use]
    pub const fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// I lotti sotto la radice di culling della libreria, più recenti
    /// prima — vuoto se nessuna radice è ancora designata (spec §2.6: senza
    /// radice, culling si comporta esattamente come oggi, nessun
    /// comportamento nuovo forzato).
    ///
    /// Ambito **owner/admin**, non lo scope di visibilità generale delle
    /// cartelle: `LibraryRepo::find_by_id` (che risolve
    /// `culling_root_folder_id`) è già owner-o-admin per costruzione, e la
    /// spec descrive il culling come un flusso personale del proprietario
    /// (nessuna menzione di condivisione dell'area). Se in futuro servirà
    /// condividere un lotto con un editor, va deciso allora — non
    /// anticipato qui senza un requisito reale.
    ///
    /// I tre conteggi sono sottoquery indipendenti per lotto, non `JOIN` +
    /// `COUNT(DISTINCT ..)`: con tre `LEFT JOIN` (radice/`_taken`/
    /// `_skipped`) il prodotto cartesiano fra i tre insiemi di asset
    /// gonfierebbe le righe intermedie inutilmente — economico non vuol
    /// dire "una sola query a tutti i costi", vuol dire "per lotto, non per
    /// libreria" (piano, Task 3): ogni sottoquery resta un accesso indicizzato
    /// su `assets.folder_id`.
    ///
    /// # Errors
    /// Come `LibraryRepo::find_by_id`.
    pub async fn list_lots(
        &self,
        ctx: &AuthContext,
        library_id: LibraryId,
    ) -> Result<Vec<CullingLot>, DbError> {
        let library = LibraryRepo::new(self.db)
            .find_by_id(ctx, library_id)
            .await?;
        let Some(root_id) = library.culling_root_folder_id else {
            return Ok(Vec::new());
        };

        let rows: Vec<LotRow> = sqlx::query_as(
            "SELECT \
                lot.id, lot.name, lot.created_at, \
                (SELECT COUNT(*) FROM assets a \
                  WHERE a.folder_id = lot.id AND a.status = 'indexed') AS pending, \
                (SELECT COUNT(*) FROM assets a JOIN folders tf ON tf.id = a.folder_id \
                  WHERE tf.parent_id = lot.id AND tf.culling_role = 'taken' \
                    AND a.status = 'indexed') AS taken, \
                (SELECT COUNT(*) FROM assets a JOIN folders sf ON sf.id = a.folder_id \
                  WHERE sf.parent_id = lot.id AND sf.culling_role = 'skipped' \
                    AND a.status = 'indexed') AS skipped \
             FROM folders lot \
             WHERE lot.parent_id = $1 \
             ORDER BY lot.created_at DESC",
        )
        .bind(root_id.as_uuid())
        .fetch_all(self.db.pool())
        .await?;

        Ok(rows.into_iter().map(LotRow::into_domain).collect())
    }

    /// Scegliere/scartare/annullare (Task 4, spec §2.3). Il flag cambia
    /// sempre; **se e solo se** l'asset è già dentro un lotto di culling
    /// (discendente diretto della radice designata: nel lotto stesso, in
    /// avvicinamento, oppure già in `_taken`/`_skipped`) lo spostamento
    /// fisico accompagna il cambio, con la primitiva del Task 1.
    ///
    /// Permesso: **non** un cancello unico per l'intera chiamata. Fuori da
    /// un lotto il flag resta impostabile da chiunque veda l'asset — lo
    /// stesso permesso di oggi (`FlagRepo::set`), invariato dalla spec
    /// (§2.6: "fuori da un lotto... resta solo un flag, come oggi"). Dentro
    /// un lotto lo spostamento fisico passa da `AssetRepo::move_asset`, che
    /// pretende `editor` su entrambe le cartelle per conto proprio — se il
    /// chiamante non lo è, l'intera chiamata fallisce con `Forbidden` prima
    /// di toccare il flag: uno spostamento raccontato dall'interfaccia ma
    /// non avvenuto sul disco sarebbe peggio di un rifiuto.
    ///
    /// Lo spostamento fisico avviene **prima** della scrittura del flag: un
    /// fallimento (permesso, collisione di nome) lascia il flag intatto
    /// invece di mentire su dove si trova il file.
    ///
    /// # Errors
    /// Come `AssetRepo::find_by_id`, poi come `AssetRepo::move_asset` se
    /// l'asset è dentro un lotto, poi come `FlagRepo::set`.
    pub async fn set_pick(
        &self,
        ctx: &AuthContext,
        asset_id: AssetId,
        pick: Pick,
    ) -> Result<Asset, DbError> {
        let assets = AssetRepo::new(self.db);
        let folders = FolderRepo::new(self.db);
        let flags = FlagRepo::new(self.db);

        let asset = assets.find_by_id(ctx, asset_id).await?;
        let (folder, library) = folders.find_with_library(ctx, asset.folder_id).await?;

        if let Some(root_id) = library.culling_root_folder_id
            && let Some(lot) = culling_lot_of(&folders, ctx, &folder, root_id).await?
        {
            let target = match pick {
                Pick::Pick => {
                    provision_culling_child(&folders, ctx, &lot, CullingRole::Taken).await?
                }
                Pick::Reject => {
                    provision_culling_child(&folders, ctx, &lot, CullingRole::Skipped).await?
                }
                Pick::None => lot,
            };
            assets
                .move_asset(ctx, asset_id, target.id, asset.filename.clone())
                .await?;
        }

        let mut current = flags.get(ctx, asset_id).await?;
        current.pick = pick;
        flags.set(ctx, asset_id, &current).await?;

        assets.find_by_id(ctx, asset_id).await
    }

    /// "Svuota scartati" (Task 4, esposto via HTTP in Fase 11 Task 17):
    /// elimina **definitivamente** dal disco ogni asset oggi in `_skipped`
    /// dentro questo lotto. Riusa `TrashRepo::choose` con
    /// `DiskAction::Purged` invece di duplicarne la logica: stesso cancello
    /// owner/admin ("un editor non può distruggere file"), stesso ordine
    /// riga-poi-file, stesso audit in `trash_entries`. La conferma è
    /// responsabilità del chiamante (API/UI), non di questo metodo — qui non
    /// c'è nulla da confermare due volte.
    ///
    /// Riuscita **parziale**, mai un blocco totale silenzioso — stesso
    /// principio già scritto per le operazioni di massa (Fase 9/10): un
    /// asset il cui purge fallisce non impedisce agli altri di essere
    /// eliminati. Il vettore restituito porta l'esito per ciascun asset,
    /// nell'ordine stabile di `find_by_folder` (per nome file); il
    /// chiamante HTTP lo traduce in `BulkOutcome`.
    ///
    /// L'**autorizzazione** resta invece tutto-o-niente, come in
    /// `TrashRepo::batch_delete` per `Purged` (spec Fase 10 §Task 4): un
    /// chiamante che non può distruggere anche un solo asset del lotto non
    /// arriva a toccarne nessuno — niente file sparisce mentre la richiesta
    /// viene rifiutata a metà.
    ///
    /// # Errors
    /// Come `FolderRepo::find_by_id` sul lotto, o `ensure_culling_child` —
    /// per l'impossibilità di risolvere il lotto stesso — poi come
    /// `TrashRepo::assert_batch_purge_authorized` se il chiamante non è
    /// owner/admin. I fallimenti sui singoli asset durante il purge vero e
    /// proprio sono nel `Result` di ogni tupla restituita, non qui.
    pub async fn empty_skipped(
        &self,
        ctx: &AuthContext,
        lot_folder_id: FolderId,
    ) -> Result<Vec<(AssetId, Result<(), DbError>)>, DbError> {
        let folders = FolderRepo::new(self.db);
        let assets = AssetRepo::new(self.db);
        let trash = TrashRepo::new(self.db);

        let lot = folders.find_by_id(ctx, lot_folder_id).await?;
        let skipped = folders
            .ensure_culling_child(&lot, CullingRole::Skipped)
            .await?;
        let victims = assets.find_by_folder(ctx, skipped.id).await?;
        let victim_ids: Vec<AssetId> = victims.iter().map(|a| a.id).collect();
        trash
            .assert_batch_purge_authorized(ctx, &victim_ids)
            .await?;

        let mut results = Vec::with_capacity(victims.len());
        for victim in victims {
            let outcome = trash
                .choose(ctx, victim.id, DiskAction::Purged)
                .await
                .map(|_| ());
            results.push((victim.id, outcome));
        }
        Ok(results)
    }
}

/// `FolderRepo::ensure_culling_child` crea solo la riga: nessun repository
/// di `keeppix-db` tocca il filesystem per creare cartelle, tranne dove
/// l'operazione stessa è fisica (`TrashRepo`, `UploadSessionRepo`) — e
/// spostare un file in `_taken`/`_skipped` che potrebbero non essere mai
/// esistite prima d'ora è esattamente quel caso. Stesso ordine di
/// `dav::write::mkcol` (Fase 5 Task 7): directory su disco **prima** della
/// riga, mai il contrario — se l'`INSERT` fallisse dopo un `create_dir_all`
/// riuscito, resterebbe una cartella fantasma sul disco senza una riga
/// corrispondente, non il rovescio silenzioso (riga senza cartella, `rename`
/// che fallisce al prossimo tentativo). Rollback best-effort della sola
/// directory creata da questa chiamata, mai di una preesistente.
async fn provision_culling_child(
    folders: &FolderRepo<'_>,
    ctx: &AuthContext,
    lot: &Folder,
    role: CullingRole,
) -> Result<Folder, DbError> {
    let name = match role {
        CullingRole::Taken => "_taken",
        CullingRole::Skipped => "_skipped",
    };
    let target_dir = folders.absolute_path(ctx, lot.id).await?.join(name);
    let already_on_disk = tokio::fs::metadata(&target_dir).await.is_ok();
    tokio::fs::create_dir_all(&target_dir)
        .await
        .map_err(|err| DbError::Io(format!("creating {}: {err}", target_dir.display())))?;

    match folders.ensure_culling_child(lot, role).await {
        Ok(child) => Ok(child),
        Err(err) => {
            if !already_on_disk {
                let _ = tokio::fs::remove_dir(&target_dir).await;
            }
            Err(err)
        }
    }
}

/// Il lotto che contiene `folder`, se `folder` è un discendente diretto
/// della radice di culling designata — nel lotto stesso (asset in attesa),
/// oppure in una delle sue due figlie `_taken`/`_skipped`. `None` per
/// qualunque altra cartella, compresa la radice stessa: solo la struttura a
/// due livelli che il Task 2/3 costruiscono conta, mai riconosciuta per
/// nome.
async fn culling_lot_of(
    folders: &FolderRepo<'_>,
    ctx: &AuthContext,
    folder: &Folder,
    root_id: FolderId,
) -> Result<Option<Folder>, DbError> {
    if folder.parent_id == Some(root_id) {
        return Ok(Some(folder.clone()));
    }
    if folder.culling_role.is_none() {
        return Ok(None);
    }
    let Some(lot_id) = folder.parent_id else {
        return Ok(None);
    };
    let lot = folders.find_by_id(ctx, lot_id).await?;
    Ok((lot.parent_id == Some(root_id)).then_some(lot))
}
