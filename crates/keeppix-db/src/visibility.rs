use keeppix_domain::{AuthContext, LibraryId};

use crate::{Db, DbError};

/// Filtro di visibilità risolto per un chiamante.
///
/// In 1a: le librerie che possiedi, o tutte se sei admin. In Fase 3: più i
/// sottoalberi condivisi. I chiamanti usano [`Self::filter`], non l'elenco
/// grezzo degli id — altrimenti la Fase 3 dovrebbe riscriverli tutti.
pub struct VisibilityScope {
    unrestricted: bool,
    library_ids: Vec<LibraryId>,
}

/// Clausola SQL + i valori da bindare. Oggi un solo `uuid[]` (NULL = admin);
/// la Fase 3 può aggiungere bind senza cambiare i chiamanti che usano
/// `sql()` e `bind()`.
pub struct VisibilityFilter {
    sql: String,
    /// `None` = nessuna restrizione (admin). `Some` anche vuoto = solo quelle
    /// librerie: `= ANY('{}')` non matcha nulla, senza diventare `IN ()`.
    bind: Option<Vec<uuid::Uuid>>,
}

impl VisibilityFilter {
    #[must_use]
    pub fn sql(&self) -> &str {
        &self.sql
    }

    #[must_use]
    pub fn bind(&self) -> Option<&[uuid::Uuid]> {
        self.bind.as_deref()
    }
}

impl VisibilityScope {
    /// # Errors
    /// `Connection` se la query delle librerie fallisce.
    pub async fn resolve(db: &Db, ctx: &AuthContext) -> Result<Self, DbError> {
        if ctx.is_admin() {
            return Ok(Self {
                unrestricted: true,
                library_ids: Vec::new(),
            });
        }

        let owner = ctx.user_id();
        let ids: Vec<uuid::Uuid> = sqlx::query_scalar(
            "SELECT id FROM libraries WHERE $1::uuid IS NOT NULL AND owner_id = $1 ORDER BY id",
        )
        .bind(owner.map(|id| id.as_uuid()))
        .fetch_all(db.pool())
        .await?;

        Ok(Self {
            unrestricted: false,
            library_ids: ids.into_iter().map(LibraryId::from_uuid).collect(),
        })
    }

    #[must_use]
    pub const fn is_unrestricted(&self) -> bool {
        self.unrestricted
    }

    #[must_use]
    pub fn library_ids(&self) -> &[LibraryId] {
        &self.library_ids
    }

    /// Clausola booleana sull'espressione `library_id_sql`, da bindare al
    /// parametro `$param`. I chiamanti interpolano `filter.sql()` e passano
    /// `filter.bind()` a sqlx: non costruiscono `IN (…)` da soli.
    #[must_use]
    pub fn filter(&self, library_id_sql: &str, param: usize) -> VisibilityFilter {
        VisibilityFilter {
            sql: format!("(${param}::uuid[] IS NULL OR {library_id_sql} = ANY(${param}::uuid[]))"),
            bind: if self.unrestricted {
                None
            } else {
                Some(self.library_ids.iter().map(LibraryId::as_uuid).collect())
            },
        }
    }
}
