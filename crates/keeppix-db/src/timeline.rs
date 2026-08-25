//! Bucket mensili e pagine keyset della timeline. Nessun `OFFSET`.

use chrono::{DateTime, Months, NaiveDate, Utc};
use keeppix_domain::{AssetId, AuthContext, LibraryId};

use crate::assets::A_COLUMNS;
use crate::libraries::LibraryRepo;
use crate::stacks::{
    AssetStackRow, AssetWithStack, STACK_BADGE_COLUMNS_SQL, STACK_BADGE_JOIN_SQL,
    STACK_PRIMARY_JOIN_SQL, STACK_PRIMARY_ONLY_SQL,
};
use crate::visibility::VisibilityScope;
use crate::{Db, DbError, MapBounds};

pub struct TimelineRepo<'a> {
    db: &'a Db,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonthBucket {
    pub month: NaiveDate,
    pub count: i64,
}

/// Una riga di geometria: dimensioni note (o `None` se l'asset non è ancora
/// stato dimensionato) e il momento dello scatto, nello stesso ordine della
/// timeline (`taken_at_utc DESC, id DESC`). Nessun identificativo: la
/// geometria descrive altezze, non identifica asset (spec fase-10 §2.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GeometryRecord {
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub taken_at_utc: DateTime<Utc>,
}

/// Geometria di un'intera vista della timeline, più l'informazione minima per
/// costruire un `ETag`: il massimo `updated_at` fra gli asset della vista.
/// `records.len()` è il conteggio della risposta piena.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Geometry {
    pub records: Vec<GeometryRecord>,
    pub last_modified: Option<DateTime<Utc>>,
    /// `Some` solo se questa era una richiesta paginata ([`GeometryPage`]) e
    /// potrebbe esserci altro dopo — il chiamante HTTP lo espone come cursore
    /// opaco per la pagina successiva, mai nel corpo binario: la geometria
    /// non porta identificativi per costruzione (spec fase-10 §2.3).
    pub next_cursor: Option<(DateTime<Utc>, AssetId)>,
}

/// Prima pagina "a vista intera" (senza `page`) o continuazione keyset dopo
/// un cursore — mai `OFFSET` (vedi nota in testa al file): il costo di
/// saltare N righe cresce con N, il keyset resta O(log n) sull'indice
/// esistente a prescindere da dove ci si trova nella vista.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GeometryPage {
    pub limit: i64,
    pub after: Option<(DateTime<Utc>, AssetId)>,
}

/// Tetto di `GeometryPage::limit` — un client che chiede di più viene
/// silenziosamente clampato, non rifiutato: la richiesta resta valida, dà
/// solo meno per volta. 20.000 record da 6 byte sono ~117 KB, già ben oltre
/// quanto serve a un primo disegno su rete lenta.
const GEOMETRY_PAGE_LIMIT_MAX: i64 = 20_000;

/// Riga grezza condivisa da `geometry`/`geometry_in_bounds`: id (per il
/// cursore della pagina successiva, mai nel payload binario), dimensioni,
/// istante dello scatto.
type GeometryRow = (uuid::Uuid, Option<i32>, Option<i32>, DateTime<Utc>);

/// Timbratura leggera della vista geometria (`count` + `max(updated_at)`),
/// usata per validare `If-None-Match` **prima** di scaricare tutti i record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GeometryStamp {
    pub count: u64,
    pub last_modified: Option<DateTime<Utc>>,
}

