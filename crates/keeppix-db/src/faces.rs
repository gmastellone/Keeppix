//! Detected faces. [`Self::insert_detected`] does not take an
//! `AuthContext`: this is the detection pipeline, like
//! [`crate::EmbeddingRepo`] — not a user action.
//!
//! The human decisions ([`Self::assign`], [`Self::reject`],
//! [`Self::confirm_proposal`]) **do take** an `AuthContext`: a user must
//! not be able to act on (or learn of the existence of) a face on an
//! asset they cannot see. Once manually assigned (`assigned_by` set), a
//! face is never touched again by automatic clustering.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use keeppix_domain::{AssetId, AuthContext, Face, FaceBBox, FaceId, PersonId, UserId};

use crate::visibility::VisibilityScope;
use crate::{AssetRepo, Db, DbError};

/// A face confirmed on an asset, as returned by
/// [`FaceRepo::confirmed_among`] — just `person_id`/name, not the full
/// `faces` row (bbox/embedding/scores are not needed by the caller).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfirmedFace {
    pub person_id: PersonId,
    /// `None` for an unnamed person ("Person 4" — the fallback label is
    /// the caller's responsibility, not this layer's).
    pub person_name: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
struct ConfirmedFaceRow {
    asset_id: uuid::Uuid,
    person_id: uuid::Uuid,
    person_name: Option<String>,
}

/// Kept in sync with `keeppix_media::face::MODEL_VERSION`. Duplicated here
/// because `keeppix-db` cannot depend on `keeppix-media` (`deny.toml`) —
/// same reason as `EmbeddingRepo::MODEL_VERSION`.
pub const MODEL_VERSION: &str = "yunet+sface";

/// A candidate for detection: has a `content_hash` (so it can have a thumbnail).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingFaceScan {
    pub asset_id: AssetId,
    pub content_hash: [u8; 32],
}

#[derive(Debug, sqlx::FromRow)]
struct FaceRow {
    id: uuid::Uuid,
    asset_id: uuid::Uuid,
    bbox_x: f32,
    bbox_y: f32,
    bbox_w: f32,
    bbox_h: f32,
    landmarks: Option<serde_json::Value>,
    detect_score: f32,
    quality: Option<f32>,
    person_id: Option<uuid::Uuid>,
    assigned_by: Option<uuid::Uuid>,
    assigned_at: Option<DateTime<Utc>>,
    rejected_at: Option<DateTime<Utc>>,
    proposed_person_id: Option<uuid::Uuid>,
    proposed_score: Option<f32>,
    model_version: String,
    created_at: DateTime<Utc>,
}

impl FaceRow {
    fn into_domain(self) -> Face {
        Face {
            id: FaceId::from_uuid(self.id),
            asset_id: AssetId::from_uuid(self.asset_id),
            bbox: FaceBBox {
                x: self.bbox_x,
                y: self.bbox_y,
                w: self.bbox_w,
                h: self.bbox_h,
            },
            landmarks: self.landmarks,
            detect_score: self.detect_score,
            quality: self.quality,
            person_id: self.person_id.map(PersonId::from_uuid),
            assigned_by: self.assigned_by.map(UserId::from_uuid),
            assigned_at: self.assigned_at,
            rejected_at: self.rejected_at,
            proposed_person_id: self.proposed_person_id.map(PersonId::from_uuid),
            proposed_score: self.proposed_score,
            model_version: self.model_version,
            created_at: self.created_at,
        }
    }
}

const COLUMNS: &str = "id, asset_id, bbox_x, bbox_y, bbox_w, bbox_h, landmarks, detect_score, \
                       quality, person_id, assigned_by, assigned_at, rejected_at, \
                       proposed_person_id, proposed_score, model_version, created_at";

/// Input of a just-detected face, before any clustering. Does not include
/// `person_id`: assignment comes later, from incremental clustering or a
/// human.
#[derive(Debug, Clone)]
pub struct NewDetectedFace {
    pub asset_id: AssetId,
    pub bbox: FaceBBox,
    pub landmarks: Option<serde_json::Value>,
    pub embedding: Option<Vec<f32>>,
    pub detect_score: f32,
    pub quality: Option<f32>,
    pub model_version: String,
}

