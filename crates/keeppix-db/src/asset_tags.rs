//! AI-proposed `asset_tags` assignments, and the human review queue over
//! those proposals.
//!
//! [`Self::propose_for_tag`] / [`Self::propose_for_assets`] do not take an
//! `AuthContext`: this is the system's analysis pipeline, like
//! [`crate::EmbeddingRepo`]. Matching is not a user action — it fires after
//! creating/patching a tag with an embedding, or after a batch of photo
//! embeddings.
//!
//! The human decisions ([`Self::confirm`], [`Self::reject`], and their bulk
//! variants) **do take** an `AuthContext`: they are user actions, and a user
//! must not be able to decide on (or learn of the existence of) a proposal
//! on an asset they cannot see. Once decided, `confirmed`/`rejected` are
//! never overwritten by a rematch (`ON CONFLICT ... WHERE state = 'proposed'`
//! in [`Self::propose_for_tag`]).

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use keeppix_domain::{AssetId, AuthContext, TagId};

use crate::pgvector::probe_pgvector;
use crate::visibility::VisibilityScope;
use crate::{AssetRepo, Db, DbError};

/// A tag confirmed on an asset, as returned by
/// [`AssetTagRepo::confirmed_among`] — not the table's raw assignment row
/// (`state`/`source` are not needed by the caller, which has already
/// filtered on `state='confirmed'`).
#[derive(Debug, Clone, PartialEq)]
pub struct ConfirmedTag {
    pub tag_id: TagId,
    pub name: String,
    pub color: Option<String>,
    /// The tag's `parent_id`: "categories" are tags with `kind='category'`,
    /// not a separate table — a tag with no parent (or whose hierarchy is
    /// not set) has `None`.
    pub category_id: Option<TagId>,
}

#[derive(Debug, sqlx::FromRow)]
struct ConfirmedTagRow {
    asset_id: uuid::Uuid,
    tag_id: uuid::Uuid,
    name: String,
    color: Option<String>,
    parent_id: Option<uuid::Uuid>,
}

/// A tag of an asset for the info panel — unlike [`ConfirmedTag`], this
/// also carries `state` (`"confirmed"` or `"proposed"`, never `"rejected"`,
/// filtered out by [`AssetTagRepo::for_asset`]) and `source` (`"ai"` or
/// `"user"`, the column's raw values): the caller uses them to choose
/// between the three chip renderings, not to build logic here.
#[derive(Debug, Clone, PartialEq)]
pub struct AssetTagDetail {
    pub tag_id: TagId,
    pub name: String,
    pub color: Option<String>,
    pub category_id: Option<TagId>,
    pub state: String,
    pub source: String,
}

#[derive(Debug, sqlx::FromRow)]
struct AssetTagDetailRow {
    tag_id: uuid::Uuid,
    name: String,
    color: Option<String>,
    parent_id: Option<uuid::Uuid>,
    state: String,
    source: String,
}

impl AssetTagDetailRow {
    fn into_domain(self) -> AssetTagDetail {
        AssetTagDetail {
            tag_id: TagId::from_uuid(self.tag_id),
            name: self.name,
            color: self.color,
            category_id: self.parent_id.map(TagId::from_uuid),
            state: self.state,
            source: self.source,
        }
    }
}

/// The two possible human decisions on a proposal. Internal-only: it is
/// never serialized, the translation to/from the SQL string stays here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Decision {
    Confirmed,
    Rejected,
}

impl Decision {
    const fn as_state(self) -> &'static str {
        match self {
            Self::Confirmed => "confirmed",
            Self::Rejected => "rejected",
        }
    }
}

/// A pending proposal, as the review queue displays it: already enriched
/// with the tag name and filename, so the caller does not need a second
/// round of queries per row.
#[derive(Debug, Clone, PartialEq)]
pub struct ProposalView {
    pub asset_id: AssetId,
    pub tag_id: TagId,
    pub tag_name: String,
    pub score: Option<f32>,
    pub filename: String,
    pub taken_at_utc: Option<DateTime<Utc>>,
}

#[derive(Debug, sqlx::FromRow)]
struct ProposalRow {
    asset_id: uuid::Uuid,
    tag_id: uuid::Uuid,
    tag_name: String,
    score: Option<f32>,
    filename: String,
    taken_at_utc: Option<DateTime<Utc>>,
}

