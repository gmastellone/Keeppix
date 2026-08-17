use keeppix_domain::{AuthContext, FolderPath, LibraryId};

use crate::{Db, DbError};

#[derive(Clone)]
struct FolderGrant {
    id: uuid::Uuid,
    library_id: uuid::Uuid,
    path: String,
}

/// Filtro di visibilità risolto per un chiamante.
///
/// Prefissi = cartelle visibili (radici delle librerie possedute +
/// condivisioni). Buchi = `inherit = false`. I path `ltree` sono unici
/// **dentro** una libreria, non nel database: il filtro accoppia sempre
/// `library_id` e `path`.
pub struct VisibilityScope {
    unrestricted: bool,
    grants: Vec<FolderGrant>,
    holes: Vec<FolderGrant>,
}

/// Clausola SQL + due `uuid[]`: id delle cartelle concesse (`NULL` = admin)
/// e id dei buchi. Un array vuoto di concessi non matcha nulla.
pub struct VisibilityFilter {
    sql: String,
    grants: Option<Vec<uuid::Uuid>>,
    holes: Vec<uuid::Uuid>,
}

impl VisibilityFilter {
    #[must_use]
    pub fn sql(&self) -> &str {
        &self.sql
    }

    #[must_use]
    pub fn bind(&self) -> Option<&[uuid::Uuid]> {
        self.grants.as_deref()
    }

    #[must_use]
    pub fn holes(&self) -> &[uuid::Uuid] {
        &self.holes
    }
}

impl VisibilityScope {
    /// # Errors
    /// `Connection` se la query delle librerie o dei permessi fallisce.
    pub async fn resolve(db: &Db, ctx: &AuthContext) -> Result<Self, DbError> {
        if ctx.is_admin() {
            return Ok(Self {
                unrestricted: true,
                grants: Vec::new(),
                holes: Vec::new(),
            });
        }

        let Some(owner) = ctx.user_id() else {
            return Ok(Self {
                unrestricted: false,
                grants: Vec::new(),
                holes: Vec::new(),
            });
        };

        let rows: Vec<(uuid::Uuid, uuid::Uuid, String, bool)> = sqlx::query_as(
            "SELECT f.id, f.library_id, f.path::text, true \
               FROM folders f \
               JOIN libraries l ON l.id = f.library_id \
              WHERE l.owner_id = $1 AND f.parent_id IS NULL \
             UNION ALL \
             SELECT f.id, f.library_id, f.path::text, p.inherit \
               FROM permissions p \
               JOIN folders f ON f.id = p.object_id \
              WHERE p.object_type = 'folder' \
                AND ( \
                     (p.subject_type = 'user' AND p.subject_id = $1) \
                  OR (p.subject_type = 'group' AND p.subject_id IN ( \
                        SELECT group_id FROM group_members WHERE user_id = $1 \
                     )) \
                )",
        )
        .bind(owner.as_uuid())
        .fetch_all(db.pool())
        .await?;

        let mut grants = Vec::new();
        let mut holes = Vec::new();
        for (id, library_id, path, inherit) in rows {
            let grant = FolderGrant {
                id,
                library_id,
                path,
            };
            if inherit {
                grants.push(grant);
            } else {
                holes.push(grant);
            }
        }

        Ok(Self {
            unrestricted: false,
            grants,
            holes,
        })
    }

    #[must_use]
    pub const fn is_unrestricted(&self) -> bool {
        self.unrestricted
    }

    /// True se la cartella cade sotto un prefisso concesso **nella stessa
    /// libreria** e non sotto un buco. Un path illeggibile è un no.
    #[must_use]
    pub fn allows(&self, library_id: LibraryId, path: &str) -> bool {
        if self.unrestricted {
            return true;
        }
        let Ok(candidate) = FolderPath::parse(path) else {
            return false;
        };
        let lib = library_id.as_uuid();
        let granted = self.grants.iter().any(|g| {
            g.library_id == lib
                && FolderPath::parse(&g.path)
                    .ok()
                    .is_some_and(|grant| candidate.is_descendant_of(&grant))
        });
        if !granted {
            return false;
        }
        let blocked = self.holes.iter().any(|h| {
            h.library_id == lib
                && FolderPath::parse(&h.path)
                    .ok()
                    .is_some_and(|hole| candidate.is_descendant_of(&hole))
        });
        !blocked
    }

    /// Clausola su `path` + `library_id`. Occupa due parametri da `param`.
    /// Le espressioni **devono essere qualificate** (`f.path`, `folders.path`):
    /// il sottoquery `vis_g` ha colonne omonime, e un `path` nudo si lega
    /// a quello interno — `EXISTS` diventerebbe vero per ogni riga.
    #[must_use]
    pub fn filter(&self, path_sql: &str, library_sql: &str, param: usize) -> VisibilityFilter {
        let holes_param = param + 1;
        VisibilityFilter {
            sql: format!(
                "(${param}::uuid[] IS NULL OR ( \
                    EXISTS ( \
                      SELECT 1 FROM folders vis_g \
                       WHERE vis_g.id = ANY(${param}::uuid[]) \
                         AND {library_sql} = vis_g.library_id \
                         AND {path_sql} <@ vis_g.path \
                    ) AND NOT EXISTS ( \
                      SELECT 1 FROM folders vis_h \
                       WHERE vis_h.id = ANY(${holes_param}::uuid[]) \
                         AND {library_sql} = vis_h.library_id \
                         AND {path_sql} <@ vis_h.path \
                    ) \
                 ))"
            ),
            grants: if self.unrestricted {
                None
            } else {
                Some(self.grants.iter().map(|g| g.id).collect())
            },
            holes: if self.unrestricted {
                Vec::new()
            } else {
                self.holes.iter().map(|h| h.id).collect()
            },
        }
    }

    /// Clausola su `library_id` per tabelle senza path (`change_log`).
    #[must_use]
    pub fn filter_library(&self, library_id_sql: &str, param: usize) -> VisibilityFilter {
        let holes_param = param + 1;
        VisibilityFilter {
            sql: format!(
                "(${param}::uuid[] IS NULL OR {library_id_sql} IN ( \
                    SELECT vis_g.library_id FROM folders vis_g \
                     WHERE vis_g.id = ANY(${param}::uuid[]) \
                 ) AND cardinality(COALESCE(${holes_param}::uuid[], '{{}}'::uuid[])) >= 0)"
            ),
            grants: if self.unrestricted {
                None
            } else {
                Some(self.grants.iter().map(|g| g.id).collect())
            },
            holes: if self.unrestricted {
                Vec::new()
            } else {
                self.holes.iter().map(|h| h.id).collect()
            },
        }
    }
}
