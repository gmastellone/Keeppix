//! Duplicati per `content_hash` (spec §7): deduplica **esatta per hash**,
//! nessun ML. Un gruppo è un insieme di asset con lo stesso `content_hash` e
//! `count > 1`; l'azione utente è «tieni questo, elimina gli altri».
//!
//! Questa era la scansione di [`crate::problems::ProblemsRepo::duplicates`]
//! della Fase 1c: qui si estende con la lista dei singoli membri di un
//! gruppo (necessaria per scegliere quale tenere) e con l'azione di
//! risoluzione, che riusa [`crate::TrashRepo`] invece di reimplementare le
//! tre opzioni di cancellazione.

use keeppix_domain::{Asset, AssetId, AssetStatus, AuthContext, DiskAction};

use crate::visibility::VisibilityScope;
use crate::{AssetRepo, Db, DbError, TrashRepo};

pub struct DuplicateRepo<'a> {
    db: &'a Db,
}

#[derive(Debug, Clone)]
pub struct DuplicateGroup {
    pub content_hash: Vec<u8>,
    pub count: i64,
    pub size_bytes: i64,
}

impl DuplicateGroup {
    /// Spazio recuperabile: `size_bytes * (copie - 1)`, **non** la somma
    /// totale delle copie — la prima copia non è "recuperabile", è la foto.
    #[must_use]
    pub const fn reclaimable_bytes(&self) -> i64 {
        self.size_bytes.saturating_mul(self.count.saturating_sub(1))
    }
}

impl<'a> DuplicateRepo<'a> {
    #[must_use]
    pub const fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// Gruppi con lo stesso `content_hash` e più di una copia, visibili al
    /// chiamante. Un asset `trashed` non conta: è già in coda per sparire,
    /// e le sue tre opzioni di cancellazione (spec §6) contano già una
    /// copia in meno di quelle davvero recuperabili.
    ///
    /// # Errors
    /// `Connection` se la query fallisce.
    pub async fn groups(&self, ctx: &AuthContext) -> Result<Vec<DuplicateGroup>, DbError> {
        let scope = VisibilityScope::resolve(self.db, ctx).await?;
        let filter = scope.filter("f.path", "f.library_id", 1);
        let rows: Vec<(Vec<u8>, i64, i64)> = sqlx::query_as(&format!(
            "SELECT a.content_hash, count(*)::bigint, min(a.size_bytes)::bigint \
               FROM assets a \
               JOIN folders f ON f.id = a.folder_id \
              WHERE a.content_hash IS NOT NULL AND a.status <> 'trashed' AND {} \
              GROUP BY a.content_hash \
             HAVING count(*) > 1 \
              ORDER BY (min(a.size_bytes) * (count(*) - 1)) DESC \
              LIMIT 200",
            filter.sql()
        ))
        .bind(filter.bind())
        .bind(filter.holes())
        .fetch_all(self.db.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|(content_hash, count, size_bytes)| DuplicateGroup {
                content_hash,
                count,
                size_bytes,
            })
            .collect())
    }

    /// I singoli asset di un gruppo, per scegliere quale tenere. Esclude i
    /// `trashed` per lo stesso motivo di [`Self::groups`] — non hanno senso
    /// come bersaglio di "tieni questo" o "elimina questo", sono già gestiti
    /// dal cestino.
    ///
    /// # Errors
    /// `Connection` se la query fallisce.
    pub async fn members(
        &self,
        ctx: &AuthContext,
        content_hash: &[u8; 32],
    ) -> Result<Vec<Asset>, DbError> {
        let assets = AssetRepo::new(self.db)
            .find_by_hash(ctx, content_hash)
            .await?;
        Ok(assets
            .into_iter()
            .filter(|a| a.status != AssetStatus::Trashed)
            .collect())
    }

    /// «Tieni questo, elimina gli altri» (spec §7): applica `action` a ogni
    /// altro membro **non cestinato** del gruppo. Riusa
    /// [`crate::TrashRepo::choose`] asset per asset — le tre opzioni, i
    /// controlli di visibilità e il cancello su `Purged` sono già lì, non
    /// vanno duplicati qui.
    ///
    /// # Errors
    /// `Forbidden` se `keep` non è visibile al chiamante o non appartiene
    /// al gruppo — anche quando l'id non esiste, per non offrire un
    /// oracolo di esistenza. Altrimenti, lo stesso errore di
    /// [`crate::TrashRepo::choose`] sul primo membro che fallisce: gli
    /// altri restano cestinati, non è un'operazione tutto-o-niente perché
    /// una foto già spostata non deve tornare indietro se la successiva
    /// fallisce.
    pub async fn resolve(
        &self,
        ctx: &AuthContext,
        content_hash: &[u8; 32],
        keep: AssetId,
        action: DiskAction,
    ) -> Result<usize, DbError> {
        let members = self.members(ctx, content_hash).await?;
        if !members.iter().any(|a| a.id == keep) {
            // `keep` non è nel gruppo (o non è visibile: `find_by_hash` lo
            // avrebbe già filtrato) — stesso trattamento riservato a ogni
            // id sondato fuori dalla propria visibilità.
            return Err(DbError::Forbidden);
        }

        let trash = TrashRepo::new(self.db);
        let mut resolved = 0usize;
        for member in members {
            if member.id == keep {
                continue;
            }
            trash.choose(ctx, member.id, action).await?;
            resolved += 1;
        }
        Ok(resolved)
    }
}