impl ProposalRow {
    fn into_view(self) -> ProposalView {
        ProposalView {
            asset_id: AssetId::from_uuid(self.asset_id),
            tag_id: TagId::from_uuid(self.tag_id),
            tag_name: self.tag_name,
            score: self.score,
            filename: self.filename,
            taken_at_utc: self.taken_at_utc,
        }
    }
}

/// Band below the tag's threshold: score >= `threshold - BAND` still
/// produces a proposal (lower score -> bottom of the queue). System
/// constant, not exposed in the API.
///
/// Calibrated against `OpenCLIP` XLM-R IT/EN: real text-image cosine
/// similarity in this embedding space sits around 0.10-0.20, not 0-1 — a
/// band of 0.01 (one percentage point on the old, implicit scale) was too
/// tight to catch correct but weak matches (observed minimum
/// `correct_score`: 0.126-0.132).
pub const TAG_MATCH_BAND: f32 = 0.05;

pub struct AssetTagRepo<'a> {
    db: &'a Db,
}

impl<'a> AssetTagRepo<'a> {
    #[must_use]
    pub const fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// Matches all photos with an embedding at the same `model_version` as
    /// the tag. Only inserts/updates rows with `state='proposed'`,
    /// `source='ai'`.
    ///
    /// # Errors
    /// `Connection` if the query fails (or if the AI schema does not exist).
    pub async fn propose_for_tag(&self, tag_id: TagId) -> Result<u64, DbError> {
        let result = sqlx::query(
            "INSERT INTO asset_tags (asset_id, tag_id, state, source, score) \
             SELECT ae.asset_id, t.id, 'proposed', 'ai', \
                    (1.0 - (ae.embedding <=> t.embedding))::real \
             FROM tags t \
             JOIN asset_embeddings ae \
               ON ae.model_version = t.model_version \
             WHERE t.id = $1 \
               AND t.kind = 'tag' \
               AND t.embedding IS NOT NULL \
               AND t.model_version IS NOT NULL \
               AND (1.0 - (ae.embedding <=> t.embedding)) \
                   >= (t.threshold - $2::real) \
             ON CONFLICT (asset_id, tag_id) DO UPDATE \
               SET score = EXCLUDED.score \
               WHERE asset_tags.state = 'proposed'",
        )
        .bind(tag_id.as_uuid())
        .bind(TAG_MATCH_BAND)
        .execute(self.db.pool())
        .await?;
        Ok(result.rows_affected())
    }

    /// Matches the given assets against all tags with an embedding (same
    /// `model_version`). Same rules as [`Self::propose_for_tag`].
    ///
    /// # Errors
    /// `Connection` if the query fails (or if the AI schema does not exist).
    pub async fn propose_for_assets(&self, asset_ids: &[AssetId]) -> Result<u64, DbError> {
        if asset_ids.is_empty() {
            return Ok(0);
        }
        let uuids: Vec<uuid::Uuid> = asset_ids.iter().map(AssetId::as_uuid).collect();
        let result = sqlx::query(
            "INSERT INTO asset_tags (asset_id, tag_id, state, source, score) \
             SELECT ae.asset_id, t.id, 'proposed', 'ai', \
                    (1.0 - (ae.embedding <=> t.embedding))::real \
             FROM tags t \
             JOIN asset_embeddings ae \
               ON ae.model_version = t.model_version \
             WHERE ae.asset_id = ANY($1) \
               AND t.kind = 'tag' \
               AND t.embedding IS NOT NULL \
               AND t.model_version IS NOT NULL \
               AND (1.0 - (ae.embedding <=> t.embedding)) \
                   >= (t.threshold - $2::real) \
             ON CONFLICT (asset_id, tag_id) DO UPDATE \
               SET score = EXCLUDED.score \
               WHERE asset_tags.state = 'proposed'",
        )
        .bind(&uuids)
        .bind(TAG_MATCH_BAND)
        .execute(self.db.pool())
        .await?;
        Ok(result.rows_affected())
    }