pub struct FaceRepo<'a> {
    db: &'a Db,
}

impl<'a> FaceRepo<'a> {
    #[must_use]
    pub const fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// Inserts a just-detected face, without a person. Internal pipeline:
    /// no `AuthContext`.
    ///
    /// # Errors
    /// `Connection` if the query fails (or if the faces schema does not exist).
    pub async fn insert_detected(&self, new: NewDetectedFace) -> Result<Face, DbError> {
        let embedding_literal = new
            .embedding
            .as_deref()
            .map(crate::embeddings::vector_literal);
        let row: FaceRow = sqlx::query_as(&format!(
            "INSERT INTO faces (id, asset_id, bbox_x, bbox_y, bbox_w, bbox_h, landmarks, \
                                 embedding, detect_score, quality, model_version) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8::vector, $9, $10, $11) \
             RETURNING {COLUMNS}"
        ))
        .bind(FaceId::new().as_uuid())
        .bind(new.asset_id.as_uuid())
        .bind(new.bbox.x)
        .bind(new.bbox.y)
        .bind(new.bbox.w)
        .bind(new.bbox.h)
        .bind(&new.landmarks)
        .bind(embedding_literal)
        .bind(new.detect_score)
        .bind(new.quality)
        .bind(&new.model_version)
        .fetch_one(self.db.pool())
        .await?;
        Ok(row.into_domain())
    }

    /// Faces of an asset, bounding boxes included — the photo details
    /// panel. Excludes rejected false positives.
    ///
    /// # Errors
    /// `Forbidden` if the asset is not visible to the caller.
    pub async fn list_for_asset(
        &self,
        ctx: &AuthContext,
        asset_id: AssetId,
    ) -> Result<Vec<Face>, DbError> {
        AssetRepo::new(self.db)
            .assert_visible(ctx, std::slice::from_ref(&asset_id))
            .await?;
        let rows: Vec<FaceRow> = sqlx::query_as(&format!(
            "SELECT {COLUMNS} FROM faces \
              WHERE asset_id = $1 AND rejected_at IS NULL \
              ORDER BY bbox_x"
        ))
        .bind(asset_id.as_uuid())
        .fetch_all(self.db.pool())
        .await?;
        Ok(rows.into_iter().map(FaceRow::into_domain).collect())
    }

    /// Automatically assigns a face to a person (incremental clustering):
    /// does NOT touch `assigned_by`/`assigned_at`, which remain reserved
    /// for the human decision. Does not take an `AuthContext`: pipeline.
    ///
    /// # Errors
    /// `Connection` if the query fails.
    pub async fn auto_assign(&self, face_id: FaceId, person_id: PersonId) -> Result<(), DbError> {
        let old_person = self.person_of(face_id).await?;
        sqlx::query(
            "UPDATE faces SET person_id = $2, proposed_person_id = NULL, proposed_score = NULL \
              WHERE id = $1 AND assigned_by IS NULL",
        )
        .bind(face_id.as_uuid())
        .bind(person_id.as_uuid())
        .execute(self.db.pool())
        .await?;
        self.recompute_affected_centroids(old_person, Some(person_id))
            .await
    }

    async fn person_of(&self, face_id: FaceId) -> Result<Option<PersonId>, DbError> {
        let row: Option<(Option<uuid::Uuid>,)> =
            sqlx::query_as("SELECT person_id FROM faces WHERE id = $1")
                .bind(face_id.as_uuid())
                .fetch_optional(self.db.pool())
                .await?;
        Ok(row.and_then(|(id,)| id).map(PersonId::from_uuid))
    }

    /// Recomputes the centroids of persons affected by a composition
    /// change — a face entering or leaving a person changes the average
    /// of its confirmed embeddings. Does not fail if
    /// `PersonRepo::recompute_centroid` is called twice on the same
    /// person (idempotent: it always rereads `faces` from scratch).
    async fn recompute_affected_centroids(
        &self,
        old_person: Option<PersonId>,
        new_person: Option<PersonId>,
    ) -> Result<(), DbError> {
        let person_repo = crate::PersonRepo::new(self.db);
        if let Some(id) = old_person {
            person_repo.recompute_centroid(id).await?;
        }
        if let Some(id) = new_person
            && Some(id) != old_person
        {
            person_repo.recompute_centroid(id).await?;
        }
        Ok(())
    }

