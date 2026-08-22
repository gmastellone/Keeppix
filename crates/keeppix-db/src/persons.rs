//! Persone: identità che vivono nel tempo attraverso più volti e più asset
//! (Fase 8 Task 6/7). Distinte dai `groups` della Fase 3 (permessi utenti) —
//! vedi [`crate::person_groups`] per i gruppi di persone fotografate.
//!
//! Una persona non ha una libreria o cartella propria: la sua visibilità è
//! **transitiva**, attraverso i volti confermati che la compongono. Un
//! utente vede una persona solo se vede almeno un asset in cui compare —
//! altrimenti l'esistenza stessa della persona (e il suo nome) sarebbe un
//! canale di fuga di informazione su foto che non dovrebbe vedere. Un link
//! pubblico (`ctx.user_id() == None`) non vede mai nessuna persona: spec
//! fase-8-volti.md §7, "sui link pubblici i volti non compaiono mai".

use chrono::{DateTime, Utc};
use keeppix_domain::{AuthContext, FaceId, Person, PersonId, PersonName, PersonSeparation};

use crate::visibility::VisibilityScope;
use crate::{Db, DbError};

#[derive(Debug, sqlx::FromRow)]
struct PersonRow {
    id: uuid::Uuid,
    name: Option<String>,
    cover_face_id: Option<uuid::Uuid>,
    hidden_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
}

impl PersonRow {
    fn into_domain(self) -> Person {
        Person {
            id: PersonId::from_uuid(self.id),
            name: self.name,
            cover_face_id: self.cover_face_id.map(FaceId::from_uuid),
            hidden_at: self.hidden_at,
            created_at: self.created_at,
        }
    }
}

const COLUMNS: &str = "id, name, cover_face_id, hidden_at, created_at";

/// Una persona con il conteggio dei volti confermati visibili al chiamante —
/// quanto serve alla pagina Persone (spec §5) senza un secondo giro di
/// query per riga.
#[derive(Debug, Clone, PartialEq)]
pub struct PersonSummary {
    pub person: Person,
    pub face_count: i64,
}

#[derive(Debug, sqlx::FromRow)]
struct PersonSummaryRow {
    id: uuid::Uuid,
    name: Option<String>,
    cover_face_id: Option<uuid::Uuid>,
    hidden_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    face_count: i64,
}

impl PersonSummaryRow {
    fn into_summary(self) -> PersonSummary {
        PersonSummary {
            person: Person {
                id: PersonId::from_uuid(self.id),
                name: self.name,
                cover_face_id: self.cover_face_id.map(FaceId::from_uuid),
                hidden_at: self.hidden_at,
                created_at: self.created_at,
            },
            face_count: self.face_count,
        }
    }
}

pub struct PersonRepo<'a> {
    db: &'a Db,
}

impl<'a> PersonRepo<'a> {
    #[must_use]
    pub const fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// Crea una persona, con nome opzionale. Non prende `AuthContext`: sia
    /// il raggruppamento automatico (persona senza nome, Task 5) sia un
    /// umano (Task 6 "nuova persona", Task 7 "separa") la chiamano, e in
    /// nessuno dei due casi c'è ancora un asset da cui derivare la
    /// visibilità — la validazione di visibilità arriva dopo, sui volti che
    /// si assegnano a questa persona.
    ///
    /// # Errors
    /// `Conflict` se il nome è già usato da un'altra persona.
    pub async fn create(&self, name: Option<PersonName>) -> Result<Person, DbError> {
        let row: PersonRow = sqlx::query_as(&format!(
            "INSERT INTO persons (id, name) VALUES ($1, $2) RETURNING {COLUMNS}"
        ))
        .bind(PersonId::new().as_uuid())
        .bind(name.map(PersonName::into_string))
        .fetch_one(self.db.pool())
        .await
        .map_err(map_name_conflict)?;
        Ok(row.into_domain())
    }

