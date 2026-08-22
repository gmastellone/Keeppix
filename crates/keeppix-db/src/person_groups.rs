//! Gruppi di PERSONE FOTOGRAFATE (Fase 8 Task 6). Da non confondere con i
//! `groups` della Fase 3, che sono gruppi di *utenti* per i permessi: nomi
//! simili, concetti distinti, tabelle separate di proposito.
//!
//! CRUD puro sopra persone già identificate: nessun calcolo, nessuna IA
//! (spec §5.1). Una persona può stare in più gruppi.

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
    /// `Forbidden` senza utente autenticato. `Conflict` se il nome è già in
    /// uso.
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

    /// Tutti i gruppi: un gruppo è metadato di navigazione, non un dato
    /// sensibile — la sua sola esistenza (nome "Famiglia") non rivela quali
    /// foto contiene, quindi non ha bisogno di essere filtrato per
    /// visibilità come le persone stesse. La lista delle persone al suo
    /// interno, invece, passa da [`PersonRepo::find_by_id`] a valle.
    ///
    /// # Errors
    /// `Forbidden` senza utente autenticato.
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
    /// `Forbidden` senza utente autenticato. `NotFound` se il nome è già in
    /// uso o il gruppo non esiste.
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
    /// `Forbidden` senza utente autenticato.
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

    /// Aggiunge una persona al gruppo. Verifica che il chiamante veda
    /// almeno un volto della persona, riusando [`PersonRepo::find_by_id`] —
    /// altrimenti si potrebbe scoprire l'esistenza di una persona invisibile
    /// componendo un gruppo attorno a lei.
    ///
    /// # Errors
    /// Come [`PersonRepo::find_by_id`]. `Forbidden` senza utente
    /// autenticato per il resto del controllo sul gruppo.
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
    /// `Forbidden` senza utente autenticato.
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

    /// Persone del gruppo, filtrate sulla visibilità del chiamante (una
    /// persona nel gruppo che non vede resta invisibile anche qui).
    ///
    /// # Errors
    /// `Forbidden` senza utente autenticato.
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
