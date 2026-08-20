//! Gestione dei gruppi di utenti. Solo admin può creare, modificare, cancellare.

use keeppix_domain::{AuthContext, GroupId, UserId};

use crate::{Db, DbError};

pub struct GroupRepo<'a> {
    db: &'a Db,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Group {
    pub id: uuid::Uuid,
    pub name: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl Group {
    fn into_domain(self) -> GroupView {
        GroupView {
            id: GroupId::from_uuid(self.id),
            name: self.name,
            created_at: self.created_at,
        }
    }
}

#[derive(Debug, Clone)]
pub struct GroupView {
    pub id: GroupId,
    pub name: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct GroupMemberRow {
    pub user_id: uuid::Uuid,
    pub added_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone)]
pub struct GroupMember {
    pub user_id: UserId,
    pub added_at: chrono::DateTime<chrono::Utc>,
}

fn map_name_conflict(err: sqlx::Error) -> DbError {
    if let sqlx::Error::Database(ref db_err) = err
        && db_err.code().as_deref() == Some("23505")
    {
        return DbError::Conflict("group name is already in use".to_owned());
    }
    DbError::Connection(err)
}

impl<'a> GroupRepo<'a> {
    #[must_use]
    pub const fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// Elenco di tutti i gruppi. Solo admin.
    ///
    /// # Errors
    /// `DbError::Forbidden` se non admin.
    pub async fn list(&self, ctx: &AuthContext) -> Result<Vec<GroupView>, DbError> {
        if !ctx.is_admin() {
            return Err(DbError::Forbidden);
        }
        let rows: Vec<Group> =
            sqlx::query_as("SELECT id, name, created_at FROM groups ORDER BY name")
                .fetch_all(self.db.pool())
                .await?;
        Ok(rows.into_iter().map(Group::into_domain).collect())
    }

    /// Crea un nuovo gruppo. Solo admin. Il nome è unico case-insensitive (409 in conflitto).
    ///
    /// # Errors
    /// `DbError::Forbidden` se non admin; `DbError::Conflict` se il nome esiste già.
    pub async fn create(&self, ctx: &AuthContext, name: &str) -> Result<GroupView, DbError> {
        if !ctx.is_admin() {
            return Err(DbError::Forbidden);
        }
        let created_by = ctx.user_id().map(|id| id.as_uuid());
        let row: Group = sqlx::query_as(
            "INSERT INTO groups (id, name, created_by) VALUES ($1, $2, $3) \
             RETURNING id, name, created_at",
        )
        .bind(GroupId::new().as_uuid())
        .bind(name)
        .bind(created_by)
        .fetch_one(self.db.pool())
        .await
        .map_err(map_name_conflict)?;
        Ok(row.into_domain())
    }

    /// Rinomina un gruppo. Solo admin. 409 se il nuovo nome è già in uso.
    ///
    /// # Errors
    /// `DbError::Forbidden` / `DbError::NotFound` / `DbError::Conflict`.
    pub async fn update(
        &self,
        ctx: &AuthContext,
        id: GroupId,
        name: &str,
    ) -> Result<GroupView, DbError> {
        if !ctx.is_admin() {
            return Err(DbError::Forbidden);
        }
        let row: Option<Group> = sqlx::query_as(
            "UPDATE groups SET name = $2 WHERE id = $1 RETURNING id, name, created_at",
        )
        .bind(id.as_uuid())
        .bind(name)
        .fetch_optional(self.db.pool())
        .await
        .map_err(map_name_conflict)?;
        row.ok_or(DbError::NotFound).map(Group::into_domain)
    }

    /// Cancella un gruppo. Solo admin.
    ///
    /// Se il gruppo ha permessi attivi e `cascade = false`, ritorna
    /// `DbError::Conflict`. Se `cascade = true`, cancella prima i permessi poi
    /// il gruppo.
    ///
    /// # Errors
    /// `DbError::Forbidden` / `DbError::NotFound` / `DbError::Conflict`.
    pub async fn delete(
        &self,
        ctx: &AuthContext,
        id: GroupId,
        cascade: bool,
    ) -> Result<(), DbError> {
        if !ctx.is_admin() {
            return Err(DbError::Forbidden);
        }

        // Verifica esistenza
        let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM groups WHERE id = $1)")
            .bind(id.as_uuid())
            .fetch_one(self.db.pool())
            .await?;
        if !exists {
            return Err(DbError::NotFound);
        }