    /// # Errors
    /// `NotFound` se la persona non esiste. `Forbidden` se esiste ma nessun
    /// volto suo è visibile al chiamante (mai `NotFound` in quel caso, per
    /// non offrire un oracolo di esistenza).
    pub async fn find_by_id(&self, ctx: &AuthContext, id: PersonId) -> Result<Person, DbError> {
        let row: Option<PersonRow> =
            sqlx::query_as(&format!("SELECT {COLUMNS} FROM persons WHERE id = $1"))
                .bind(id.as_uuid())
                .fetch_optional(self.db.pool())
                .await?;
        let Some(row) = row else {
            return Err(DbError::NotFound);
        };
        if ctx.is_admin() {
            return Ok(row.into_domain());
        }
        if self.has_visible_face(ctx, id).await? {
            Ok(row.into_domain())
        } else {
            Err(DbError::Forbidden)
        }
    }

    async fn has_visible_face(&self, ctx: &AuthContext, id: PersonId) -> Result<bool, DbError> {
        if ctx.user_id().is_none() {
            return Ok(false);
        }
        let scope = VisibilityScope::resolve(self.db, ctx).await?;
        let filter = scope.filter("f.path", "f.library_id", "a.id", 2);
        let count: i64 = sqlx::query_scalar(&format!(
            "SELECT count(*) FROM faces fa \
             JOIN assets a ON a.id = fa.asset_id \
             JOIN folders f ON f.id = a.folder_id \
             WHERE fa.person_id = $1 AND fa.rejected_at IS NULL AND {}",
            filter.sql()
        ))
        .bind(id.as_uuid())
        .bind(filter.bind())
        .bind(filter.holes())
        .bind(filter.assets())
        .fetch_one(self.db.pool())
        .await?;
        Ok(count > 0)
    }

    /// Persone visibili al chiamante, con il conteggio dei loro volti
    /// confermati visibili — pagina Persone (spec §5). Un link pubblico non
    /// vede mai nessuna persona (spec §7).
    ///
    /// # Errors
    /// `Connection` se la query fallisce.
    pub async fn list_visible(
        &self,
        ctx: &AuthContext,
        include_hidden: bool,
    ) -> Result<Vec<PersonSummary>, DbError> {
        if ctx.user_id().is_none() {
            return Ok(Vec::new());
        }
        let scope = VisibilityScope::resolve(self.db, ctx).await?;
        let filter = scope.filter("f.path", "f.library_id", "a.id", 2);
        let rows: Vec<PersonSummaryRow> = sqlx::query_as(&format!(
            "SELECT p.id, p.name, p.cover_face_id, p.hidden_at, p.created_at, \
                    count(fa.id) AS face_count \
             FROM persons p \
             JOIN faces fa ON fa.person_id = p.id AND fa.rejected_at IS NULL \
             JOIN assets a ON a.id = fa.asset_id \
             JOIN folders f ON f.id = a.folder_id \
             WHERE ($1::bool OR p.hidden_at IS NULL) AND {} \
             GROUP BY p.id \
             ORDER BY count(fa.id) DESC, p.created_at",
            filter.sql()
        ))
        .bind(include_hidden)
        .bind(filter.bind())
        .bind(filter.holes())
        .bind(filter.assets())
        .fetch_all(self.db.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(PersonSummaryRow::into_summary)
            .collect())
    }

    /// Rinomina, o cancella il nome (`None`). Campo vuoto rifiutato a monte
    /// da [`PersonName::parse`] — il chiamante non può passare una stringa
    /// vuota, solo `None` per "nessun nome" o un `PersonName` non vuoto.
    ///
    /// # Errors
    /// Come [`Self::find_by_id`]. `Conflict` se il nome è già in uso.
    pub async fn rename(
        &self,
        ctx: &AuthContext,
        id: PersonId,
        name: Option<PersonName>,
    ) -> Result<Person, DbError> {
        self.find_by_id(ctx, id).await?;
        let row: PersonRow = sqlx::query_as(&format!(
            "UPDATE persons SET name = $2 WHERE id = $1 RETURNING {COLUMNS}"
        ))
        .bind(id.as_uuid())
        .bind(name.map(PersonName::into_string))
        .fetch_one(self.db.pool())
        .await
        .map_err(map_name_conflict)?;
        Ok(row.into_domain())
    }

