//! Groups of PHOTOGRAPHED PEOPLE. Not to be confused with `groups`, which
//! are *user* groups for permissions: similar names, distinct concepts,
//! separate tables on purpose.
//!
//! Pure CRUD over already-identified people: no computation, no AI. A
//! person can belong to multiple groups.

use chrono::{DateTime, Utc};
use keeppix_domain::{AuthContext, PersonGroup, PersonGroupId, PersonId};

use crate::{Db, DbError, PersonRepo};

#[derive(Debug, sqlx::FromRow)]
struct GroupRow {
    id: uuid::Uuid,
    name: String,
    created_by: uuid::Uuid,
    created_at: DateTime<Utc>,
}

impl GroupRow {
    fn into_domain(self) -> PersonGroup {
        PersonGroup {
            id: PersonGroupId::from_uuid(self.id),
            name: self.name,
            created_by: keeppix_domain::UserId::from_uuid(self.created_by),
            created_at: self.created_at,
        }
    }
}

const COLUMNS: &str = "id, name, created_by, created_at";

#[derive(Debug, Clone)]
pub struct NewPersonGroup {
    pub name: String,
}

pub struct PersonGroupRepo<'a> {
    db: &'a Db,
}

impl<'a> PersonGroupRepo<'a> {
    #[must_use]
    pub const fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// # Errors
    /// `Forbidden` without an authenticated user. `Conflict` if the name
    /// is already in use.
    pub async fn create(
        &self,
        ctx: &AuthContext,
        new: NewPersonGroup,
    ) -> Result<PersonGroup, DbError> {
        let Some(user_id) = ctx.user_id() else {
            return Err(DbError::Forbidden);
        };
        let row: GroupRow = sqlx::query_as(&format!(
            "INSERT INTO person_groups (id, name, created_by) VALUES ($1, $2, $3) \
             RETURNING {COLUMNS}"
        ))
        .bind(PersonGroupId::new().as_uuid())
        .bind(&new.name)
        .bind(user_id.as_uuid())
        .fetch_one(self.db.pool())
        .await
        .map_err(map_name_conflict)?;
        Ok(row.into_domain())
    }

    /// All groups: a group is navigation metadata, not sensitive data —
    /// its mere existence (name "Family") does not reveal which photos it
    /// contains, so it does not need to be filtered by visibility the way
    /// people themselves are. The list of people inside it, on the other
    /// hand, goes through [`PersonRepo::find_by_id`] downstream.
    ///
    /// # Errors
    /// `Forbidden` without an authenticated user.
    pub async fn list(&self, ctx: &AuthContext) -> Result<Vec<PersonGroup>, DbError> {
        if ctx.user_id().is_none() {
            return Err(DbError::Forbidden);
        }
        let rows: Vec<GroupRow> = sqlx::query_as(&format!(
            "SELECT {COLUMNS} FROM person_groups ORDER BY name"
        ))
        .fetch_all(self.db.pool())
        .await?;
        Ok(rows.into_iter().map(GroupRow::into_domain).collect())
    }

    /// # Errors
    /// `Forbidden` without an authenticated user. `NotFound` if the name
    /// is already in use or the group does not exist.
    pub async fn rename(
        &self,
        ctx: &AuthContext,
        id: PersonGroupId,
        name: &str,
    ) -> Result<PersonGroup, DbError> {
        if ctx.user_id().is_none() {
            return Err(DbError::Forbidden);
        }
        let row: Option<GroupRow> = sqlx::query_as(&format!(
            "UPDATE person_groups SET name = $2 WHERE id = $1 RETURNING {COLUMNS}"
        ))
        .bind(id.as_uuid())
        .bind(name)
        .fetch_optional(self.db.pool())
        .await
        .map_err(map_name_conflict)?;
        row.map(GroupRow::into_domain).ok_or(DbError::NotFound)
    }

    /// # Errors
    /// `Forbidden` without an authenticated user.
    pub async fn delete(&self, ctx: &AuthContext, id: PersonGroupId) -> Result<(), DbError> {
        if ctx.user_id().is_none() {
            return Err(DbError::Forbidden);
        }
        sqlx::query("DELETE FROM person_groups WHERE id = $1")
            .bind(id.as_uuid())
            .execute(self.db.pool())
            .await?;
        Ok(())
    }

    /// Adds a person to the group. Checks that the caller can see at
    /// least one face of the person, reusing [`PersonRepo::find_by_id`] —
    /// otherwise the existence of an invisible person could be discovered
    /// by composing a group around them.
    ///
    /// # Errors
    /// Same as [`PersonRepo::find_by_id`]. `Forbidden` without an
    /// authenticated user for the rest of the group check.
    pub async fn add_member(
        &self,
        ctx: &AuthContext,
        group_id: PersonGroupId,
        person_id: PersonId,
    ) -> Result<(), DbError> {
        if ctx.user_id().is_none() {
            return Err(DbError::Forbidden);
        }
        PersonRepo::new(self.db).find_by_id(ctx, person_id).await?;
        sqlx::query(
            "INSERT INTO person_group_members (group_id, person_id) VALUES ($1, $2) \
             ON CONFLICT DO NOTHING",
        )
        .bind(group_id.as_uuid())
        .bind(person_id.as_uuid())
        .execute(self.db.pool())
        .await?;
        Ok(())
    }

    /// # Errors
    /// `Forbidden` without an authenticated user.
    pub async fn remove_member(
        &self,
        ctx: &AuthContext,
        group_id: PersonGroupId,
        person_id: PersonId,
    ) -> Result<(), DbError> {
        if ctx.user_id().is_none() {
            return Err(DbError::Forbidden);
        }
        sqlx::query("DELETE FROM person_group_members WHERE group_id = $1 AND person_id = $2")
            .bind(group_id.as_uuid())
            .bind(person_id.as_uuid())
            .execute(self.db.pool())
            .await?;
        Ok(())
    }

    /// People in the group, filtered by the caller's visibility (a person
    /// in the group they cannot see stays invisible here too).
    ///
    /// # Errors
    /// `Forbidden` without an authenticated user.
    pub async fn members(
        &self,
        ctx: &AuthContext,
        group_id: PersonGroupId,
    ) -> Result<Vec<PersonId>, DbError> {
        if ctx.user_id().is_none() {
            return Err(DbError::Forbidden);
        }
        let ids: Vec<(uuid::Uuid,)> =
            sqlx::query_as("SELECT person_id FROM person_group_members WHERE group_id = $1")
                .bind(group_id.as_uuid())
                .fetch_all(self.db.pool())
                .await?;
        let person_repo = PersonRepo::new(self.db);
        let mut visible = Vec::with_capacity(ids.len());
        for (id,) in ids {
            let person_id = PersonId::from_uuid(id);
            if person_repo.find_by_id(ctx, person_id).await.is_ok() {
                visible.push(person_id);
            }
        }
        Ok(visible)
    }
}

fn map_name_conflict(err: sqlx::Error) -> DbError {
    if let sqlx::Error::Database(ref db_err) = err
        && db_err.code().as_deref() == Some("23505")
    {
        return DbError::Conflict("a person group with this name already exists".to_owned());
    }
    DbError::Connection(err)
}
