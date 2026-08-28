//! User group management. Only an admin can create, modify, or delete groups.

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

    /// List of all groups. Admin only.
    ///
    /// # Errors
    /// `DbError::Forbidden` if not admin.
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

    /// Creates a new group. Admin only. The name is unique case-insensitively (409 on conflict).
    ///
    /// # Errors
    /// `DbError::Forbidden` if not admin; `DbError::Conflict` if the name already exists.
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

    /// Renames a group. Admin only. 409 if the new name is already in use.
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

    /// Deletes a group. Admin only.
    ///
    /// If the group has active permissions and `cascade = false`, returns
    /// `DbError::Conflict`. If `cascade = true`, deletes the permissions
    /// first, then the group.
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

        // Check existence
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

    /// List of a group's members. Admin only.
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

    /// Adds a user to a group. Admin only. Idempotent (ON CONFLICT DO NOTHING).
    ///
    /// # Errors
    /// `DbError::Forbidden` / `DbError::NotFound` if the group does not exist.
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
        // Even if the user does not exist, the FK will reject it with a
        // referential-integrity error — treated as Connection, not
        // NotFound, because the client already gets that check from the
        // HTTP layer (404 on a nonexistent UserId comes from UserRepo).
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

    /// Removes a user from a group. Admin only. Does not delete the group
    /// if it ends up with no members.
    ///
    /// # Errors
    /// `DbError::Forbidden` / `DbError::NotFound` if the group does not exist.
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