    /// Lists pending proposals (`state = 'proposed'`), filtered by the
    /// caller's visibility and, optionally, by a single tag. Ordered by
    /// `score` descending (null scores, theoretically impossible for a
    /// proposed row, sort last).
    ///
    /// # Errors
    /// `Forbidden` without an authenticated user (a public link has no
    /// review queue); `Connection` on DB error or missing AI schema.
    pub async fn list_proposed(
        &self,
        ctx: &AuthContext,
        tag_id: Option<TagId>,
    ) -> Result<Vec<ProposalView>, DbError> {
        if ctx.user_id().is_none() {
            return Err(DbError::Forbidden);
        }
        let scope = VisibilityScope::resolve(self.db, ctx).await?;
        let filter = scope.filter("f.path", "f.library_id", "a.id", 2);
        let rows: Vec<ProposalRow> = sqlx::query_as(&format!(
            "SELECT at.asset_id, at.tag_id, t.name AS tag_name, at.score, \
                    a.filename, a.taken_at_utc \
             FROM asset_tags at \
             JOIN tags t ON t.id = at.tag_id \
             JOIN assets a ON a.id = at.asset_id \
             JOIN folders f ON f.id = a.folder_id \
             WHERE at.state = 'proposed' \
               AND ($1::uuid IS NULL OR at.tag_id = $1) \
               AND {} \
             ORDER BY at.score DESC NULLS LAST, at.asset_id",
            filter.sql()
        ))
        .bind(tag_id.map(|id| id.as_uuid()))
        .bind(filter.bind())
        .bind(filter.holes())
        .bind(filter.assets())
        .fetch_all(self.db.pool())
        .await?;
        Ok(rows.into_iter().map(ProposalRow::into_view).collect())
    }

    /// Number of `asset_tags` in `state = 'proposed'` whose asset is
    /// visible to the caller — the "tags" half of the combined
    /// `bootstrap.badges.revision` badge (the "faces" half shares the same
    /// field).
    ///
    /// Unlike the other methods on this repository, this one **does not
    /// propagate** the error when the AI schema does not exist (pgvector
    /// missing on the connected Postgres): `bootstrap` is the startup
    /// bundle for the entire application, not an optional endpoint like
    /// `/tags`, so an external Postgres without pgvector must not break
    /// startup of the whole interface.
    ///
    /// # Errors
    /// `Connection` if the query (including the pgvector probe) fails for a
    /// reason other than the extension being missing.
    pub async fn count_proposed_visible(&self, ctx: &AuthContext) -> Result<i64, DbError> {
        if ctx.user_id().is_none() {
            return Ok(0);
        }
        let status = probe_pgvector(self.db).await?;
        if !status.available {
            return Ok(0);
        }

        let scope = VisibilityScope::resolve(self.db, ctx).await?;
        let filter = scope.filter("f.path", "f.library_id", "a.id", 1);
        let count: i64 = sqlx::query_scalar(&format!(
            "SELECT count(*) FROM asset_tags at \
             JOIN assets a ON a.id = at.asset_id \
             JOIN folders f ON f.id = a.folder_id \
             WHERE at.state = 'proposed' AND {}",
            filter.sql()
        ))
        .bind(filter.bind())
        .bind(filter.holes())
        .bind(filter.assets())
        .fetch_one(self.db.pool())
        .await?;
        Ok(count)
    }

    /// Confirms a proposal: `state = 'confirmed'`, `decided_by`/`decided_at`
    /// set. Idempotent if it was already confirmed.
    ///
    /// # Errors
    /// `Forbidden` if the asset is not visible to the caller (or the
    /// context has no user) — never `NotFound`, so as not to offer an
    /// existence oracle on the asset. `NotFound` if the asset is visible
    /// but this tag was never proposed on it. `Conflict` if the pair has
    /// already been decided in the opposite direction (`rejected`): a
    /// permanent decision does not get reversed.
    pub async fn confirm(
        &self,
        ctx: &AuthContext,
        tag_id: TagId,
        asset_id: AssetId,
    ) -> Result<(), DbError> {
        self.decide(ctx, tag_id, asset_id, Decision::Confirmed)
            .await
    }

    /// Rejects a proposal, permanently: `state = 'rejected'`,
    /// `decided_by`/`decided_at` set. A rematch never resurrects it
    /// ([`Self::propose_for_tag`] only updates rows with
    /// `state = 'proposed'`). Idempotent if already rejected.
    ///
    /// # Errors
    /// Same as [`Self::confirm`], with the roles of `confirmed`/`rejected`
    /// swapped.
    pub async fn reject(
        &self,
        ctx: &AuthContext,
        tag_id: TagId,
        asset_id: AssetId,
    ) -> Result<(), DbError> {
        self.decide(ctx, tag_id, asset_id, Decision::Rejected).await
    }

