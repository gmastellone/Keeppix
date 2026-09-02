//! People: identities that persist over time across multiple faces and
//! multiple assets. Distinct from `groups` (user permissions) — see
//! [`crate::person_groups`] for groups of photographed people.
//!
//! A person has no library or folder of their own: their visibility is
//! **transitive**, through the confirmed faces that make them up. A user
//! can see a person only if they can see at least one asset the person
//! appears in — otherwise the mere existence of the person (and their
//! name) would be an information-leak channel about photos the user
//! should not see. A public link (`ctx.user_id() == None`) never sees any
//! person: faces never appear on public links.

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

/// A person with the count of confirmed faces visible to the caller, and
/// a cover photo's `content_hash`/`thumbhash` — what the People page
/// needs to render every card (name, count, thumbnail) from **one**
/// query, not one query per row plus a whole `SearchRepo::run` per card
/// just to find a representative photo (`PeopleView.vue`'s old
/// `loadCover`, one search per visible person — tens to hundreds of
/// requests on a real library, enough concurrent load against a
/// 10-connection pool to make the whole app feel slow, not just this
/// page).
#[derive(Debug, Clone, PartialEq)]
pub struct PersonSummary {
    pub person: Person,
    pub face_count: i64,
    pub cover_hash: Option<Vec<u8>>,
    pub cover_thumbhash: Option<Vec<u8>>,
}

#[derive(Debug, sqlx::FromRow)]
struct PersonSummaryRow {
    id: uuid::Uuid,
    name: Option<String>,
    cover_face_id: Option<uuid::Uuid>,
    hidden_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    face_count: i64,
    cover_hash: Option<Vec<u8>>,
    cover_thumbhash: Option<Vec<u8>>,
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
            cover_hash: self.cover_hash,
            cover_thumbhash: self.cover_thumbhash,
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

    /// Creates a person, with an optional name. Does not take an
    /// `AuthContext`: both automatic clustering (unnamed person) and a
    /// human ("new person", "split") call this, and in neither case is
    /// there yet an asset to derive visibility from — visibility
    /// validation happens later, on the faces assigned to this person.
    ///
    /// # Errors
    /// `Conflict` if the name is already used by another person.
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
    /// `NotFound` if the person does not exist. `Forbidden` if they exist
    /// but no face of theirs is visible to the caller (never `NotFound`
    /// in that case, so as not to offer an existence oracle).
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