impl<'a> TimelineRepo<'a> {
    #[must_use]
    pub const fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// Conta le pile (una pila impilata conta 1, non quanti file la
    /// compongono), non più righe di `folder_month_counts` (Ruling Task 3):
    /// il trigger che alimenta quella tabella non guarda `stack_id` — farlo
    /// significherebbe insegnargli a ricalcolare il conteggio di uno stack
    /// ogni volta che cambia il primario (`StackRepo::set_primary`) o un
    /// membro si aggiunge/rimuove, molta più complessità nel trigger per un
    /// solo endpoint. Si conta direttamente da `assets` con lo stesso
    /// filtro di primario di `page`, così il numero di mesi e il numero di
    /// tessere per mese non divergono mai. La tabella `folder_month_counts`
    /// resta intatta per gli altri usi (contatori di cartella, cestino).
    ///
    /// # Errors
    /// `Forbidden` se `library_id` non è del chiamante (anche inesistente).
    /// `Connection` se la query fallisce.
    pub async fn buckets(
        &self,
        ctx: &AuthContext,
        library_id: Option<LibraryId>,
    ) -> Result<Vec<MonthBucket>, DbError> {
        if let Some(id) = library_id {
            LibraryRepo::new(self.db).find_by_id(ctx, id).await?;
        }
        let scope = VisibilityScope::resolve(self.db, ctx).await?;
        let filter = scope.filter("f.path", "f.library_id", "a.id", 1);
        let sql = format!(
            "SELECT date_trunc('month', a.taken_at_utc)::date AS month, \
                    count(*)::bigint AS count \
               FROM assets a \
               JOIN folders f ON f.id = a.folder_id \
               {STACK_PRIMARY_JOIN_SQL} \
              WHERE {} \
                AND ($4::uuid IS NULL OR f.library_id = $4) \
                AND a.status = 'indexed' \
                AND a.kind <> 'unknown' \
                AND a.taken_at_utc IS NOT NULL \
                AND {STACK_PRIMARY_ONLY_SQL} \
              GROUP BY month \
              ORDER BY month DESC",
            filter.sql()
        );
        let rows: Vec<(NaiveDate, i64)> = sqlx::query_as(&sql)
            .bind(filter.bind())
            .bind(filter.holes())
            .bind(filter.assets())
            .bind(library_id.map(|id| id.as_uuid()))
            .fetch_all(self.db.pool())
            .await?;
        Ok(rows
            .into_iter()
            .map(|(month, count)| MonthBucket { month, count })
            .collect())
    }

    /// Conteggi mensili ricalcolati sugli asset effettivamente dentro `bounds`.
    ///
    /// # Errors
    /// `Forbidden` se `library_id` non è del chiamante; `Connection` se la
    /// query fallisce.
    pub async fn buckets_in_bounds(
        &self,
        ctx: &AuthContext,
        library_id: Option<LibraryId>,
        bounds: MapBounds,
    ) -> Result<Vec<MonthBucket>, DbError> {
        if let Some(id) = library_id {
            LibraryRepo::new(self.db).find_by_id(ctx, id).await?;
        }
        let scope = VisibilityScope::resolve(self.db, ctx).await?;
        let filter = scope.filter("f.path", "f.library_id", "a.id", 1);
        let bbox = effective_bbox_filter_sql(5, 6, 7, 8);
        let sql = format!(
            "SELECT date_trunc('month', a.taken_at_utc)::date AS month, \
                    count(*)::bigint AS count \
               FROM assets a \
               JOIN folders f ON f.id = a.folder_id \
               LEFT JOIN asset_overrides o ON o.asset_id = a.id \
               {STACK_PRIMARY_JOIN_SQL} \
              WHERE {} \
                AND ($4::uuid IS NULL OR f.library_id = $4) \
                AND a.status = 'indexed' \
                AND a.kind <> 'unknown' \
                AND a.taken_at_utc IS NOT NULL \
                AND ({bbox}) \
                AND {STACK_PRIMARY_ONLY_SQL} \
              GROUP BY month \
              ORDER BY month DESC",
            filter.sql()
        );
        let rows: Vec<(NaiveDate, i64)> = sqlx::query_as(&sql)
            .bind(filter.bind())
            .bind(filter.holes())
            .bind(filter.assets())
            .bind(library_id.map(|id| id.as_uuid()))
            .bind(bounds.west)
            .bind(bounds.south)
            .bind(bounds.east)
            .bind(bounds.north)
            .fetch_all(self.db.pool())
            .await?;
        Ok(rows
            .into_iter()
            .map(|(month, count)| MonthBucket { month, count })
            .collect())
    }