    /// Proposes (without assigning) a face to a person: an uncertain
    /// distance from the nearest centroid. Goes into the review queue.
    /// Does not take an `AuthContext`: pipeline.
    ///
    /// # Errors
    /// `Connection` if the query fails.
    pub async fn propose(
        &self,
        face_id: FaceId,
        person_id: PersonId,
        score: f32,
    ) -> Result<(), DbError> {
        sqlx::query(
            "UPDATE faces SET proposed_person_id = $2, proposed_score = $3 \
              WHERE id = $1 AND assigned_by IS NULL AND person_id IS NULL",
        )
        .bind(face_id.as_uuid())
        .bind(person_id.as_uuid())
        .bind(score)
        .execute(self.db.pool())
        .await?;
        Ok(())
    }

    /// Manual assignment, from a photo's details panel or the review
    /// queue: sets `assigned_by`/`assigned_at`, and from this point on the
    /// face is never touched again by automatic clustering.
    ///
    /// # Errors
    /// `Forbidden` if the face's asset is not visible to the caller (or
    /// without an authenticated user — a public link never decides on
    /// faces). `NotFound` if the face does not exist.
    pub async fn assign(
        &self,
        ctx: &AuthContext,
        face_id: FaceId,
        person_id: PersonId,
    ) -> Result<(), DbError> {
        let Some(user_id) = ctx.user_id() else {
            return Err(DbError::Forbidden);
        };
        self.assert_face_visible(ctx, face_id).await?;
        let old_person = self.person_of(face_id).await?;

        let result = sqlx::query(
            "UPDATE faces SET person_id = $2, assigned_by = $3, assigned_at = now(), \
                               rejected_at = NULL, proposed_person_id = NULL, proposed_score = NULL \
              WHERE id = $1",
        )
        .bind(face_id.as_uuid())
        .bind(person_id.as_uuid())
        .bind(user_id.as_uuid())
        .execute(self.db.pool())
        .await?;
        if result.rows_affected() == 0 {
            return Err(DbError::NotFound);
        }
        self.recompute_affected_centroids(old_person, Some(person_id))
            .await
    }

    /// "Not a face": a permanent false positive. Disappears from review
    /// and is never proposed again by a later re-analysis.
    ///
    /// # Errors
    /// Same as [`Self::assign`].
    pub async fn reject(&self, ctx: &AuthContext, face_id: FaceId) -> Result<(), DbError> {
        let Some(user_id) = ctx.user_id() else {
            return Err(DbError::Forbidden);
        };
        self.assert_face_visible(ctx, face_id).await?;
        let old_person = self.person_of(face_id).await?;

        let result = sqlx::query(
            "UPDATE faces SET rejected_at = now(), assigned_by = $2, assigned_at = now(), \
                               person_id = NULL, proposed_person_id = NULL, proposed_score = NULL \
              WHERE id = $1",
        )
        .bind(face_id.as_uuid())
        .bind(user_id.as_uuid())
        .execute(self.db.pool())
        .await?;
        if result.rows_affected() == 0 {
            return Err(DbError::NotFound);
        }
        self.recompute_affected_centroids(old_person, None).await
    }

    async fn assert_face_visible(&self, ctx: &AuthContext, face_id: FaceId) -> Result<(), DbError> {
        let asset_id: Option<uuid::Uuid> =
            sqlx::query_scalar("SELECT asset_id FROM faces WHERE id = $1")
                .bind(face_id.as_uuid())
                .fetch_optional(self.db.pool())
                .await?;
        let Some(asset_id) = asset_id else {
            return Err(DbError::NotFound);
        };
        AssetRepo::new(self.db)
            .assert_visible(ctx, &[AssetId::from_uuid(asset_id)])
            .await
    }

