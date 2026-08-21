//! Permessi solo-allow. I gruppi si risolvono con un join, mai nel token.

use std::collections::HashMap;

use keeppix_domain::{AssetId, AuthContext, FolderId, ObjectRole, UserId};
use serde::Serialize;
use uuid::Uuid;

use crate::share_links::{album_asset_counts, folder_asset_counts};
use crate::{Db, DbError};

pub struct PermissionRepo<'a> {
    db: &'a Db,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubjectType {
    User,
    Group,
}

impl SubjectType {
    const fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Group => "group",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectType {
    Folder,
    Album,
    Asset,
}

impl ObjectType {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Folder => "folder",
            Self::Album => "album",
            Self::Asset => "asset",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Permission {
    pub id: Uuid,
    pub role: ObjectRole,
    pub inherit: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct NewGrant {
    pub subject: SubjectType,
    pub subject_id: Uuid,
    pub object: ObjectType,
    pub object_id: Uuid,
    pub role: ObjectRole,
    pub inherit: bool,
}

impl<'a> PermissionRepo<'a> {
    #[must_use]
    pub const fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// Concede o aggiorna un permesso. Il controllo «solo owner o admin»
    /// arriva col pannello (Task 4): qui si scrive la riga.
    ///
    /// # Errors
    /// `Forbidden` senza utente; `Connection` se l'upsert fallisce.
    pub async fn grant(&self, ctx: &AuthContext, grant: NewGrant) -> Result<Permission, DbError> {
        self.assert_can_manage(ctx, grant.object, grant.object_id)
            .await?;
        let granted_by = ctx.user_id().ok_or(DbError::Forbidden)?;
        let id = Uuid::now_v7();
        let row: (Uuid, String, bool) = sqlx::query_as(
            "INSERT INTO permissions \
                 (id, subject_type, subject_id, object_type, object_id, role, inherit, granted_by) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8) \
             ON CONFLICT (subject_type, subject_id, object_type, object_id) \
             DO UPDATE SET role = EXCLUDED.role, inherit = EXCLUDED.inherit, \
                           granted_by = EXCLUDED.granted_by \
             RETURNING id, role, inherit",
        )
        .bind(id)
        .bind(grant.subject.as_str())
        .bind(grant.subject_id)
        .bind(grant.object.as_str())
        .bind(grant.object_id)
        .bind(grant.role.as_str())
        .bind(grant.inherit)
        .bind(granted_by.as_uuid())
        .fetch_one(self.db.pool())
        .await?;

        let role = ObjectRole::parse(&row.1)
            .ok_or_else(|| crate::row::corrupted("permission role", &row.1))?;
        self.invalidate_subject_scope(grant.subject, grant.subject_id)
            .await?;
        Ok(Permission {
            id: row.0,
            role,
            inherit: row.2,
        })
    }

    /// Ruolo più alto fra i permessi applicabili su una cartella, incluso
    /// ciò che arriva dai gruppi. Vince `editor` su `viewer`.
    ///
    /// # Errors
    /// `Connection` se la query fallisce.
    pub async fn effective_role(
        &self,
        ctx: &AuthContext,
        folder_id: FolderId,
    ) -> Result<Option<ObjectRole>, DbError> {
        let Some(user_id) = ctx.user_id() else {
            return Ok(None);
        };
        let role: Option<String> = sqlx::query_scalar(
            "SELECT p.role \
               FROM permissions p \
              WHERE p.object_type = 'folder' AND p.object_id = $2 \
                AND ( \
                     (p.subject_type = 'user' AND p.subject_id = $1) \
                  OR (p.subject_type = 'group' AND p.subject_id IN ( \
                        SELECT group_id FROM group_members WHERE user_id = $1 \
                     )) \
                ) \
              ORDER BY CASE p.role WHEN 'editor' THEN 1 ELSE 0 END DESC \
              LIMIT 1",
        )
        .bind(user_id.as_uuid())
        .bind(folder_id.as_uuid())
        .fetch_optional(self.db.pool())
        .await?;

        Ok(role.as_deref().and_then(ObjectRole::parse))
    }

    /// Stesso cancello di `FolderRepo::move_subtree`: owner/admin, oppure
    /// `editor` sulla cartella dell'asset. Un viewer che *vede* non scrive.
    ///
    /// # Errors
    /// `Forbidden` se anche un solo asset è sotto una cartella dove il
    /// chiamante non è owner/admin né editor. `Connection` se la query fallisce.
    pub async fn assert_can_edit_assets(
        &self,
        ctx: &AuthContext,
        asset_ids: &[AssetId],
    ) -> Result<(), DbError> {
        // Admin: short-circuit come prima — la visibilità resta a
        // `assert_visible` / `filter_visible` del chiamante.
        if asset_ids.is_empty() || ctx.is_admin() {
            return Ok(());
        }
        let (_, failed) = self.partition_editable_assets(ctx, asset_ids).await?;
        if failed.is_empty() {
            Ok(())
        } else {
            Err(DbError::Forbidden)
        }
    }

    /// Parte gli `asset_ids` in modificabili / no, senza abortire al primo
    /// rifiuto. Gli id non visibili e quelli solo in lettura finiscono in
    /// `failed` con `Forbidden` (o `NotFound` per un admin su id assente).
    ///
    /// # Errors
    /// `Connection` se una query fallisce.
    pub async fn partition_editable_assets(
        &self,
        ctx: &AuthContext,
        asset_ids: &[AssetId],
    ) -> Result<(Vec<AssetId>, Vec<(AssetId, DbError)>), DbError> {
        if asset_ids.is_empty() {
            return Ok((Vec::new(), Vec::new()));
        }

        let visible = crate::AssetRepo::new(self.db)
            .filter_visible(ctx, asset_ids)
            .await?;
        let visible_set: std::collections::HashSet<Uuid> =
            visible.iter().map(AssetId::as_uuid).collect();

        let mut failed = Vec::new();
        let mut candidates = Vec::new();
        for &id in asset_ids {
            if visible_set.contains(&id.as_uuid()) {
                candidates.push(id);
            } else if ctx.is_admin() {
                failed.push((id, DbError::NotFound));
            } else {
                failed.push((id, DbError::Forbidden));
            }
        }

        if candidates.is_empty() || ctx.is_admin() {
            return Ok((candidates, failed));
        }

        let ids: Vec<Uuid> = candidates.iter().map(AssetId::as_uuid).collect();
        let rows: Vec<(Uuid, Uuid, Uuid)> = sqlx::query_as(
            "SELECT a.id, a.folder_id, l.owner_id \
               FROM assets a \
               JOIN folders f ON f.id = a.folder_id \
               JOIN libraries l ON l.id = f.library_id \
              WHERE a.id = ANY($1)",
        )
        .bind(&ids)
        .fetch_all(self.db.pool())
        .await?;
        let meta: std::collections::HashMap<Uuid, (Uuid, Uuid)> = rows
            .into_iter()
            .map(|(asset_id, folder_id, owner_id)| (asset_id, (folder_id, owner_id)))
            .collect();

        let mut editable = Vec::new();
        for &id in &candidates {
            let Some((folder_id, owner_id)) = meta.get(&id.as_uuid()).copied() else {
                failed.push((id, DbError::Forbidden));
                continue;
            };
            if ctx.user_id() == Some(UserId::from_uuid(owner_id)) {
                editable.push(id);
                continue;
            }
            match self
                .effective_role(ctx, FolderId::from_uuid(folder_id))
                .await?
            {
                Some(ObjectRole::Editor) => editable.push(id),
                _ => failed.push((id, DbError::Forbidden)),
            }
        }
        Ok((editable, failed))
    }

    /// Elenco permessi diretti su un oggetto.
    ///
    /// # Errors
    /// `Forbidden` se il chiamante non può amministrare l'oggetto.
    pub async fn list_direct(
        &self,
        ctx: &AuthContext,
        object: ObjectType,
        object_id: Uuid,
    ) -> Result<Vec<PermissionGrantView>, DbError> {
        self.assert_can_manage(ctx, object, object_id).await?;
        let rows: Vec<(Uuid, String, Uuid, String, bool)> = sqlx::query_as(
            "SELECT id, subject_type, subject_id, role, inherit \
               FROM permissions \
              WHERE object_type = $1 AND object_id = $2 \
              ORDER BY created_at",
        )
        .bind(object.as_str())
        .bind(object_id)
        .fetch_all(self.db.pool())
        .await?;
        rows.into_iter()
            .map(|(id, st, sid, role, inherit)| {
                let role = ObjectRole::parse(&role)
                    .ok_or_else(|| crate::row::corrupted("permission role", &role))?;
                Ok(PermissionGrantView {
                    id,
                    subject_type: st,
                    subject_id: sid,
                    role,
                    inherit,
                    inherited: false,
                })
            })
            .collect()
    }

    /// Revoca un permesso per id.
    ///
    /// # Errors
    /// `Forbidden` se non autorizzato; `NotFound` se il permesso non esiste.
    pub async fn revoke(&self, ctx: &AuthContext, permission_id: Uuid) -> Result<(), DbError> {
        let row: Option<(String, Uuid, String, Uuid)> = sqlx::query_as(
            "SELECT object_type, object_id, subject_type, subject_id FROM permissions WHERE id = $1",
        )
        .bind(permission_id)
        .fetch_optional(self.db.pool())
        .await?;
        let Some((object_type, object_id, subject_type, subject_id)) = row else {
            return Err(DbError::NotFound);
        };
        let object = parse_object_type(&object_type)?;
        self.assert_can_manage(ctx, object, object_id).await?;
        let n = sqlx::query("DELETE FROM permissions WHERE id = $1")
            .bind(permission_id)
            .execute(self.db.pool())
            .await?
            .rows_affected();
        if n == 0 {
            return Err(DbError::NotFound);
        }
        self.invalidate_subject_scope(parse_subject_type(&subject_type)?, subject_id)
            .await?;
        Ok(())
    }

    async fn invalidate_subject_scope(
        &self,
        subject: SubjectType,
        subject_id: Uuid,
    ) -> Result<(), DbError> {
        match subject {
            SubjectType::User => {
                self.db
                    .invalidate_permission_cache_for_user(UserId::from_uuid(subject_id))
                    .await;
            }
            SubjectType::Group => {
                let members: Vec<Uuid> =
                    sqlx::query_scalar("SELECT user_id FROM group_members WHERE group_id = $1")
                        .bind(subject_id)
                        .fetch_all(self.db.pool())
                        .await?;
                for member in members {
                    self.db
                        .invalidate_permission_cache_for_user(UserId::from_uuid(member))
                        .await;
                }
            }
        }
        Ok(())
    }

    /// Aggiorna ruolo o ereditarietà.
    ///
    /// # Errors
    /// `NotFound` se il permesso non esiste; `Forbidden` se il chiamante
    /// non può gestire l'oggetto; `Connection` su errore DB.
    pub async fn patch(
        &self,
        ctx: &AuthContext,
        permission_id: Uuid,
        role: Option<ObjectRole>,
        inherit: Option<bool>,
    ) -> Result<Permission, DbError> {
        let row: Option<(String, Uuid, String, Uuid, String, bool)> = sqlx::query_as(
            "SELECT object_type, object_id, subject_type, subject_id, role, inherit \
               FROM permissions WHERE id = $1",
        )
        .bind(permission_id)
        .fetch_optional(self.db.pool())
        .await?;
        let Some((object_type, object_id, subject_type, subject_id, current_role, current_inherit)) =
            row
        else {
            return Err(DbError::NotFound);
        };
        let object = parse_object_type(&object_type)?;
        self.assert_can_manage(ctx, object, object_id).await?;
        let new_role = role.map_or(current_role, |r| r.as_str().to_owned());
        let new_inherit = inherit.unwrap_or(current_inherit);
        let out: (Uuid, String, bool) = sqlx::query_as(
            "UPDATE permissions SET role = $2, inherit = $3 WHERE id = $1 \
             RETURNING id, role, inherit",
        )
        .bind(permission_id)
        .bind(&new_role)
        .bind(new_inherit)
        .fetch_one(self.db.pool())
        .await?;
        let role = ObjectRole::parse(&out.1)
            .ok_or_else(|| crate::row::corrupted("permission role", &out.1))?;
        self.invalidate_subject_scope(parse_subject_type(&subject_type)?, subject_id)
            .await?;
        Ok(Permission {
            id: out.0,
            role,
            inherit: out.2,
        })
    }

    /// Spiega perché un utente vede (o non vede) un oggetto, con nomi e
    /// percorsi leggibili — mai gli id grezzi (spec §3.1).
    ///
    /// ponytail: elenca i grant che matchano (inherit sull'antenato, o
    /// esatto). Non replica i "buchi" `inherit=false` di `VisibilityScope`.
    /// Soffitto: un antenato con inherit può comparire anche sotto un nodo
    /// non ereditario. Upgrade: stessa regola di visibilità.
    ///
    /// # Errors
    /// `Forbidden` se il chiamante non può gestire l'oggetto; `Connection` DB.
    pub async fn explain(
        &self,
        ctx: &AuthContext,
        object: ObjectType,
        object_id: Uuid,
        user_id: Uuid,
    ) -> Result<ExplainResult, DbError> {
        self.assert_can_manage(ctx, object, object_id).await?;

        let user: Option<(String, String)> =
            sqlx::query_as("SELECT display_name, role FROM users WHERE id = $1")
                .bind(user_id)
                .fetch_optional(self.db.pool())
                .await?;
        let Some((display_name, system_role)) = user else {
            return Ok(ExplainResult {
                granted: false,
                chain: Vec::new(),
            });
        };

        let target_name = object_display_name(self.db, object, object_id).await?;

        if system_role == "admin" {
            return Ok(ExplainResult {
                granted: true,
                chain: vec![ExplainChainLink {
                    subject_type: "user".to_owned(),
                    subject_name: display_name,
                    role: "admin".to_owned(),
                    granted_on_type: object.as_str().to_owned(),
                    granted_on_name: target_name,
                    inherited_in: None,
                }],
            });
        }

        if object == ObjectType::Folder
            && let Some(owner) = folder_owner(self.db, object_id).await?
            && owner == user_id
        {
            return Ok(ExplainResult {
                granted: true,
                chain: vec![ExplainChainLink {
                    subject_type: "user".to_owned(),
                    subject_name: display_name,
                    role: "owner".to_owned(),
                    granted_on_type: object.as_str().to_owned(),
                    granted_on_name: target_name,
                    inherited_in: None,
                }],
            });
        }

        let rows = match object {
            ObjectType::Folder => folder_grants(self.db, object_id, user_id).await?,
            ObjectType::Album | ObjectType::Asset => {
                direct_grants(self.db, object, object_id, user_id).await?
            }
        };

        let mut chain = Vec::new();
        for (subject_type, subject_id, role, granted_on_id) in rows {
            let subject_name = subject_display_name(self.db, &subject_type, subject_id).await?;
            let granted_on_name = object_display_name(self.db, object, granted_on_id).await?;
            let inherited_in = if granted_on_id == object_id {
                None
            } else {
                Some(target_name.clone())
            };
            chain.push(ExplainChainLink {
                subject_type,
                subject_name,
                role,
                granted_on_type: object.as_str().to_owned(),
                granted_on_name,
                inherited_in,
            });
        }

        Ok(ExplainResult {
            granted: !chain.is_empty(),
            chain,
        })
    }

    /// Tutto ciò che è condiviso **con** l'utente corrente (cartelle e
    /// album), l'inverso di `list_direct` — quella è interrogabile solo per
    /// oggetto. Un accesso via gruppo compare con `via_group` impostato,
    /// non solo quelli concessi direttamente all'utente.
    ///
    /// Se lo stesso oggetto arriva sia da un permesso diretto che da un
    /// gruppo, vince il ruolo più alto e l'origine mostrata è quella
    /// diretta — l'utente ha comunque accesso a titolo personale.
    ///
    /// # Errors
    /// `Forbidden` senza utente autenticato; `Connection` se una query
    /// fallisce.
    pub async fn list_shared_with_me(
        &self,
        ctx: &AuthContext,
    ) -> Result<Vec<SharedWithMeItem>, DbError> {
        let user_id = ctx.user_id().ok_or(DbError::Forbidden)?;

        let rows: Vec<GrantRow> = sqlx::query_as(
            "SELECT object_type, object_id, role, subject_type, subject_id \
               FROM permissions \
              WHERE object_type IN ('folder', 'album') \
                AND ( \
                     (subject_type = 'user' AND subject_id = $1) \
                  OR (subject_type = 'group' AND subject_id IN ( \
                        SELECT group_id FROM group_members WHERE user_id = $1 \
                     )) \
                )",
        )
        .bind(user_id.as_uuid())
        .fetch_all(self.db.pool())
        .await?;

        let mut by_object: HashMap<Uuid, Aggregated> = HashMap::new();
        for row in rows {
            let role = ObjectRole::parse(&row.role)
                .ok_or_else(|| crate::row::corrupted("permission role", &row.role))?;
            let entry = by_object
                .entry(row.object_id)
                .or_insert_with(|| Aggregated {
                    object_type: row.object_type.clone(),
                    role,
                    direct: false,
                    group_ids: Vec::new(),
                });
            entry.role = entry.role.max(role);
            if row.subject_type == "user" {
                entry.direct = true;
            } else {
                entry.group_ids.push(row.subject_id);
            }
        }

        let folder_ids: Vec<Uuid> = by_object
            .iter()
            .filter(|(_, a)| a.object_type == ObjectType::Folder.as_str())
            .map(|(id, _)| *id)
            .collect();
        let album_ids: Vec<Uuid> = by_object
            .iter()
            .filter(|(_, a)| a.object_type == ObjectType::Album.as_str())
            .map(|(id, _)| *id)
            .collect();
        let group_ids: Vec<Uuid> = by_object
            .values()
            .flat_map(|a| a.group_ids.iter().copied())
            .collect();

        let folders = folder_name_and_owner(self.db, &folder_ids).await?;
        let albums = album_name_and_owner(self.db, &album_ids).await?;
        let owner_ids: Vec<Uuid> = folders
            .values()
            .chain(albums.values())
            .map(|(_, owner)| *owner)
            .collect();
        let owner_names = display_names(self.db, &owner_ids).await?;
        let group_names = group_names(self.db, &group_ids).await?;
        let folder_counts = folder_asset_counts(self.db, &folder_ids).await?;
        let album_counts = album_asset_counts(self.db, &album_ids).await?;

        let mut items: Vec<SharedWithMeItem> = Vec::new();
        for (object_id, aggregated) in by_object {
            let (name, owner_id, item_count) =
                if aggregated.object_type == ObjectType::Folder.as_str() {
                    let Some((name, owner_id)) = folders.get(&object_id) else {
                        continue;
                    };
                    let count = folder_counts.get(&object_id).copied().unwrap_or(0);
                    (name.clone(), *owner_id, count)
                } else {
                    let Some((name, owner_id)) = albums.get(&object_id) else {
                        continue;
                    };
                    let count = album_counts.get(&object_id).copied().unwrap_or(0);
                    (name.clone(), *owner_id, count)
                };
            let via_group = if aggregated.direct {
                None
            } else {
                aggregated
                    .group_ids
                    .first()
                    .and_then(|id| group_names.get(id).cloned())
            };
            items.push(SharedWithMeItem {
                object_type: aggregated.object_type,
                object_id,
                name,
                owner_name: owner_names.get(&owner_id).cloned().unwrap_or_default(),
                role: aggregated.role,
                via_group,
                item_count,
            });
        }
        items.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(items)
    }

    async fn assert_can_manage(
        &self,
        ctx: &AuthContext,
        object: ObjectType,
        object_id: Uuid,
    ) -> Result<(), DbError> {
        if ctx.is_admin() {
            return Ok(());
        }
        let Some(user_id) = ctx.user_id() else {
            return Err(DbError::Forbidden);
        };
        let owner: Option<uuid::Uuid> = match object {
            ObjectType::Folder => {
                sqlx::query_scalar(
                    "SELECT l.owner_id FROM folders f \
                      JOIN libraries l ON l.id = f.library_id \
                     WHERE f.id = $1",
                )
                .bind(object_id)
                .fetch_optional(self.db.pool())
                .await?
            }
            ObjectType::Album => {
                sqlx::query_scalar("SELECT owner_id FROM albums WHERE id = $1")
                    .bind(object_id)
                    .fetch_optional(self.db.pool())
                    .await?
            }
            ObjectType::Asset => {
                sqlx::query_scalar(
                    "SELECT l.owner_id FROM assets a \
                      JOIN folders f ON f.id = a.folder_id \
                      JOIN libraries l ON l.id = f.library_id \
                     WHERE a.id = $1",
                )
                .bind(object_id)
                .fetch_optional(self.db.pool())
                .await?
            }
        };
        match owner {
            Some(owner) if owner == user_id.as_uuid() => Ok(()),
            Some(_) | None => Err(DbError::Forbidden),
        }
    }
}

/// Un oggetto (cartella o album) condiviso con l'utente corrente.
#[derive(Debug, Clone, Serialize)]
pub struct SharedWithMeItem {
    pub object_type: String,
    pub object_id: Uuid,
    pub name: String,
    pub owner_name: String,
    pub role: ObjectRole,
    /// `Some(nome del gruppo)` se l'accesso arriva solo da un gruppo;
    /// `None` se l'utente ha (anche) un permesso diretto.
    pub via_group: Option<String>,
    pub item_count: i64,
}

#[derive(sqlx::FromRow)]
struct GrantRow {
    object_type: String,
    object_id: Uuid,
    role: String,
    subject_type: String,
    subject_id: Uuid,
}

struct Aggregated {
    object_type: String,
    role: ObjectRole,
    direct: bool,
    group_ids: Vec<Uuid>,
}

async fn folder_name_and_owner(
    db: &Db,
    folder_ids: &[Uuid],
) -> Result<HashMap<Uuid, (String, Uuid)>, DbError> {
    if folder_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows: Vec<(Uuid, String, Uuid)> = sqlx::query_as(
        "SELECT f.id, f.name, l.owner_id \
           FROM folders f JOIN libraries l ON l.id = f.library_id \
          WHERE f.id = ANY($1)",
    )
    .bind(folder_ids)
    .fetch_all(db.pool())
    .await?;
    Ok(rows
        .into_iter()
        .map(|(id, name, owner)| (id, (name, owner)))
        .collect())
}

async fn album_name_and_owner(
    db: &Db,
    album_ids: &[Uuid],
) -> Result<HashMap<Uuid, (String, Uuid)>, DbError> {
    if album_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows: Vec<(Uuid, String, Uuid)> =
        sqlx::query_as("SELECT id, name, owner_id FROM albums WHERE id = ANY($1)")
            .bind(album_ids)
            .fetch_all(db.pool())
            .await?;
    Ok(rows
        .into_iter()
        .map(|(id, name, owner)| (id, (name, owner)))
        .collect())
}

async fn display_names(db: &Db, user_ids: &[Uuid]) -> Result<HashMap<Uuid, String>, DbError> {
    if user_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows: Vec<(Uuid, String)> =
        sqlx::query_as("SELECT id, display_name FROM users WHERE id = ANY($1)")
            .bind(user_ids)
            .fetch_all(db.pool())
            .await?;
    Ok(rows.into_iter().collect())
}

async fn group_names(db: &Db, group_ids: &[Uuid]) -> Result<HashMap<Uuid, String>, DbError> {
    if group_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows: Vec<(Uuid, String)> =
        sqlx::query_as("SELECT id, name FROM groups WHERE id = ANY($1)")
            .bind(group_ids)
            .fetch_all(db.pool())
            .await?;
    Ok(rows.into_iter().collect())
}

#[derive(Debug, Clone, Serialize)]
pub struct PermissionGrantView {
    pub id: Uuid,
    pub subject_type: String,
    pub subject_id: Uuid,
    pub role: ObjectRole,
    pub inherit: bool,
    pub inherited: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExplainResult {
    pub granted: bool,
    pub chain: Vec<ExplainChainLink>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExplainChainLink {
    pub subject_type: String,
    pub subject_name: String,
    pub role: String,
    pub granted_on_type: String,
    pub granted_on_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inherited_in: Option<String>,
}

fn parse_object_type(raw: &str) -> Result<ObjectType, DbError> {
    match raw {
        "folder" => Ok(ObjectType::Folder),
        "album" => Ok(ObjectType::Album),
        "asset" => Ok(ObjectType::Asset),
        other => Err(crate::row::corrupted("object_type", other)),
    }
}

fn parse_subject_type(raw: &str) -> Result<SubjectType, DbError> {
    match raw {
        "user" => Ok(SubjectType::User),
        "group" => Ok(SubjectType::Group),
        other => Err(crate::row::corrupted("subject_type", other)),
    }
}

async fn object_display_name(
    db: &Db,
    object: ObjectType,
    object_id: Uuid,
) -> Result<String, DbError> {
    match object {
        ObjectType::Folder => folder_display_path(db, object_id).await,
        ObjectType::Album => {
            let name: Option<String> = sqlx::query_scalar("SELECT name FROM albums WHERE id = $1")
                .bind(object_id)
                .fetch_optional(db.pool())
                .await?;
            Ok(name.unwrap_or_else(|| "album".to_owned()))
        }
        ObjectType::Asset => {
            let name: Option<String> =
                sqlx::query_scalar("SELECT filename FROM assets WHERE id = $1")
                    .bind(object_id)
                    .fetch_optional(db.pool())
                    .await?;
            Ok(name.unwrap_or_else(|| "asset".to_owned()))
        }
    }
}

async fn folder_display_path(db: &Db, folder_id: Uuid) -> Result<String, DbError> {
    let library: Option<String> = sqlx::query_scalar(
        "SELECT l.name FROM folders f JOIN libraries l ON l.id = f.library_id \
          WHERE f.id = $1",
    )
    .bind(folder_id)
    .fetch_optional(db.pool())
    .await?;
    let Some(library) = library else {
        return Ok("folder".to_owned());
    };
    let names: Vec<String> = sqlx::query_scalar(
        "WITH RECURSIVE up AS ( \
             SELECT id, parent_id, name, 0 AS lvl FROM folders WHERE id = $1 \
             UNION ALL \
             SELECT p.id, p.parent_id, p.name, up.lvl + 1 \
               FROM folders p JOIN up ON p.id = up.parent_id \
         ) \
         SELECT name FROM up WHERE name <> '' ORDER BY lvl DESC",
    )
    .bind(folder_id)
    .fetch_all(db.pool())
    .await?;
    let mut out = format!("/{library}");
    for name in names {
        out.push('/');
        out.push_str(&name);
    }
    Ok(out)
}

async fn folder_owner(db: &Db, folder_id: Uuid) -> Result<Option<Uuid>, DbError> {
    sqlx::query_scalar(
        "SELECT l.owner_id FROM folders f JOIN libraries l ON l.id = f.library_id \
          WHERE f.id = $1",
    )
    .bind(folder_id)
    .fetch_optional(db.pool())
    .await
    .map_err(DbError::from)
}

async fn subject_display_name(
    db: &Db,
    subject_type: &str,
    subject_id: Uuid,
) -> Result<String, DbError> {
    let name: Option<String> = if subject_type == "group" {
        sqlx::query_scalar("SELECT name FROM groups WHERE id = $1")
            .bind(subject_id)
            .fetch_optional(db.pool())
            .await?
    } else {
        sqlx::query_scalar("SELECT display_name FROM users WHERE id = $1")
            .bind(subject_id)
            .fetch_optional(db.pool())
            .await?
    };
    Ok(name.unwrap_or_else(|| subject_type.to_owned()))
}

async fn folder_grants(
    db: &Db,
    folder_id: Uuid,
    user_id: Uuid,
) -> Result<Vec<(String, Uuid, String, Uuid)>, DbError> {
    let target_path: Option<String> =
        sqlx::query_scalar("SELECT path::text FROM folders WHERE id = $1")
            .bind(folder_id)
            .fetch_optional(db.pool())
            .await?;
    let Some(target_path) = target_path else {
        return Ok(Vec::new());
    };
    sqlx::query_as(
        "SELECT p.subject_type, p.subject_id, p.role, p.object_id \
           FROM permissions p \
           JOIN folders f ON f.id = p.object_id \
          WHERE p.object_type = 'folder' \
            AND ( \
                 (p.subject_type = 'user' AND p.subject_id = $1) \
              OR (p.subject_type = 'group' AND p.subject_id IN ( \
                    SELECT group_id FROM group_members WHERE user_id = $1 \
                 )) \
            ) \
            AND ( \
                 (p.inherit AND $2::ltree <@ f.path) \
              OR (NOT p.inherit AND p.object_id = $3) \
            ) \
          ORDER BY nlevel(f.path) DESC",
    )
    .bind(user_id)
    .bind(&target_path)
    .bind(folder_id)
    .fetch_all(db.pool())
    .await
    .map_err(DbError::from)
}

async fn direct_grants(
    db: &Db,
    object: ObjectType,
    object_id: Uuid,
    user_id: Uuid,
) -> Result<Vec<(String, Uuid, String, Uuid)>, DbError> {
    sqlx::query_as(
        "SELECT p.subject_type, p.subject_id, p.role, p.object_id \
           FROM permissions p \
          WHERE p.object_type = $2 AND p.object_id = $3 \
            AND ( \
                 (p.subject_type = 'user' AND p.subject_id = $1) \
              OR (p.subject_type = 'group' AND p.subject_id IN ( \
                    SELECT group_id FROM group_members WHERE user_id = $1 \
                 )) \
            )",
    )
    .bind(user_id)
    .bind(object.as_str())
    .bind(object_id)
    .fetch_all(db.pool())
    .await
    .map_err(DbError::from)
}
