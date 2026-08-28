//! Duplicates by `content_hash`: **exact hash-based** deduplication, no
//! ML. A group is a set of assets sharing the same `content_hash` with
//! `count > 1`; the user action is "keep this one, delete the others".
//!
//! This started as the scan behind
//! [`crate::problems::ProblemsRepo::duplicates`]; here it is extended with
//! the list of a group's individual members (needed to choose which one
//! to keep) and with the resolution action, which reuses
//! [`crate::TrashRepo`] instead of reimplementing the three deletion
//! options.

use keeppix_domain::{Asset, AssetId, AssetStatus, AuthContext, DiskAction};

use crate::visibility::VisibilityScope;
use crate::{AssetRepo, Db, DbError, TrashRepo};

pub struct DuplicateRepo<'a> {
    db: &'a Db,
}

#[derive(Debug, Clone)]
pub struct DuplicateGroup {
    pub content_hash: Vec<u8>,
    pub count: i64,
    pub size_bytes: i64,
}

impl DuplicateGroup {
    /// Reclaimable space: `size_bytes * (copies - 1)`, **not** the total
    /// sum across copies — the first copy is not "reclaimable," it is the
    /// photo.
    #[must_use]
    pub const fn reclaimable_bytes(&self) -> i64 {
        self.size_bytes.saturating_mul(self.count.saturating_sub(1))
    }
}

impl<'a> DuplicateRepo<'a> {
    #[must_use]
    pub const fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// Groups sharing the same `content_hash` with more than one copy,
    /// visible to the caller. A `trashed` asset does not count: it is
    /// already queued to disappear, and its three deletion options would
    /// otherwise count one fewer copy than are truly reclaimable.
    ///
    /// # Errors
    /// `Connection` if the query fails.
    pub async fn groups(&self, ctx: &AuthContext) -> Result<Vec<DuplicateGroup>, DbError> {
        let scope = VisibilityScope::resolve(self.db, ctx).await?;
        let filter = scope.filter("f.path", "f.library_id", "a.id", 1);
        let rows: Vec<(Vec<u8>, i64, i64)> = sqlx::query_as(&format!(
            "SELECT a.content_hash, count(*)::bigint, min(a.size_bytes)::bigint \
               FROM assets a \
               JOIN folders f ON f.id = a.folder_id \
              WHERE a.content_hash IS NOT NULL AND a.status <> 'trashed' AND {} \
              GROUP BY a.content_hash \
             HAVING count(*) > 1 \
              ORDER BY (min(a.size_bytes) * (count(*) - 1)) DESC \
              LIMIT 200",
            filter.sql()
        ))
        .bind(filter.bind())
        .bind(filter.holes())
        .bind(filter.assets())
        .fetch_all(self.db.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|(content_hash, count, size_bytes)| DuplicateGroup {
                content_hash,
                count,
                size_bytes,
            })
            .collect())
    }

    /// The individual assets of a group, to choose which one to keep.
    /// Excludes `trashed` ones for the same reason as [`Self::groups`] —
    /// they make no sense as a target for "keep this" or "delete this,"
    /// they are already handled by trash.
    ///
    /// # Errors
    /// `Connection` if the query fails.
    pub async fn members(
        &self,
        ctx: &AuthContext,
        content_hash: &[u8; 32],
    ) -> Result<Vec<Asset>, DbError> {
        let assets = AssetRepo::new(self.db)
            .find_by_hash(ctx, content_hash)
            .await?;
        Ok(assets
            .into_iter()
            .filter(|a| a.status != AssetStatus::Trashed)
            .collect())
    }

    /// "Keep this one, delete the others": applies `action` to every other
    /// **non-trashed** member of the group. Reuses
    /// [`crate::TrashRepo::choose`] asset by asset — the three options,
    /// visibility checks, and the gate on `Purged` already live there and
    /// must not be duplicated here.
    ///
    /// # Errors
    /// `Forbidden` if `keep` is not visible to the caller or does not
    /// belong to the group — including when the id does not exist, so as
    /// not to offer an existence oracle. Otherwise, the same error as
    /// [`crate::TrashRepo::choose`] on the first member that fails: the
    /// others remain trashed — this is not an all-or-nothing operation,
    /// because a photo already moved must not be rolled back if the next
    /// one fails.
    pub async fn resolve(
        &self,
        ctx: &AuthContext,
        content_hash: &[u8; 32],
        keep: AssetId,
        action: DiskAction,
    ) -> Result<usize, DbError> {
        let members = self.members(ctx, content_hash).await?;
        if !members.iter().any(|a| a.id == keep) {
            // `keep` is not in the group (or is not visible: `find_by_hash`
            // would already have filtered it out) — the same treatment
            // given to any id probed outside the caller's own visibility.
            return Err(DbError::Forbidden);
        }

        let trash = TrashRepo::new(self.db);
        let mut resolved = 0usize;
        for member in members {
            if member.id == keep {
                continue;
            }
            trash.choose(ctx, member.id, action).await?;
            resolved += 1;
        }
        Ok(resolved)
    }
}