    /// Pagina keyset dentro un mese. `limit` è clampato a 1..=200. Restituisce
    /// solo il primario di ogni pila, con il badge di stack (Task 3): un
    /// asset RAW+JPEG impilato è una tessera, non due.
    ///
    /// # Errors
    /// `Forbidden` / `Connection` come `buckets`.
    pub async fn page(
        &self,
        ctx: &AuthContext,
        bucket: NaiveDate,
        cursor: Option<(DateTime<Utc>, AssetId)>,
        limit: i64,
    ) -> Result<Vec<AssetWithStack>, DbError> {
        let limit = limit.clamp(1, 200);
        let scope = VisibilityScope::resolve(self.db, ctx).await?;
        let filter = scope.filter("f.path", "f.library_id", "a.id", 6);
        let start = month_start(bucket);
        let end = bucket
            .checked_add_months(Months::new(1))
            .map_or(start, month_start);
        let (cursor_time, cursor_id) = match cursor {
            Some((t, id)) => (Some(t), Some(id.as_uuid())),
            None => (None, None),
        };
        let sql = format!(
            "SELECT {A_COLUMNS}, {STACK_BADGE_COLUMNS_SQL} FROM assets a \
             JOIN folders f ON f.id = a.folder_id \
             {STACK_BADGE_JOIN_SQL} \
             WHERE {} \
               AND a.status = 'indexed' \
               AND a.kind <> 'unknown' \
               AND a.taken_at_utc >= $1 AND a.taken_at_utc < $2 \
               AND ($3::timestamptz IS NULL \
                    OR a.taken_at_utc < $3 \
                    OR (a.taken_at_utc = $3 AND a.id < $4)) \
               AND {STACK_PRIMARY_ONLY_SQL} \
             ORDER BY a.taken_at_utc DESC NULLS LAST, a.id DESC \
             LIMIT $5",
            filter.sql()
        );
        let rows: Vec<AssetStackRow> = sqlx::query_as(&sql)
            .bind(start)
            .bind(end)
            .bind(cursor_time)
            .bind(cursor_id)
            .bind(limit)
            .bind(filter.bind())
            .bind(filter.holes())
            .bind(filter.assets())
            .fetch_all(self.db.pool())
            .await?;
        rows.into_iter().map(AssetStackRow::into_domain).collect()
    }

    /// Pagina keyset dentro un mese e un riquadro geografico.
    ///
    /// # Errors
    /// `Forbidden` / `Connection` come `page`.
    pub async fn page_in_bounds(
        &self,
        ctx: &AuthContext,
        bucket: NaiveDate,
        cursor: Option<(DateTime<Utc>, AssetId)>,
        limit: i64,
        bounds: MapBounds,
    ) -> Result<Vec<AssetWithStack>, DbError> {
        let limit = limit.clamp(1, 200);
        let scope = VisibilityScope::resolve(self.db, ctx).await?;
        let filter = scope.filter("f.path", "f.library_id", "a.id", 6);
        let start = month_start(bucket);
        let end = bucket
            .checked_add_months(Months::new(1))
            .map_or(start, month_start);
        let (cursor_time, cursor_id) = match cursor {
            Some((t, id)) => (Some(t), Some(id.as_uuid())),
            None => (None, None),
        };
        let bbox = effective_bbox_filter_sql(9, 10, 11, 12);
        let sql = format!(
            "SELECT {A_COLUMNS}, {STACK_BADGE_COLUMNS_SQL} FROM assets a \
             JOIN folders f ON f.id = a.folder_id \
             LEFT JOIN asset_overrides o ON o.asset_id = a.id \
             {STACK_BADGE_JOIN_SQL} \
             WHERE {} \
               AND a.status = 'indexed' \
               AND a.kind <> 'unknown' \
               AND a.taken_at_utc >= $1 AND a.taken_at_utc < $2 \
               AND ($3::timestamptz IS NULL \
                    OR a.taken_at_utc < $3 \
                    OR (a.taken_at_utc = $3 AND a.id < $4)) \
               AND ({bbox}) \
               AND {STACK_PRIMARY_ONLY_SQL} \
             ORDER BY a.taken_at_utc DESC NULLS LAST, a.id DESC \
             LIMIT $5",
            filter.sql()
        );
        let rows: Vec<AssetStackRow> = sqlx::query_as(&sql)
            .bind(start)
            .bind(end)
            .bind(cursor_time)
            .bind(cursor_id)
            .bind(limit)
            .bind(filter.bind())
            .bind(filter.holes())
            .bind(filter.assets())
            .bind(bounds.west)
            .bind(bounds.south)
            .bind(bounds.east)
            .bind(bounds.north)
            .fetch_all(self.db.pool())
            .await?;
        rows.into_iter().map(AssetStackRow::into_domain).collect()
    }