    async fn decide(
        &self,
        ctx: &AuthContext,
        tag_id: TagId,
        asset_id: AssetId,
        decision: Decision,
    ) -> Result<(), DbError> {
        let Some(user_id) = ctx.user_id() else {
            return Err(DbError::Forbidden);
        };
        AssetRepo::new(self.db)
            .assert_visible(ctx, std::slice::from_ref(&asset_id))
            .await?;

        let target = decision.as_state();
        let transitioned: Option<(String,)> = sqlx::query_as(
            "UPDATE asset_tags SET state = $3, decided_by = $4, decided_at = now() \
             WHERE tag_id = $1 AND asset_id = $2 AND state = 'proposed' \
             RETURNING state",
        )
        .bind(tag_id.as_uuid())
        .bind(asset_id.as_uuid())
        .bind(target)
        .bind(user_id.as_uuid())
        .fetch_optional(self.db.pool())
        .await?;
        if transitioned.is_some() {
            return Ok(());
        }

        // It was not 'proposed': fall back to an honest lookup instead of a
        // silent 0-row update — this distinguishes "never proposed",
        // "already decided the same way" (idempotent), and "already
        // decided the other way" (a real conflict).
        let current: Option<(String,)> =
            sqlx::query_as("SELECT state FROM asset_tags WHERE tag_id = $1 AND asset_id = $2")
                .bind(tag_id.as_uuid())
                .bind(asset_id.as_uuid())
                .fetch_optional(self.db.pool())
                .await?;
        match current {
            None => Err(DbError::NotFound),
            Some((state,)) if state == target => Ok(()),
            Some((state,)) => Err(DbError::Conflict(format!(
                "asset_tags already decided as '{state}'; cannot become '{target}'"
            ))),
        }
    }

    /// Confirms all pending proposals for a tag, limited to assets visible
    /// to the caller — the review queue's "Confirm all". Returns the ids
    /// confirmed by this call.
    ///
    /// # Errors
    /// `Forbidden` without an authenticated user; `Connection` on DB error.
    pub async fn confirm_all_for_tag(
        &self,
        ctx: &AuthContext,
        tag_id: TagId,
    ) -> Result<Vec<AssetId>, DbError> {
        self.decide_all_for_tag(ctx, tag_id, Decision::Confirmed)
            .await
    }

    /// Like [`Self::confirm_all_for_tag`], but rejects — "Reject all",
    /// permanent like [`Self::reject`].
    ///
    /// # Errors
    /// Same as [`Self::confirm_all_for_tag`].
    pub async fn reject_all_for_tag(
        &self,
        ctx: &AuthContext,
        tag_id: TagId,
    ) -> Result<Vec<AssetId>, DbError> {
        self.decide_all_for_tag(ctx, tag_id, Decision::Rejected)
            .await
    }