    /// Candidate faces for incremental clustering: with a computed
    /// embedding, not yet linked to a person, not rejected. Does not take
    /// an `AuthContext`: system pipeline.
    ///
    /// # Errors
    /// `Connection` if the query fails.
    pub async fn list_unassigned_with_embedding(
        &self,
        model_version: &str,
        limit: i64,
    ) -> Result<Vec<Face>, DbError> {
        let rows: Vec<FaceRow> = sqlx::query_as(&format!(
            "SELECT {COLUMNS} FROM faces \
              WHERE person_id IS NULL AND rejected_at IS NULL AND assigned_by IS NULL \
                AND embedding IS NOT NULL AND model_version = $1 \
              ORDER BY created_at \
              LIMIT $2"
        ))
        .bind(model_version)
        .bind(limit)
        .fetch_all(self.db.pool())
        .await?;
        Ok(rows.into_iter().map(FaceRow::into_domain).collect())
    }

    /// The embedding of a face, for comparison against a candidate
    /// person's centroid. Does not take an `AuthContext`: pipeline.
    ///
    /// # Errors
    /// `Connection` if the query fails.
    pub async fn embedding_of(&self, face_id: FaceId) -> Result<Option<Vec<f32>>, DbError> {
        let raw: Option<(Option<String>,)> =
            sqlx::query_as("SELECT embedding::text FROM faces WHERE id = $1")
                .bind(face_id.as_uuid())
                .fetch_optional(self.db.pool())
                .await?;
        raw.and_then(|(text,)| text)
            .map(|text| crate::embeddings::parse_vector_text(&text))
            .transpose()
    }

    /// Proposed faces (uncertain assignment, pending human review),
    /// filtered by the caller's visibility — the review queue, same shape
    /// as the tag queue.
    ///
    /// # Errors
    /// `Forbidden` without an authenticated user.
    pub async fn list_proposed(&self, ctx: &AuthContext) -> Result<Vec<Face>, DbError> {
        if ctx.user_id().is_none() {
            return Err(DbError::Forbidden);
        }
        let scope = VisibilityScope::resolve(self.db, ctx).await?;
        let filter = scope.filter("f.path", "f.library_id", "a.id", 1);
        let columns: Vec<String> = COLUMNS.split(", ").map(|c| format!("fa.{c}")).collect();
        let rows: Vec<FaceRow> = sqlx::query_as(&format!(
            "SELECT {} FROM faces fa \
             JOIN assets a ON a.id = fa.asset_id \
             JOIN folders f ON f.id = a.folder_id \
             WHERE fa.proposed_person_id IS NOT NULL AND fa.person_id IS NULL \
               AND fa.rejected_at IS NULL AND {} \
             ORDER BY fa.proposed_score DESC NULLS LAST, fa.id",
            columns.join(", "),
            filter.sql()
        ))
        .bind(filter.bind())
        .bind(filter.holes())
        .bind(filter.assets())
        .fetch_all(self.db.pool())
        .await?;
        Ok(rows.into_iter().map(FaceRow::into_domain).collect())
    }

    /// Confirms a proposal: assigns the face to the proposed person, as if
    /// it were a direct human decision.
    ///
    /// # Errors
    /// `Forbidden`/`NotFound` same as [`Self::assign`]. `Conflict` if the
    /// face has no (more) pending proposal.
    pub async fn confirm_proposal(
        &self,
        ctx: &AuthContext,
        face_id: FaceId,
    ) -> Result<(), DbError> {
        let Some(user_id) = ctx.user_id() else {
            return Err(DbError::Forbidden);
        };
        self.assert_face_visible(ctx, face_id).await?;

        let target: Option<(uuid::Uuid,)> = sqlx::query_as(
            "SELECT proposed_person_id FROM faces \
              WHERE id = $1 AND proposed_person_id IS NOT NULL AND person_id IS NULL",
        )
        .bind(face_id.as_uuid())
        .fetch_optional(self.db.pool())
        .await?;
        let Some((person_id,)) = target else {
            return Err(DbError::Conflict("face has no pending proposal".to_owned()));
        };

        sqlx::query(
            "UPDATE faces SET person_id = $2, assigned_by = $3, assigned_at = now(), \
                               proposed_person_id = NULL, proposed_score = NULL \
              WHERE id = $1",
        )
        .bind(face_id.as_uuid())
        .bind(person_id)
        .bind(user_id.as_uuid())
        .execute(self.db.pool())
        .await?;
        self.recompute_affected_centroids(None, Some(PersonId::from_uuid(person_id)))
            .await
    }