    /// Geometria di tutta la vista (nessuna paginazione): larghezza, altezza
    /// e istante dello scatto per ogni asset visibile, nello stesso ordine
    /// della timeline. Gli asset senza `width`/`height` nota restano nel
    /// risultato con `None`: escluderli farebbe "saltare" il layout quando il
    /// sizing arriva (spec fase-10 §2.3, punto 5).
    ///
    /// Filtra `kind <> 'unknown'` come `page` (Ruling Task 3: prima non lo
    /// faceva, per restare coerente con `folder_month_counts`, che non guarda
    /// `kind` — ma ora che `buckets` non legge più da lì e filtra `kind`
    /// direttamente, è quella la coerenza da mantenere). Restituisce solo il
    /// primario di ogni pila (Task 3). La query resta index-only su
    /// `assets_geometry_idx` (`folder_id, taken_at_utc DESC, id DESC INCLUDE
    /// (width, height, stack_id, kind) WHERE status = 'indexed'`, migrazione
    /// 0035): sia `stack_id` (per il filtro di primario) sia `kind` sono
    /// nell'`INCLUDE`; il join verso `stacks` per l'uguaglianza col primario
    /// tocca solo quella tabella, piccola, non `assets`.
    ///
    /// # Errors
    /// `Forbidden` se `library_id` non è del chiamante; `Connection` se la
    /// query fallisce.
    pub async fn geometry(
        &self,
        ctx: &AuthContext,
        library_id: Option<LibraryId>,
        page: Option<GeometryPage>,
    ) -> Result<Geometry, DbError> {
        if let Some(id) = library_id {
            LibraryRepo::new(self.db).find_by_id(ctx, id).await?;
        }
        let scope = VisibilityScope::resolve(self.db, ctx).await?;
        let filter = scope.filter("f.path", "f.library_id", "a.id", 1);
        let (cursor_time, cursor_id, limit) = page.map_or((None, None, None), |p| {
            (
                p.after.map(|(t, _)| t),
                p.after.map(|(_, id)| id.as_uuid()),
                Some(p.limit.clamp(1, GEOMETRY_PAGE_LIMIT_MAX)),
            )
        });
        // `LIMIT $7` è sempre nella query: Postgres tratta `LIMIT NULL` come
        // "nessun limite" (equivalente a ometterlo), quindi non serve un
        // ramo di SQL condizionale — solo un binding sempre presente, che
        // tiene il conteggio dei placeholder fisso.
        let sql = format!(
            "SELECT a.id, a.width, a.height, a.taken_at_utc FROM assets a \
             JOIN folders f ON f.id = a.folder_id \
             {STACK_PRIMARY_JOIN_SQL} \
             WHERE {} \
               AND a.status = 'indexed' \
               AND a.kind <> 'unknown' \
               AND a.taken_at_utc IS NOT NULL \
               AND ($4::uuid IS NULL OR f.library_id = $4) \
               AND ($5::timestamptz IS NULL \
                    OR a.taken_at_utc < $5 \
                    OR (a.taken_at_utc = $5 AND a.id < $6)) \
               AND {STACK_PRIMARY_ONLY_SQL} \
             ORDER BY a.taken_at_utc DESC, a.id DESC \
             LIMIT $7",
            filter.sql()
        );
        let rows: Vec<GeometryRow> = sqlx::query_as(&sql)
            .bind(filter.bind())
            .bind(filter.holes())
            .bind(filter.assets())
            .bind(library_id.map(|id| id.as_uuid()))
            .bind(cursor_time)
            .bind(cursor_id)
            .bind(limit)
            .fetch_all(self.db.pool())
            .await?;
        // Se `page` è impostata e la risposta arriva esattamente al `limit`
        // (clampato) richiesto, potrebbe esserci altro dopo: il chiamante
        // HTTP lo segnala col cursore dell'ultima riga. Se la risposta è
        // più corta (o `page` è `None`), quella era l'intera vista.
        let next_cursor = limit
            .filter(|&l| usize::try_from(l).is_ok_and(|l| rows.len() == l))
            .and_then(|_| {
                rows.last()
                    .map(|(id, _, _, taken_at_utc)| (*taken_at_utc, AssetId::from_uuid(*id)))
            });
        let records = rows
            .into_iter()
            .map(|(_, width, height, taken_at_utc)| GeometryRecord {
                width,
                height,
                taken_at_utc,
            })
            .collect();

        // Il timbro per l'ETag ha senso solo sulla vista intera (Task 6): una
        // richiesta paginata (Task 4-bis, cold-start) salta la validazione
        // 304 e questa query, che altrimenti pagherebbe una scansione in più
        // per ogni pagina senza usarne mai il risultato.
        let last_modified = if page.is_none() {
            let last_modified_sql = format!(
                "SELECT max(a.updated_at) FROM assets a \
                 JOIN folders f ON f.id = a.folder_id \
                 {STACK_PRIMARY_JOIN_SQL} \
                 WHERE {} \
                   AND a.status = 'indexed' \
                   AND a.kind <> 'unknown' \
                   AND a.taken_at_utc IS NOT NULL \
                   AND ($4::uuid IS NULL OR f.library_id = $4) \
                   AND {STACK_PRIMARY_ONLY_SQL}",
                filter.sql()
            );
            sqlx::query_scalar(&last_modified_sql)
                .bind(filter.bind())
                .bind(filter.holes())
                .bind(filter.assets())
                .bind(library_id.map(|id| id.as_uuid()))
                .fetch_one(self.db.pool())
                .await?
        } else {
            None
        };
        Ok(Geometry {
            records,
            last_modified,
            next_cursor,
        })
    }