    /// Nasconde/mostra: per gli sconosciuti sullo sfondo che non
    /// interessano ma non sono falsi positivi (spec §5).
    ///
    /// # Errors
    /// Come [`Self::find_by_id`].
    pub async fn set_hidden(
        &self,
        ctx: &AuthContext,
        id: PersonId,
        hidden: bool,
    ) -> Result<Person, DbError> {
        self.find_by_id(ctx, id).await?;
        let row: PersonRow = sqlx::query_as(&format!(
            "UPDATE persons SET hidden_at = CASE WHEN $2 THEN now() ELSE NULL END \
              WHERE id = $1 RETURNING {COLUMNS}"
        ))
        .bind(id.as_uuid())
        .bind(hidden)
        .fetch_one(self.db.pool())
        .await?;
        Ok(row.into_domain())
    }

    /// Sceglie la copertina: deve essere un volto **di questa persona**, non
    /// rifiutato.
    ///
    /// # Errors
    /// Come [`Self::find_by_id`]. `Conflict` se `face_id` non appartiene a
    /// questa persona.
    pub async fn set_cover(
        &self,
        ctx: &AuthContext,
        id: PersonId,
        face_id: FaceId,
    ) -> Result<Person, DbError> {
        self.find_by_id(ctx, id).await?;
        let row: Option<PersonRow> = sqlx::query_as(&format!(
            "UPDATE persons SET cover_face_id = $2 \
              WHERE id = $1 \
                AND EXISTS ( \
                  SELECT 1 FROM faces \
                   WHERE id = $2 AND person_id = $1 AND rejected_at IS NULL \
                ) \
              RETURNING {COLUMNS}"
        ))
        .bind(id.as_uuid())
        .bind(face_id.as_uuid())
        .fetch_optional(self.db.pool())
        .await?;
        row.map(PersonRow::into_domain)
            .ok_or_else(|| DbError::Conflict("face does not belong to this person".to_owned()))
    }

    /// Unisce `absorbed` in `survivor`: tutti i volti passano alla persona
    /// sopravvissuta, le persone assorbite spariscono (spec §4.2). Se
    /// `survivor` non ha nome, eredita il primo nome trovato fra gli
    /// assorbiti (in ordine di chiamata). Consentito anche fra persone già
    /// separate — separare è reversibile a mano, `person_separations`
    /// blocca solo il riaccorpamento **automatico** (spec §4.3).
    ///
    /// # Errors
    /// Come [`Self::find_by_id`], su `survivor` e su ogni `absorbed`.
    pub async fn merge(
        &self,
        ctx: &AuthContext,
        survivor: PersonId,
        absorbed: &[PersonId],
    ) -> Result<Person, DbError> {
        let mut current = self.find_by_id(ctx, survivor).await?;
        if absorbed.is_empty() {
            return Ok(current);
        }
        for &id in absorbed {
            self.find_by_id(ctx, id).await?;
        }

        if current.name.is_none() {
            for &id in absorbed {
                let name: Option<Option<String>> =
                    sqlx::query_scalar("SELECT name FROM persons WHERE id = $1")
                        .bind(id.as_uuid())
                        .fetch_optional(self.db.pool())
                        .await?;
                if let Some(Some(name)) = name {
                    current.name = Some(name);
                    break;
                }
            }
        }

        let absorbed_uuids: Vec<uuid::Uuid> = absorbed.iter().map(PersonId::as_uuid).collect();
        sqlx::query("UPDATE faces SET person_id = $1 WHERE person_id = ANY($2)")
            .bind(survivor.as_uuid())
            .bind(&absorbed_uuids)
            .execute(self.db.pool())
            .await?;
        sqlx::query("DELETE FROM persons WHERE id = ANY($1)")
            .bind(&absorbed_uuids)
            .execute(self.db.pool())
            .await?;
        sqlx::query("UPDATE persons SET name = $2 WHERE id = $1")
            .bind(survivor.as_uuid())
            .bind(&current.name)
            .execute(self.db.pool())
            .await?;

        self.recompute_centroid(survivor).await?;
        self.find_by_id(ctx, survivor).await
    }

