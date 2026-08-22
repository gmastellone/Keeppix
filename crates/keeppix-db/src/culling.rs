//! Culling a cartelle (Fase 9 Task 2-5): la radice designata, i lotti sotto
//! di essa, e lo spostamento fisico che accompagna scelto/scartato.

use keeppix_domain::{AuthContext, CullingLot, FolderId, LibraryId};

use crate::{Db, DbError, LibraryRepo};

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
}