    /// `count(*)` + `max(updated_at)` sugli stessi filtri di [`Self::geometry`],
    /// senza leggere `width`/`height`. Serve a rispondere `304` senza pagare
    /// la scansione completa della vista.
    ///
    /// # Errors
    /// Come [`Self::geometry`].
    pub async fn geometry_stamp(
        &self,
        ctx: &AuthContext,
        library_id: Option<LibraryId>,
    ) -> Result<GeometryStamp, DbError> {
        if let Some(id) = library_id {
            LibraryRepo::new(self.db).find_by_id(ctx, id).await?;
        }
        let scope = VisibilityScope::resolve(self.db, ctx).await?;
        let filter = scope.filter("f.path", "f.library_id", "a.id", 1);
        let sql = format!(
            "SELECT count(*)::bigint, max(a.updated_at) FROM assets a \
             JOIN folders f ON f.id = a.folder_id \
             {STACK_PRIMARY_JOIN_SQL} \
             WHERE {} \
               AND a.status = 'indexed' \
               AND a.kind <> 'unknown' \
               AND a.taken_at_utc IS NOT NULL \
               AND ($4::uuid IS NULL OR f.library_id = $4) \
               AND {STACK_PRIMARY_ONLY_SQL}",
            filter.sql()
        );
        let (count, last_modified): (i64, Option<DateTime<Utc>>) = sqlx::query_as(&sql)
            .bind(filter.bind())
            .bind(filter.holes())
            .bind(filter.assets())
            .bind(library_id.map(|id| id.as_uuid()))
            .fetch_one(self.db.pool())
            .await?;
        Ok(GeometryStamp {
            count: u64::try_from(count).unwrap_or(0),
            last_modified,
        })
    }