    /// Separa: i volti indicati lasciano `source` e formano una persona
    /// nuova. **Non ripristina uno stato precedente** — è la risposta alla
    /// domanda aperta n.5 del documento funzionale, e va scritta
    /// nell'interfaccia perché l'utente non si aspetti un annullamento
    /// (spec §4.2). Registra `person_separations`: l'automatismo non
    /// riunirà mai più queste due persone (spec §4.3).
    ///
    /// # Errors
    /// Come [`Self::find_by_id`] su `source`. `Conflict` se `face_ids` è
    /// vuoto o se uno dei volti non appartiene a `source`.
    pub async fn separate(
        &self,
        ctx: &AuthContext,
        source: PersonId,
        face_ids: &[FaceId],
        new_name: Option<PersonName>,
    ) -> Result<Person, DbError> {
        let Some(user_id) = ctx.user_id() else {
            return Err(DbError::Forbidden);
        };
        self.find_by_id(ctx, source).await?;
        if face_ids.is_empty() {
            return Err(DbError::Conflict(
                "no faces selected to split off".to_owned(),
            ));
        }

        let new_person = self.create(new_name).await?;
        let face_uuids: Vec<uuid::Uuid> = face_ids.iter().map(FaceId::as_uuid).collect();
        let moved = sqlx::query(
            "UPDATE faces SET person_id = $1, assigned_by = $3, assigned_at = now() \
              WHERE id = ANY($2) AND person_id = $4",
        )
        .bind(new_person.id.as_uuid())
        .bind(&face_uuids)
        .bind(user_id.as_uuid())
        .bind(source.as_uuid())
        .execute(self.db.pool())
        .await?;
        if moved.rows_affected() != face_uuids.len() as u64 {
            // Rollback: cancella la persona appena creata invece di
            // lasciare una persona vuota orfana.
            sqlx::query("DELETE FROM persons WHERE id = $1")
                .bind(new_person.id.as_uuid())
                .execute(self.db.pool())
                .await
                .ok();
            return Err(DbError::Conflict(
                "one or more faces do not belong to the source person".to_owned(),
            ));
        }

        let (a, b) = PersonSeparation::ordered(source, new_person.id);
        sqlx::query(
            "INSERT INTO person_separations (person_a, person_b, created_by) \
             VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
        )
        .bind(a.as_uuid())
        .bind(b.as_uuid())
        .bind(user_id.as_uuid())
        .execute(self.db.pool())
        .await?;

        self.recompute_centroid(source).await?;
        self.recompute_centroid(new_person.id).await?;
        self.find_by_id(ctx, new_person.id).await
    }

    /// `true` se le due persone sono state separate a mano — l'automatismo
    /// non deve mai riunirle (spec §4.3). Non prende `AuthContext`: la
    /// consulta il raggruppamento incrementale (pipeline di sistema).
    ///
    /// # Errors
    /// `Connection` se la query fallisce.
    pub async fn is_separated(&self, a: PersonId, b: PersonId) -> Result<bool, DbError> {
        let (lo, hi) = PersonSeparation::ordered(a, b);
        let found: Option<(uuid::Uuid, uuid::Uuid)> = sqlx::query_as(
            "SELECT person_a, person_b FROM person_separations \
              WHERE person_a = $1 AND person_b = $2",
        )
        .bind(lo.as_uuid())
        .bind(hi.as_uuid())
        .fetch_optional(self.db.pool())
        .await?;
        Ok(found.is_some())
    }