    async fn decide_all_for_tag(
        &self,
        ctx: &AuthContext,
        tag_id: TagId,
        decision: Decision,
    ) -> Result<Vec<AssetId>, DbError> {
        let Some(user_id) = ctx.user_id() else {
            return Err(DbError::Forbidden);
        };
        let scope = VisibilityScope::resolve(self.db, ctx).await?;
        let filter = scope.filter("f.path", "f.library_id", "a.id", 4);
        let target = decision.as_state();

        let rows: Vec<(uuid::Uuid,)> = sqlx::query_as(&format!(
            "UPDATE asset_tags at SET state = $2, decided_by = $3, decided_at = now() \
             WHERE at.tag_id = $1 AND at.state = 'proposed' \
               AND EXISTS ( \
                 SELECT 1 FROM assets a JOIN folders f ON f.id = a.folder_id \
                  WHERE a.id = at.asset_id AND {} \
               ) \
             RETURNING at.asset_id",
            filter.sql()
        ))
        .bind(tag_id.as_uuid())
        .bind(target)
        .bind(user_id.as_uuid())
        .bind(filter.bind())
        .bind(filter.holes())
        .bind(filter.assets())
        .fetch_all(self.db.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|(id,)| AssetId::from_uuid(id))
            .collect())
    }

    /// Assigns a tag to an asset by a person's direct decision (a manual
    /// addition is already a confirmation — it does not go through the
    /// review queue). Unlike [`Self::confirm`] — which only transitions
    /// from `'proposed'` and conflicts on an already-decided `'rejected'`
    /// — here the person is deciding *now*, not resolving a past AI
    /// proposal: it always writes `state='confirmed', source='user'`, even
    /// over a previous rejection. Idempotent.
    ///
    /// # Errors
    /// `Forbidden` without an authenticated user or if the asset is not
    /// visible to the caller. `Connection` if the write fails.
    pub async fn assign(
        &self,
        ctx: &AuthContext,
        tag_id: TagId,
        asset_id: AssetId,
    ) -> Result<(), DbError> {
        let Some(user_id) = ctx.user_id() else {
            return Err(DbError::Forbidden);
        };
        AssetRepo::new(self.db)
            .assert_visible(ctx, std::slice::from_ref(&asset_id))
            .await?;
        sqlx::query(
            "INSERT INTO asset_tags (asset_id, tag_id, state, source, decided_by, decided_at) \
             VALUES ($1, $2, 'confirmed', 'user', $3, now()) \
             ON CONFLICT (asset_id, tag_id) DO UPDATE SET \
               state = 'confirmed', source = 'user', decided_by = $3, decided_at = now()",
        )
        .bind(asset_id.as_uuid())
        .bind(tag_id.as_uuid())
        .bind(user_id.as_uuid())
        .execute(self.db.pool())
        .await?;
        Ok(())
    }

    /// Removes a manually assigned tag from an asset — the reverse of
    /// [`Self::assign`] (a toggle that adds or removes a tag from
    /// everyone). A real `DELETE` of the row, not a transition to
    /// `state='rejected'`: that state is the **permanent** decision of the
    /// AI review queue ([`Self::reject`], which explicitly refuses to
    /// reverse on a conflict) — the wrong semantics for "I changed my mind
    /// about a tag I added by hand". With a `DELETE`, reassigning the same
    /// tag later goes back through [`Self::assign`] without colliding with
    /// `Conflict`. Idempotent: no row to delete is not an error.
    ///
    /// # Errors
    /// `Forbidden` without an authenticated user or if the asset is not
    /// visible to the caller. `Connection` if the deletion fails.
    pub async fn unassign(
        &self,
        ctx: &AuthContext,
        tag_id: TagId,
        asset_id: AssetId,
    ) -> Result<(), DbError> {
        if ctx.user_id().is_none() {
            return Err(DbError::Forbidden);
        }
        AssetRepo::new(self.db)
            .assert_visible(ctx, std::slice::from_ref(&asset_id))
            .await?;
        sqlx::query("DELETE FROM asset_tags WHERE asset_id = $1 AND tag_id = $2")
            .bind(asset_id.as_uuid())
            .bind(tag_id.as_uuid())
            .execute(self.db.pool())
            .await?;
        Ok(())
    }

    /// Confirmed tags for a set of assets — only `state='confirmed'`,
    /// never pending proposals or rejections (only a photo's confirmed
    /// tags are shown). Same idiom as
    /// [`crate::FlagRepo::favorites_among`]: a single query for the whole
    /// page. Returns an empty map — not an error — if pgvector is not
    /// installed: `tags`/`asset_tags` do not exist at all in that case
    /// (same no-op already used in [`Self::count_proposed_visible`]).
    ///
    /// # Errors
    /// `Connection` if the query fails.
    pub async fn confirmed_among(
        &self,
        asset_ids: &[AssetId],
    ) -> Result<HashMap<AssetId, Vec<ConfirmedTag>>, DbError> {
        if asset_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let status = probe_pgvector(self.db).await?;
        if !status.available {
            return Ok(HashMap::new());
        }
        let ids: Vec<uuid::Uuid> = asset_ids.iter().map(AssetId::as_uuid).collect();
        let rows: Vec<ConfirmedTagRow> = sqlx::query_as(
            "SELECT at.asset_id, t.id AS tag_id, t.name, t.color, t.parent_id \
               FROM asset_tags at JOIN tags t ON t.id = at.tag_id \
              WHERE at.asset_id = ANY($1) AND at.state = 'confirmed'",
        )
        .bind(&ids)
        .fetch_all(self.db.pool())
        .await?;
        let mut out: HashMap<AssetId, Vec<ConfirmedTag>> = HashMap::new();
        for row in rows {
            out.entry(AssetId::from_uuid(row.asset_id))
                .or_default()
                .push(ConfirmedTag {
                    tag_id: TagId::from_uuid(row.tag_id),
                    name: row.name,
                    color: row.color,
                    category_id: row.parent_id.map(TagId::from_uuid),
                });
        }
        Ok(out)
    }

    /// Tags of **one** asset for the lightbox info panel: confirmed **and**
    /// pending proposals — never rejected ones, which must stay
    /// permanently out of view. Unlike [`Self::confirmed_among`] (bulk,
    /// confirmed only), this also needs `state`/`source` to distinguish the
    /// panel's three renderings: human-confirmed (solid chip), confirmed
    /// but AI-originated (`.ai-applied`, "AI" marker), and pending proposal
    /// (dashed chip, separate section).
    ///
    /// # Errors
    /// `Forbidden` if the asset is not visible to the caller. `Connection`
    /// if the query fails.
    pub async fn for_asset(
        &self,
        ctx: &AuthContext,
        asset_id: AssetId,
    ) -> Result<Vec<AssetTagDetail>, DbError> {
        AssetRepo::new(self.db)
            .assert_visible(ctx, std::slice::from_ref(&asset_id))
            .await?;
        let status = probe_pgvector(self.db).await?;
        if !status.available {
            return Ok(Vec::new());
        }
        let rows: Vec<AssetTagDetailRow> = sqlx::query_as(
            "SELECT t.id AS tag_id, t.name, t.color, t.parent_id, at.state, at.source \
               FROM asset_tags at JOIN tags t ON t.id = at.tag_id \
              WHERE at.asset_id = $1 AND at.state IN ('confirmed', 'proposed') \
              ORDER BY t.name ASC",
        )
        .bind(asset_id.as_uuid())
        .fetch_all(self.db.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(AssetTagDetailRow::into_domain)
            .collect())
    }

    /// Removes an **already-confirmed** tag from the info panel: not a
    /// `DELETE` like [`Self::unassign`] (which serves the manual addition
    /// from bulk edit), but a permanent transition to `state='rejected'`
    /// — removing a tag from a photo is itself a human decision and must
    /// stay permanent (otherwise a re-analysis could bring back a tag the
    /// user deliberately removed). This works on both AI- and
    /// human-originated tags: here the person is removing a tag that is
    /// already present, not deciding on a proposal ([`Self::reject`],
    /// which only transitions from `'proposed'`). Idempotent if already
    /// rejected.
    ///
    /// # Errors
    /// `Forbidden` without an authenticated user or if the asset is not
    /// visible. `NotFound` if the tag was never assigned to this asset.
    /// `Conflict` if it is still `'proposed'` (it must be confirmed or
    /// rejected from the queue, not "removed" — the two sections are
    /// handled separately).
    pub async fn remove_confirmed(
        &self,
        ctx: &AuthContext,
        tag_id: TagId,
        asset_id: AssetId,
    ) -> Result<(), DbError> {
        let Some(user_id) = ctx.user_id() else {
            return Err(DbError::Forbidden);
        };
        AssetRepo::new(self.db)
            .assert_visible(ctx, std::slice::from_ref(&asset_id))
            .await?;
        let transitioned: Option<(String,)> = sqlx::query_as(
            "UPDATE asset_tags SET state = 'rejected', decided_by = $3, decided_at = now() \
             WHERE tag_id = $1 AND asset_id = $2 AND state = 'confirmed' \
             RETURNING state",
        )
        .bind(tag_id.as_uuid())
        .bind(asset_id.as_uuid())
        .bind(user_id.as_uuid())
        .fetch_optional(self.db.pool())
        .await?;
        if transitioned.is_some() {
            return Ok(());
        }
        let current: Option<(String,)> =
            sqlx::query_as("SELECT state FROM asset_tags WHERE tag_id = $1 AND asset_id = $2")
                .bind(tag_id.as_uuid())
                .bind(asset_id.as_uuid())
                .fetch_optional(self.db.pool())
                .await?;
        match current {
            None => Err(DbError::NotFound),
            Some((state,)) if state == "rejected" => Ok(()),
            Some((state,)) => Err(DbError::Conflict(format!(
                "asset_tags is '{state}', not 'confirmed'; cannot remove"
            ))),
        }
    }
}