    /// Come [`Self::geometry`], ma ristretta a un riquadro geografico. Filtra
    /// `kind <> 'unknown'` come `buckets_in_bounds`/`page_in_bounds`: qui la
    /// query tocca comunque `asset_overrides` per la posizione effettiva, e
    /// non c'è indice di copertura da preservare come nel caso senza filtri.
    ///
    /// # Errors
    /// `Forbidden` / `Connection` come [`Self::geometry`].
    pub async fn geometry_in_bounds(
        &self,
        ctx: &AuthContext,
        library_id: Option<LibraryId>,
        bounds: MapBounds,
        page: Option<GeometryPage>,
    ) -> Result<Geometry, DbError> {
        if let Some(id) = library_id {
            LibraryRepo::new(self.db).find_by_id(ctx, id).await?;
        }
        let scope = VisibilityScope::resolve(self.db, ctx).await?;
        let filter = scope.filter("f.path", "f.library_id", "a.id", 1);
        let bbox = effective_bbox_filter_sql(5, 6, 7, 8);
        let (cursor_time, cursor_id, limit) = page.map_or((None, None, None), |p| {
            (
                p.after.map(|(t, _)| t),
                p.after.map(|(_, id)| id.as_uuid()),
                Some(p.limit.clamp(1, GEOMETRY_PAGE_LIMIT_MAX)),
            )
        });
        // Come in `geometry`: `LIMIT $11` sempre presente, `NULL` = nessun
        // limite — niente ramo di SQL condizionale.
        let sql = format!(
            "SELECT a.id, a.width, a.height, a.taken_at_utc FROM assets a \
             JOIN folders f ON f.id = a.folder_id \
             LEFT JOIN asset_overrides o ON o.asset_id = a.id \
             {STACK_PRIMARY_JOIN_SQL} \
             WHERE {} \
               AND a.status = 'indexed' \
               AND a.kind <> 'unknown' \
               AND a.taken_at_utc IS NOT NULL \
               AND ($4::uuid IS NULL OR f.library_id = $4) \
               AND ({bbox}) \
               AND ($9::timestamptz IS NULL \
                    OR a.taken_at_utc < $9 \
                    OR (a.taken_at_utc = $9 AND a.id < $10)) \
               AND {STACK_PRIMARY_ONLY_SQL} \
             ORDER BY a.taken_at_utc DESC, a.id DESC \
             LIMIT $11",
            filter.sql()
        );
        let rows: Vec<GeometryRow> = sqlx::query_as(&sql)
            .bind(filter.bind())
            .bind(filter.holes())
            .bind(filter.assets())
            .bind(library_id.map(|id| id.as_uuid()))
            .bind(bounds.west)
            .bind(bounds.south)
            .bind(bounds.east)
            .bind(bounds.north)
            .bind(cursor_time)
            .bind(cursor_id)
            .bind(limit)
            .fetch_all(self.db.pool())
            .await?;
        let next_cursor = limit
            .filter(|&l| usize::try_from(l).is_ok_and(|l| rows.len() == l))
            .and_then(|_| {
                rows.last()
                    .map(|(id, _, _, taken_at_utc)| (*taken_at_utc, AssetId::from_uuid(*id)))
            });
        let records = rows
            .into_iter()
            .map(|(_, width, height, taken_at_utc)| GeometryRecord {
                width,
                height,
                taken_at_utc,
            })
            .collect();

        let last_modified = if page.is_none() {
            let last_modified_sql = format!(
                "SELECT max(a.updated_at) FROM assets a \
                 JOIN folders f ON f.id = a.folder_id \
                 LEFT JOIN asset_overrides o ON o.asset_id = a.id \
                 {STACK_PRIMARY_JOIN_SQL} \
                 WHERE {} \
                   AND a.status = 'indexed' \
                   AND a.kind <> 'unknown' \
                   AND a.taken_at_utc IS NOT NULL \
                   AND ($4::uuid IS NULL OR f.library_id = $4) \
                   AND ({bbox}) \
                   AND {STACK_PRIMARY_ONLY_SQL}",
                filter.sql()
            );
            sqlx::query_scalar(&last_modified_sql)
                .bind(filter.bind())
                .bind(filter.holes())
                .bind(filter.assets())
                .bind(library_id.map(|id| id.as_uuid()))
                .bind(bounds.west)
                .bind(bounds.south)
                .bind(bounds.east)
                .bind(bounds.north)
                .fetch_one(self.db.pool())
                .await?
        } else {
            None
        };
        Ok(Geometry {
            records,
            last_modified,
            next_cursor,
        })
    }

