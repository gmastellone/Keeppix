use keeppix_domain::{Actor, AuthContext, FolderPath, LibraryId};

use crate::{Db, DbError};

#[derive(Clone)]
struct FolderGrant {
    id: uuid::Uuid,
    library_id: uuid::Uuid,
    path: String,
}

/// Filtro di visibilità risolto per un chiamante.
pub struct VisibilityScope {
    unrestricted: bool,
    grants: Vec<FolderGrant>,
    holes: Vec<FolderGrant>,
    /// Asset visibili senza passare dall'albero cartelle (album o grant diretto).
    asset_ids: Vec<uuid::Uuid>,
}

/// Clausola SQL + tre `uuid[]`: cartelle concesse (`NULL` = admin), buchi,
/// asset espliciti. Un array vuoto di concessi non matcha nulla via path.
pub struct VisibilityFilter {
    sql: String,
    grants: Option<Vec<uuid::Uuid>>,
    holes: Vec<uuid::Uuid>,
    assets: Vec<uuid::Uuid>,
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

    #[must_use]
    pub fn assets(&self) -> &[uuid::Uuid] {
        &self.assets
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
                asset_ids: Vec::new(),
            });
        }

        if let Actor::ShareLink {
            object_type,
            object_id,
            ..
        } = &ctx.actor
        {
            return Self::resolve_share_link(db, object_type, *object_id).await;
        }

        let Some(owner) = ctx.user_id() else {
            return Ok(Self {
                unrestricted: false,
                grants: Vec::new(),
                holes: Vec::new(),
                asset_ids: Vec::new(),
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

        let asset_ids: Vec<uuid::Uuid> = sqlx::query_scalar(
            "SELECT p.object_id FROM permissions p \
              WHERE p.object_type = 'asset' \
                AND ( \
                     (p.subject_type = 'user' AND p.subject_id = $1) \
                  OR (p.subject_type = 'group' AND p.subject_id IN ( \
                        SELECT group_id FROM group_members WHERE user_id = $1 \
                     )) \
                ) \
             UNION \
             SELECT aa.asset_id FROM permissions p \
               JOIN album_assets aa ON aa.album_id = p.object_id \
              WHERE p.object_type = 'album' \
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

        Ok(Self {
            unrestricted: false,
            grants,
            holes,
            asset_ids,
        })
    }

    async fn resolve_share_link(
        db: &Db,
        object_type: &str,
        object_id: uuid::Uuid,
    ) -> Result<Self, DbError> {
        match object_type {
            "asset" => Ok(Self {
                unrestricted: false,
                grants: Vec::new(),
                holes: Vec::new(),
                asset_ids: vec![object_id],
            }),
            "album" => {
                let ids: Vec<uuid::Uuid> =
                    sqlx::query_scalar("SELECT asset_id FROM album_assets WHERE album_id = $1")
                        .bind(object_id)
                        .fetch_all(db.pool())
                        .await?;
                Ok(Self {
                    unrestricted: false,
                    grants: Vec::new(),
                    holes: Vec::new(),
                    asset_ids: ids,
                })
            }
            "folder" => {
                let row: Option<(uuid::Uuid, uuid::Uuid, String)> =
                    sqlx::query_as("SELECT id, library_id, path::text FROM folders WHERE id = $1")
                        .bind(object_id)
                        .fetch_optional(db.pool())
                        .await?;
                let Some((id, library_id, path)) = row else {
                    return Ok(Self {
                        unrestricted: false,
                        grants: Vec::new(),
                        holes: Vec::new(),
                        asset_ids: Vec::new(),
                    });
                };
                Ok(Self {
                    unrestricted: false,
                    grants: vec![FolderGrant {
                        id,
                        library_id,
                        path,
                    }],
                    holes: Vec::new(),
                    asset_ids: Vec::new(),
                })
            }
            _ => Ok(Self {
                unrestricted: false,
                grants: Vec::new(),
                holes: Vec::new(),
                asset_ids: Vec::new(),
            }),
        }
    }

    #[must_use]
    pub const fn is_unrestricted(&self) -> bool {
        self.unrestricted
    }

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

    /// Clausola su path + library + asset id. Occupa tre parametri da `param`.
    #[must_use]
    pub fn filter(
        &self,
        path_sql: &str,
        library_sql: &str,
        asset_id_sql: &str,
        param: usize,
    ) -> VisibilityFilter {
        let holes_param = param + 1;
        let assets_param = param + 2;
        VisibilityFilter {
            sql: format!(
                "( \
                    (${param}::uuid[] IS NULL) \
                    OR ( \
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
                    ) \
                    OR (cardinality(${assets_param}::uuid[]) > 0 \
                        AND {asset_id_sql} = ANY(${assets_param}::uuid[])) \
                 )"
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
            assets: if self.unrestricted {
                Vec::new()
            } else {
                self.asset_ids.clone()
            },
        }
    }

    /// Like [`Self::filter`], but asset-level grants match when any indexed asset
    /// under `folder_fk_sql` is in the grant list (for aggregates without an
    /// `assets` row in the FROM clause).
    #[must_use]
    pub fn filter_for_folder_aggregate(
        &self,
        path_sql: &str,
        library_sql: &str,
        folder_fk_sql: &str,
        param: usize,
    ) -> VisibilityFilter {
        let holes_param = param + 1;
        let assets_param = param + 2;
        VisibilityFilter {
            sql: format!(
                "( \
                    (${param}::uuid[] IS NULL) \
                    OR ( \
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
                    ) \
                    OR (cardinality(${assets_param}::uuid[]) > 0 \
                        AND EXISTS ( \
                          SELECT 1 FROM assets vis_a \
                           WHERE vis_a.folder_id = {folder_fk_sql} \
                             AND vis_a.id = ANY(${assets_param}::uuid[]) \
                        )) \
                 )"
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
            assets: if self.unrestricted {
                Vec::new()
            } else {
                self.asset_ids.clone()
            },
        }
    }

    #[must_use]
    pub fn filter_library(&self, library_id_sql: &str, param: usize) -> VisibilityFilter {
        let holes_param = param + 1;
        let assets_param = param + 2;
        VisibilityFilter {
            sql: format!(
                "( \
                    (${param}::uuid[] IS NULL) \
                    OR {library_id_sql} IN ( \
                        SELECT vis_g.library_id FROM folders vis_g \
                         WHERE vis_g.id = ANY(${param}::uuid[]) \
                    ) \
                    OR cardinality(${assets_param}::uuid[]) > 0 \
                 ) AND cardinality(COALESCE(${holes_param}::uuid[], '{{}}'::uuid[])) >= 0"
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
            assets: if self.unrestricted {
                Vec::new()
            } else {
                self.asset_ids.clone()
            },
        }
    }
}