    /// Number of proposed faces visible to the caller — the "faces" half
    /// of the combined `bootstrap.badges.revision` badge (the "tags" half
    /// shares the same field, not a new one).
    ///
    /// # Errors
    /// `Connection` for any error other than a missing schema.
    pub async fn count_proposed_visible(&self, ctx: &AuthContext) -> Result<i64, DbError> {
        if ctx.user_id().is_none() {
            return Ok(0);
        }
        let status = crate::pgvector::probe_pgvector(self.db).await?;
        if !status.available {
            return Ok(0);
        }
        let scope = VisibilityScope::resolve(self.db, ctx).await?;
        let filter = scope.filter("f.path", "f.library_id", "a.id", 1);
        let count: i64 = sqlx::query_scalar(&format!(
            "SELECT count(*) FROM faces fa \
             JOIN assets a ON a.id = fa.asset_id \
             JOIN folders f ON f.id = a.folder_id \
             WHERE fa.proposed_person_id IS NOT NULL AND fa.person_id IS NULL \
               AND fa.rejected_at IS NULL AND {}",
            filter.sql()
        ))
        .bind(filter.bind())
        .bind(filter.holes())
        .bind(filter.assets())
        .fetch_one(self.db.pool())
        .await?;
        Ok(count)
    }

    /// Image assets with a hash, not yet passed through the detector for
    /// `model_version`, outside the Culling subtree, in a library with
    /// `faces_enabled`. Same pattern as `EmbeddingRepo::list_pending`,
    /// with `asset_face_scans` in place of `asset_embeddings` as the
    /// marker — an asset with no faces produces zero rows in `faces`,
    /// which alone is not enough to say "already analyzed". Does not take
    /// an `AuthContext`: system pipeline.
    ///
    /// # Errors
    /// `Connection` if the query fails (or if the faces schema does not exist).
    pub async fn list_pending_scan(
        &self,
        model_version: &str,
        limit: i64,
    ) -> Result<Vec<PendingFaceScan>, DbError> {
        let rows: Vec<(uuid::Uuid, Vec<u8>)> = sqlx::query_as(
            "SELECT a.id, a.content_hash \
             FROM assets a \
             JOIN folders f ON f.id = a.folder_id \
             JOIN libraries l ON l.id = f.library_id \
             LEFT JOIN folders cull ON cull.id = l.culling_root_folder_id \
             WHERE a.content_hash IS NOT NULL \
               AND a.kind = 'image' \
               AND l.faces_enabled \
               AND (cull.path IS NULL OR NOT (f.path <@ cull.path)) \
               AND NOT EXISTS ( \
                   SELECT 1 FROM asset_face_scans s \
                   WHERE s.asset_id = a.id AND s.model_version = $1 \
               ) \
             ORDER BY a.id \
             LIMIT $2",
        )
        .bind(model_version)
        .bind(limit)
        .fetch_all(self.db.pool())
        .await?;

        rows.into_iter()
            .map(|(id, hash)| {
                let content_hash: [u8; 32] = hash
                    .as_slice()
                    .try_into()
                    .map_err(|_| DbError::Corrupted(format!("content_hash len {}", hash.len())))?;
                Ok(PendingFaceScan {
                    asset_id: AssetId::from_uuid(id),
                    content_hash,
                })
            })
            .collect()
    }