    /// People visible to the caller, with the count of their visible
    /// confirmed faces — the People page. A public link never sees any
    /// person.
    ///
    /// # Errors
    /// `Connection` if the query fails.
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
        // `array_agg(... ORDER BY ...)[1]` picks one cover photo per person
        // straight out of the same visibility-filtered join `face_count`
        // already aggregates over — no second join/subquery needed, and
        // no risk of picking an asset the caller cannot see (unlike a
        // LATERAL subquery re-deriving its own visibility, this can only
        // ever aggregate rows already admitted by the WHERE below).
        // Preference order: the explicitly chosen cover_face_id if it's
        // among this person's confirmed faces, else the most recently
        // taken photo, else just a stable pick (fa.id) so the result
        // doesn't flap between requests.
        let rows: Vec<PersonSummaryRow> = sqlx::query_as(&format!(
            "SELECT p.id, p.name, p.cover_face_id, p.hidden_at, p.created_at, \
                    count(fa.id) AS face_count, \
                    (array_agg(a.content_hash ORDER BY \
                        (fa.id = p.cover_face_id) DESC, a.taken_at_utc DESC NULLS LAST, fa.id))[1] \
                        AS cover_hash, \
                    (array_agg(a.thumbhash ORDER BY \
                        (fa.id = p.cover_face_id) DESC, a.taken_at_utc DESC NULLS LAST, fa.id))[1] \
                        AS cover_thumbhash \
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

    /// Renames, or clears the name (`None`). An empty field is rejected
    /// upstream by [`PersonName::parse`] — the caller cannot pass an
    /// empty string, only `None` for "no name" or a non-empty
    /// `PersonName`.
    ///
    /// # Errors
    /// Same as [`Self::find_by_id`]. `Conflict` if the name is already in use.
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
        touch_assets_of_person(self.db, id).await?;
        Ok(row.into_domain())
    }

    /// Hides/shows: for strangers in the background who are not of
    /// interest but are not false positives either.
    ///
    /// # Errors
    /// Same as [`Self::find_by_id`].
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

    /// Chooses the cover: must be a face **of this person**, not rejected.
    ///
    /// # Errors
    /// Same as [`Self::find_by_id`]. `Conflict` if `face_id` does not
    /// belong to this person.
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

    /// Merges `absorbed` into `survivor`: all faces move to the surviving
    /// person, the absorbed people disappear. If `survivor` has no name,
    /// it inherits the first name found among the absorbed ones (in call
    /// order). Allowed even between already-separated people — separating
    /// is manually reversible, `person_separations` only blocks
    /// **automatic** re-merging.
    ///
    /// # Errors
    /// Same as [`Self::find_by_id`], on `survivor` and on each `absorbed`.
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
        // Before reassignment: every asset with a confirmed face still
        // pointing at an absorbed person is about to show `survivor`'s
        // name instead — the survivor's own pre-existing assets don't
        // change what they display, so they're not touched here.
        touch_assets_with_person_in(self.db, &absorbed_uuids).await?;
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

    /// Splits: the given faces leave `source` and form a new person.
    /// **Does not restore a previous state** — this must be made clear in
    /// the interface so the user does not expect it to be undoable.
    /// Records `person_separations`: the automatic clustering will never
    /// re-merge these two people again.
    ///
    /// # Errors
    /// Same as [`Self::find_by_id`] on `source`. `Conflict` if `face_ids`
    /// is empty or if one of the faces does not belong to `source`.
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
            // Rollback: delete the just-created person instead of
            // leaving an orphaned empty person around.
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

    /// `true` if this person appears in **at least one** separation — used
    /// by incremental clustering to decide whether an automatic
    /// assignment should always go to review instead of being treated as
    /// certain: implementing a margin threshold between centroids would
    /// require a second pgvector comparison per face; the rule "anyone
    /// with a history of separations always goes through review" is
    /// simpler, and never causes a silently wrong automatic assignment —
    /// only a few extra entries in the queue. Does not take an
    /// `AuthContext`: pipeline.
    ///
    /// # Errors
    /// `Connection` if the query fails.
    pub async fn has_any_separation(&self, id: PersonId) -> Result<bool, DbError> {
        let found: Option<(uuid::Uuid,)> = sqlx::query_as(
            "SELECT person_a FROM person_separations WHERE person_a = $1 OR person_b = $1 LIMIT 1",
        )
        .bind(id.as_uuid())
        .fetch_optional(self.db.pool())
        .await?;
        Ok(found.is_some())
    }

    /// Person with the nearest centroid (cosine distance) to `embedding` —
    /// the candidate for incremental clustering. `None` if no person with
    /// a centroid exists yet (the library's first person). Does not take
    /// an `AuthContext`: pipeline. The returned similarity is
    /// `1 - cosine_distance` — same convention as
    /// `AssetTagRepo::propose_for_tag` for its scores.
    ///
    /// # Errors
    /// `Connection` if the query fails (or if the faces schema does not exist).
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

    /// Recomputes the centroid as the average of the confirmed faces'
    /// embeddings (not rejected, with a computed embedding). Does not
    /// take an `AuthContext`: internal maintenance, called after every
    /// change to a person's composition.
    ///
    /// # Errors
    /// `Connection` if the query fails.
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

    /// Deletes a person: their faces remain (`person_id` goes back to
    /// `NULL`, `ON DELETE SET NULL`), ready for a new clustering pass —
    /// **not** a deletion of face data (that is "Delete all face data", a
    /// distinct and broader action).
    ///
    /// # Errors
    /// Same as [`Self::find_by_id`].
    pub async fn delete(&self, ctx: &AuthContext, id: PersonId) -> Result<(), DbError> {
        self.find_by_id(ctx, id).await?;
        sqlx::query("DELETE FROM persons WHERE id = $1")
            .bind(id.as_uuid())
            .execute(self.db.pool())
            .await?;
        Ok(())
    }
}

/// Fires `assets_change_log` for every asset with a confirmed face of
/// `person_id` — a name change is visible on every one of those tiles/
/// lightboxes (`FaceRepo::confirmed_among`), but nothing about `persons`
/// itself feeds the live-update mechanism, which only watches `assets`
/// rows. Same reasoning as [`crate::faces::FaceRepo`]'s private
/// `touch_asset_for_face`, at the scale a rename actually needs: one
/// person can be confirmed on hundreds of assets.
async fn touch_assets_of_person(db: &Db, person_id: PersonId) -> Result<(), DbError> {
    sqlx::query(
        "UPDATE assets SET updated_at = now() \
          WHERE id IN (SELECT asset_id FROM faces \
                        WHERE person_id = $1 AND rejected_at IS NULL)",
    )
    .bind(person_id.as_uuid())
    .execute(db.pool())
    .await?;
    Ok(())
}

/// Same as [`touch_assets_of_person`], for a batch of people at once —
/// [`PersonRepo::merge`]'s absorbed side.
async fn touch_assets_with_person_in(db: &Db, person_ids: &[uuid::Uuid]) -> Result<(), DbError> {
    sqlx::query(
        "UPDATE assets SET updated_at = now() \
          WHERE id IN (SELECT asset_id FROM faces \
                        WHERE person_id = ANY($1) AND rejected_at IS NULL)",
    )
    .bind(person_ids)
    .execute(db.pool())
    .await?;
    Ok(())
}

fn map_name_conflict(err: sqlx::Error) -> DbError {
    if let sqlx::Error::Database(ref db_err) = err
        && db_err.code().as_deref() == Some("23505")
    {
        return DbError::Conflict("a person with this name already exists".to_owned());
    }
    DbError::Connection(err)
}
