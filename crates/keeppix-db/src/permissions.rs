//! Permessi solo-allow. I gruppi si risolvono con un join, mai nel token.

use keeppix_domain::{AuthContext, FolderId, ObjectRole};
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
}