    /// How many image assets (outside culling, in a library with
    /// `faces_enabled`) still need to pass through the detector for
    /// `model_version`.
    ///
    /// # Errors
    /// `Connection` / missing faces schema.
    pub async fn count_pending_scan(&self, model_version: &str) -> Result<i64, DbError> {
        let n: i64 = sqlx::query_scalar(
            "SELECT count(*)::bigint \
             FROM assets a \
             JOIN folders f ON f.id = a.folder_id \
             JOIN libraries l ON l.id = f.library_id \
             LEFT JOIN folders cull ON cull.id = l.culling_root_folder_id \
             WHERE a.content_hash IS NOT NULL \
               AND a.kind = 'image' \
               AND l.faces_enabled \
               AND (cull.path IS NULL OR NOT (f.path <@ cull.path)) \
               AND NOT EXISTS ( \
                   SELECT 1 FROM asset_face_scans s \
                   WHERE s.asset_id = a.id AND s.model_version = $1 \
               )",
        )
        .bind(model_version)
        .fetch_one(self.db.pool())
        .await?;
        Ok(n)
    }

    /// Records that `asset_id` has been passed through the detector, with
    /// or without faces found — the marker that makes `list_pending_scan`
    /// correct even for a photo with no faces at all. Does not take an
    /// `AuthContext`: pipeline.
    ///
    /// # Errors
    /// `Connection` if the query fails.
    pub async fn mark_scanned(
        &self,
        asset_id: AssetId,
        model_version: &str,
    ) -> Result<(), DbError> {
        sqlx::query(
            "INSERT INTO asset_face_scans (asset_id, model_version) VALUES ($1, $2) \
             ON CONFLICT (asset_id) DO UPDATE SET \
               model_version = EXCLUDED.model_version, scanned_at = now()",
        )
        .bind(asset_id.as_uuid())
        .bind(model_version)
        .execute(self.db.pool())
        .await?;
        Ok(())
    }

    /// Confirms all pending proposals for a candidate person, limited to
    /// faces visible to the caller — the review queue's "confirm all",
    /// same pattern as `AssetTagRepo::confirm_all_for_tag`. Returns the
    /// faces confirmed by this call.
    ///
    /// # Errors
    /// `Forbidden` without an authenticated user.
    pub async fn confirm_all_proposed_for_person(
        &self,
        ctx: &AuthContext,
        person_id: PersonId,
    ) -> Result<Vec<FaceId>, DbError> {
        let Some(user_id) = ctx.user_id() else {
            return Err(DbError::Forbidden);
        };
        let scope = VisibilityScope::resolve(self.db, ctx).await?;
        let filter = scope.filter("f.path", "f.library_id", "a.id", 3);

        let rows: Vec<(uuid::Uuid,)> = sqlx::query_as(&format!(
            "UPDATE faces fa SET person_id = $1, assigned_by = $2, assigned_at = now(), \
                                  proposed_person_id = NULL, proposed_score = NULL \
              WHERE fa.proposed_person_id = $1 AND fa.person_id IS NULL \
                AND EXISTS ( \
                  SELECT 1 FROM assets a JOIN folders f ON f.id = a.folder_id \
                   WHERE a.id = fa.asset_id AND {} \
                ) \
              RETURNING fa.id",
            filter.sql()
        ))
        .bind(person_id.as_uuid())
        .bind(user_id.as_uuid())
        .bind(filter.bind())
        .bind(filter.holes())
        .bind(filter.assets())
        .fetch_all(self.db.pool())
        .await?;
        let ids: Vec<FaceId> = rows
            .into_iter()
            .map(|(id,)| FaceId::from_uuid(id))
            .collect();
        self.recompute_affected_centroids(None, Some(person_id))
            .await?;
        Ok(ids)
    }