        if !cascade {
            let perm_count: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM permissions \
                  WHERE subject_type = 'group' AND subject_id = $1",
            )
            .bind(id.as_uuid())
            .fetch_one(self.db.pool())
            .await?;
            if perm_count > 0 {
                return Err(DbError::Conflict(
                    "group has active permissions; use cascade=true to delete them".to_owned(),
                ));
            }
        }

        let members: Vec<uuid::Uuid> =
            sqlx::query_scalar("SELECT user_id FROM group_members WHERE group_id = $1")
                .bind(id.as_uuid())
                .fetch_all(self.db.pool())
                .await?;

        let mut tx = self.db.pool().begin().await?;
        if cascade {
            sqlx::query("DELETE FROM permissions WHERE subject_type = 'group' AND subject_id = $1")
                .bind(id.as_uuid())
                .execute(&mut *tx)
                .await?;
        }
        sqlx::query("DELETE FROM groups WHERE id = $1")
            .bind(id.as_uuid())
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        for member in members {
            self.db
                .invalidate_permission_cache_for_user(UserId::from_uuid(member))
                .await;
        }
        Ok(())
    }

    /// Elenco dei membri di un gruppo. Solo admin.
    ///
    /// # Errors
    /// `DbError::Forbidden` / `DbError::NotFound`.
    pub async fn list_members(
        &self,
        ctx: &AuthContext,
        group_id: GroupId,
    ) -> Result<Vec<GroupMember>, DbError> {
        if !ctx.is_admin() {
            return Err(DbError::Forbidden);
        }
        let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM groups WHERE id = $1)")
            .bind(group_id.as_uuid())
            .fetch_one(self.db.pool())
            .await?;
        if !exists {
            return Err(DbError::NotFound);
        }
        let rows: Vec<GroupMemberRow> = sqlx::query_as(
            "SELECT user_id, added_at FROM group_members WHERE group_id = $1 ORDER BY added_at",
        )
        .bind(group_id.as_uuid())
        .fetch_all(self.db.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| GroupMember {
                user_id: UserId::from_uuid(r.user_id),
                added_at: r.added_at,
            })
            .collect())
    }

    /// Aggiunge un utente a un gruppo. Solo admin. Idempotente (ON CONFLICT DO NOTHING).
    ///
    /// # Errors
    /// `DbError::Forbidden` / `DbError::NotFound` se il gruppo non esiste.
    pub async fn add_member(
        &self,
        ctx: &AuthContext,
        group_id: GroupId,
        user_id: UserId,
    ) -> Result<(), DbError> {
        if !ctx.is_admin() {
            return Err(DbError::Forbidden);
        }
        let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM groups WHERE id = $1)")
            .bind(group_id.as_uuid())
            .fetch_one(self.db.pool())
            .await?;
        if !exists {
            return Err(DbError::NotFound);
        }
        // Anche se l'utente non esiste, la FK lo rifiuterà con un errore di
        // integrità referenziale — trattato come Connection, non come NotFound,
        // perché il client ha già il controllo dal livello HTTP (404 su UserId
        // inesistente viene dalla UserRepo).
        sqlx::query(
            "INSERT INTO group_members (group_id, user_id) VALUES ($1, $2) \
             ON CONFLICT DO NOTHING",
        )
        .bind(group_id.as_uuid())
        .bind(user_id.as_uuid())
        .execute(self.db.pool())
        .await?;
        self.db.invalidate_permission_cache_for_user(user_id).await;
        Ok(())
    }

    /// Rimuove un utente da un gruppo. Solo admin. Non cancella il gruppo se
    /// rimane senza membri.
    ///
    /// # Errors
    /// `DbError::Forbidden` / `DbError::NotFound` se il gruppo non esiste.
    pub async fn remove_member(
        &self,
        ctx: &AuthContext,
        group_id: GroupId,
        user_id: UserId,
    ) -> Result<(), DbError> {
        if !ctx.is_admin() {
            return Err(DbError::Forbidden);
        }
        let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM groups WHERE id = $1)")
            .bind(group_id.as_uuid())
            .fetch_one(self.db.pool())
            .await?;
        if !exists {
            return Err(DbError::NotFound);
        }
        sqlx::query("DELETE FROM group_members WHERE group_id = $1 AND user_id = $2")
            .bind(group_id.as_uuid())
            .bind(user_id.as_uuid())
            .execute(self.db.pool())
            .await?;
        self.db.invalidate_permission_cache_for_user(user_id).await;
        Ok(())
    }
}