    /// `true` se questa persona compare in **almeno una** separazione — usato
    /// dal raggruppamento incrementale per decidere se un'assegnazione
    /// automatica va sempre in revisione invece che essere certa (Ruling nel
    /// ledger di fase: implementare una soglia di margine fra centroidi
    /// richiederebbe un secondo confronto pgvector per ogni volto; la regola
    /// "chi ha uno storico di separazioni passa sempre dalla revisione" è più
    /// semplice, e non causa mai un'assegnazione automatica silenziosa
    /// sbagliata — solo qualche voce in più in coda). Non prende
    /// `AuthContext`: pipeline.
    ///
    /// # Errors
    /// `Connection` se la query fallisce.
    pub async fn has_any_separation(&self, id: PersonId) -> Result<bool, DbError> {
        let found: Option<(uuid::Uuid,)> = sqlx::query_as(
            "SELECT person_a FROM person_separations WHERE person_a = $1 OR person_b = $1 LIMIT 1",
        )
        .bind(id.as_uuid())
        .fetch_optional(self.db.pool())
        .await?;
        Ok(found.is_some())
    }

    /// Persona con il centroide più vicino (distanza coseno) a `embedding` —
    /// il candidato del raggruppamento incrementale (Task 5, spec §4.1).
    /// `None` se non esiste ancora nessuna persona con un centroide (prima
    /// persona della libreria). Non prende `AuthContext`: pipeline. La
    /// similarità restituita è `1 - distanza_coseno` — stessa convenzione di
    /// `AssetTagRepo::propose_for_tag` per i punteggi.
    ///
    /// # Errors
    /// `Connection` se la query fallisce (o se lo schema volti non esiste).
    pub async fn nearest_centroid(
        &self,
        embedding: &[f32],
    ) -> Result<Option<(PersonId, f32)>, DbError> {
        let literal = crate::embeddings::vector_literal(embedding);
        let row: Option<(uuid::Uuid, f32)> = sqlx::query_as(
            "SELECT id, (1.0 - (centroid <=> $1::vector))::real AS similarity \
             FROM persons \
             WHERE centroid IS NOT NULL \
             ORDER BY centroid <=> $1::vector \
             LIMIT 1",
        )
        .bind(&literal)
        .fetch_optional(self.db.pool())
        .await?;
        Ok(row.map(|(id, similarity)| (PersonId::from_uuid(id), similarity)))
    }

    /// Ricalcola il centroide come media degli embedding dei volti
    /// confermati (non rifiutati, con impronta calcolata). Non prende
    /// `AuthContext`: manutenzione interna, chiamata dopo ogni cambio di
    /// composizione della persona.
    ///
    /// # Errors
    /// `Connection` se la query fallisce.
    pub async fn recompute_centroid(&self, id: PersonId) -> Result<(), DbError> {
        sqlx::query(
            "UPDATE persons SET centroid = ( \
                SELECT AVG(embedding) FROM faces \
                 WHERE person_id = $1 AND rejected_at IS NULL AND embedding IS NOT NULL \
              ) WHERE id = $1",
        )
        .bind(id.as_uuid())
        .execute(self.db.pool())
        .await?;
        Ok(())
    }

    /// Cancella una persona: i suoi volti restano (`person_id` torna
    /// `NULL`, `ON DELETE SET NULL`), pronti per un nuovo raggruppamento —
    /// **non** una cancellazione dei dati dei volti (quella è Task 10,
    /// "Elimina tutti i dati dei volti", un'azione distinta e più ampia).
    ///
    /// # Errors
    /// Come [`Self::find_by_id`].
    pub async fn delete(&self, ctx: &AuthContext, id: PersonId) -> Result<(), DbError> {
        self.find_by_id(ctx, id).await?;
        sqlx::query("DELETE FROM persons WHERE id = $1")
            .bind(id.as_uuid())
            .execute(self.db.pool())
            .await?;
        Ok(())
    }
}

fn map_name_conflict(err: sqlx::Error) -> DbError {
    if let sqlx::Error::Database(ref db_err) = err
        && db_err.code().as_deref() == Some("23505")
    {
        return DbError::Conflict("a person with this name already exists".to_owned());
    }
    DbError::Connection(err)
}