    /// Like [`Self::confirm_all_proposed_for_person`], but rejects —
    /// "reject all", permanent like [`Self::reject`].
    ///
    /// # Errors
    /// `Forbidden` without an authenticated user.
    pub async fn reject_all_proposed_for_person(
        &self,
        ctx: &AuthContext,
        person_id: PersonId,
    ) -> Result<Vec<FaceId>, DbError> {
        let Some(user_id) = ctx.user_id() else {
            return Err(DbError::Forbidden);
        };
        let scope = VisibilityScope::resolve(self.db, ctx).await?;
        let filter = scope.filter("f.path", "f.library_id", "a.id", 3);

        let rows: Vec<(uuid::Uuid,)> = sqlx::query_as(&format!(
            "UPDATE faces fa SET rejected_at = now(), assigned_by = $2, assigned_at = now(), \
                                  person_id = NULL, proposed_person_id = NULL, proposed_score = NULL \
              WHERE fa.proposed_person_id = $1 AND fa.person_id IS NULL \
                AND EXISTS ( \
                  SELECT 1 FROM assets a JOIN folders f ON f.id = a.folder_id \
                   WHERE a.id = fa.asset_id AND {} \
                ) \
              RETURNING fa.id",
            filter.sql()
        ))
        .bind(person_id.as_uuid())
        .bind(user_id.as_uuid())
        .bind(filter.bind())
        .bind(filter.holes())
        .bind(filter.assets())
        .fetch_all(self.db.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|(id,)| FaceId::from_uuid(id))
            .collect())
    }

    /// "Delete all face data": distinct from the `libraries.faces_enabled`
    /// switch, which stops computing new data but keeps what has already
    /// been collected. This command wipes `faces` (embeddings included),
    /// `persons`, `person_groups` — **globally**, not per library: a
    /// person can have faces across multiple libraries (clusters were
    /// never scoped to a library, see `PersonRepo::nearest_centroid`), so
    /// there is no library boundary for this action any more than there
    /// is one for the person itself. It also resets `asset_face_scans`:
    /// after deletion, every asset is "never analyzed" again, not
    /// "analyzed but zero faces" — otherwise a library that re-enables
    /// `faces_enabled` would never re-detect anything.
    ///
    /// # Errors
    /// `Forbidden` for anyone who is not an administrator — the same bar
    /// as `LibraryRepo::delete`, another destructive and irreversible
    /// action.
    pub async fn delete_all_data(&self, ctx: &AuthContext) -> Result<(), DbError> {
        if !ctx.is_admin() {
            return Err(DbError::Forbidden);
        }
        let mut tx = self.db.pool().begin().await?;
        sqlx::query("DELETE FROM person_groups")
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM persons").execute(&mut *tx).await?;
        sqlx::query("DELETE FROM faces").execute(&mut *tx).await?;
        sqlx::query("DELETE FROM asset_face_scans")
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Confirmed faces for a set of assets — "confirmed" here means
    /// `person_id IS NOT NULL AND rejected_at IS NULL`, whether manually
    /// assigned (`assigned_by` set) or from automatic clustering: both
    /// represent an established identity, unlike `proposed_person_id` (a
    /// suggestion not yet decided, never included here). Same idiom as
    /// [`crate::FlagRepo::favorites_among`]: a single query for the whole
    /// page. Returns an empty map — not an error — if pgvector is not
    /// installed: `faces`/`persons` do not exist at all in that case
    /// (same no-op already used in [`Self::count_proposed_visible`]).
    ///
    /// # Errors
    /// `Connection` if the query fails.
    pub async fn confirmed_among(
        &self,
        asset_ids: &[AssetId],
    ) -> Result<HashMap<AssetId, Vec<ConfirmedFace>>, DbError> {
        if asset_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let status = crate::pgvector::probe_pgvector(self.db).await?;
        if !status.available {
            return Ok(HashMap::new());
        }
        let ids: Vec<uuid::Uuid> = asset_ids.iter().map(AssetId::as_uuid).collect();
        let rows: Vec<ConfirmedFaceRow> = sqlx::query_as(
            "SELECT fa.asset_id, p.id AS person_id, p.name AS person_name \
               FROM faces fa JOIN persons p ON p.id = fa.person_id \
              WHERE fa.asset_id = ANY($1) AND fa.person_id IS NOT NULL \
                AND fa.rejected_at IS NULL",
        )
        .bind(&ids)
        .fetch_all(self.db.pool())
        .await?;
        let mut out: HashMap<AssetId, Vec<ConfirmedFace>> = HashMap::new();
        for row in rows {
            out.entry(AssetId::from_uuid(row.asset_id))
                .or_default()
                .push(ConfirmedFace {
                    person_id: PersonId::from_uuid(row.person_id),
                    person_name: row.person_name,
                });
        }
        Ok(out)
    }
}
