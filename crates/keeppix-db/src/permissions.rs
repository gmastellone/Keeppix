//! Permessi solo-allow. I gruppi si risolvono con un join, mai nel token.

use keeppix_domain::{AssetId, AuthContext, FolderId, ObjectRole, UserId};
use serde::Serialize;
use uuid::Uuid;

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
        if asset_ids.is_empty() || ctx.is_admin() {
            return Ok(());
        }
        let ids: Vec<Uuid> = asset_ids.iter().map(AssetId::as_uuid).collect();
        let rows: Vec<(Uuid, Uuid)> = sqlx::query_as(
            "SELECT DISTINCT a.folder_id, l.owner_id \
               FROM assets a \
               JOIN folders f ON f.id = a.folder_id \
               JOIN libraries l ON l.id = f.library_id \
              WHERE a.id = ANY($1)",
        )
        .bind(&ids)
        .fetch_all(self.db.pool())
        .await?;

        for (folder_id, owner_id) in rows {
            if ctx.user_id() == Some(UserId::from_uuid(owner_id)) {
                continue;
            }
            match self
                .effective_role(ctx, FolderId::from_uuid(folder_id))
                .await?
            {
                Some(ObjectRole::Editor) => {}
                _ => return Err(DbError::Forbidden),
            }
        }
        Ok(())
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
        let row: Option<(String, Uuid)> =
            sqlx::query_as("SELECT object_type, object_id FROM permissions WHERE id = $1")
                .bind(permission_id)
                .fetch_optional(self.db.pool())
                .await?;
        let Some((object_type, object_id)) = row else {
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
        let row: Option<(String, Uuid, String, bool)> = sqlx::query_as(
            "SELECT object_type, object_id, role, inherit FROM permissions WHERE id = $1",
        )
        .bind(permission_id)
        .fetch_optional(self.db.pool())
        .await?;
        let Some((object_type, object_id, current_role, current_inherit)) = row else {
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
        Ok(Permission {
            id: out.0,
            role,
            inherit: out.2,
        })
    }

    /// Spiega perché un utente vede (o non vede) un oggetto.
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
        // ponytail: explain minimale — granted true se esiste un permesso diretto
        // o ownership; catena completa in iterazione futura.
        let direct: Option<String> = sqlx::query_scalar(
            "SELECT role FROM permissions \
              WHERE object_type = $1 AND object_id = $2 \
                AND subject_type = 'user' AND subject_id = $3 \
              LIMIT 1",
        )
        .bind(object.as_str())
        .bind(object_id)
        .bind(user_id)
        .fetch_optional(self.db.pool())
        .await?;
        let granted = direct.is_some();
        let chain = direct
            .map(|role| ExplainChainLink {
                subject_type: "user".to_owned(),
                subject_name: user_id.to_string(),
                role,
                granted_on_type: object.as_str().to_owned(),
                granted_on_name: object_id.to_string(),
            })
            .into_iter()
            .collect();
        Ok(ExplainResult { granted, chain })
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
}

fn parse_object_type(raw: &str) -> Result<ObjectType, DbError> {
    match raw {
        "folder" => Ok(ObjectType::Folder),
        "album" => Ok(ObjectType::Album),
        "asset" => Ok(ObjectType::Asset),
        other => Err(crate::row::corrupted("object_type", other)),
    }
}