    /// Timbratura leggera sugli stessi filtri di [`Self::geometry_in_bounds`].
    ///
    /// # Errors
    /// Come [`Self::geometry_in_bounds`].
    pub async fn geometry_stamp_in_bounds(
        &self,
        ctx: &AuthContext,
        library_id: Option<LibraryId>,
        bounds: MapBounds,
    ) -> Result<GeometryStamp, DbError> {
        if let Some(id) = library_id {
            LibraryRepo::new(self.db).find_by_id(ctx, id).await?;
        }
        let scope = VisibilityScope::resolve(self.db, ctx).await?;
        let filter = scope.filter("f.path", "f.library_id", "a.id", 1);
        let bbox = effective_bbox_filter_sql(5, 6, 7, 8);
        let sql = format!(
            "SELECT count(*)::bigint, max(a.updated_at) FROM assets a \
             JOIN folders f ON f.id = a.folder_id \
             LEFT JOIN asset_overrides o ON o.asset_id = a.id \
             {STACK_PRIMARY_JOIN_SQL} \
             WHERE {} \
               AND a.status = 'indexed' \
               AND a.kind <> 'unknown' \
               AND a.taken_at_utc IS NOT NULL \
               AND ($4::uuid IS NULL OR f.library_id = $4) \
               AND ({bbox}) \
               AND {STACK_PRIMARY_ONLY_SQL}",
            filter.sql()
        );
        let (count, last_modified): (i64, Option<DateTime<Utc>>) = sqlx::query_as(&sql)
            .bind(filter.bind())
            .bind(filter.holes())
            .bind(filter.assets())
            .bind(library_id.map(|id| id.as_uuid()))
            .bind(bounds.west)
            .bind(bounds.south)
            .bind(bounds.east)
            .bind(bounds.north)
            .fetch_one(self.db.pool())
            .await?;
        Ok(GeometryStamp {
            count: u64::try_from(count).unwrap_or(0),
            last_modified,
        })
    }
}

fn effective_bbox_filter_sql(west: usize, south: usize, east: usize, north: usize) -> String {
    let w = format!("${west}");
    let s = format!("${south}");
    let e = format!("${east}");
    let n = format!("${north}");
    format!(
        "({w} <= {e} AND (\
             (o.location IS NOT NULL AND o.location \
              && ST_Segmentize(ST_MakeEnvelope({w}, {s}, {e}, {n}, 4326), 90.0)::geography) \
             OR (o.location IS NULL AND a.location \
                 && ST_Segmentize(ST_MakeEnvelope({w}, {s}, {e}, {n}, 4326), 90.0)::geography)\
         )) OR ({w} > {e} AND (\
             (o.location IS NOT NULL AND (\
                 o.location && ST_Segmentize(\
                     ST_MakeEnvelope({w}, {s}, 180.0, {n}, 4326), 90.0\
                 )::geography \
                 OR o.location && ST_Segmentize(\
                     ST_MakeEnvelope(-180.0, {s}, {e}, {n}, 4326), 90.0\
                 )::geography\
             )) OR (o.location IS NULL AND (\
                 a.location && ST_Segmentize(\
                     ST_MakeEnvelope({w}, {s}, 180.0, {n}, 4326), 90.0\
                 )::geography \
                 OR a.location && ST_Segmentize(\
                     ST_MakeEnvelope(-180.0, {s}, {e}, {n}, 4326), 90.0\
                 )::geography\
             ))\
         ))"
    )
}

fn month_start(d: NaiveDate) -> DateTime<Utc> {
    d.and_hms_opt(0, 0, 0)
        .map_or(DateTime::<Utc>::UNIX_EPOCH, |ndt| ndt.and_utc())
}
