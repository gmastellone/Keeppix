//! `WebDAV` Class 2 locks. No `AuthContext`: a lock does not belong to a
//! user the way assets or folders do — it is an opaque contract with the
//! client (Finder, Windows Explorer), identified by the token itself, not
//! by who created it. The same reason `LibraryRepo::mark_scanned` does not
//! take one either.

use crate::{Db, DbError};

/// Default TTL for a new or renewed lock (3600s).
const LOCK_TTL_SECONDS: i64 = 3600;

pub struct DavLockRepo<'a> {
    db: &'a Db,
}

impl<'a> DavLockRepo<'a> {
    #[must_use]
    pub const fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// Creates a lock with a timeout of [`LOCK_TTL_SECONDS`] from now.
    ///
    /// # Errors
    /// `Connection` if the insert fails (including a duplicate token,
    /// impossible by construction since the token is a fresh `Uuid` v7 on
    /// every call).
    pub async fn create(
        &self,
        token: &str,
        resource_path: &str,
        owner: Option<&str>,
        depth: &str,
    ) -> Result<(), DbError> {
        // `LOCK_TTL_SECONDS` is a code constant, never a value coming
        // from the client: the interpolation here does not reopen the
        // door to string concatenation of external values, which remains
        // off-limits.
        sqlx::query(&format!(
            "INSERT INTO dav_locks (token, resource_path, owner, depth, timeout_at) \
             VALUES ($1, $2, $3, $4, now() + interval '{LOCK_TTL_SECONDS} seconds')"
        ))
        .bind(token)
        .bind(resource_path)
        .bind(owner)
        .bind(depth)
        .execute(self.db.pool())
        .await?;
        Ok(())
    }

    /// Renews the timeout of a still-active lock. `false` if the token
    /// does not exist or has already expired — an expired lock is not
    /// renewed, it is recreated ("an expired lock is as good as absent").
    ///
    /// # Errors
    /// `Connection` if the update fails.
    pub async fn refresh(&self, token: &str) -> Result<bool, DbError> {
        let result = sqlx::query(&format!(
            "UPDATE dav_locks SET timeout_at = now() + interval '{LOCK_TTL_SECONDS} seconds' \
              WHERE token = $1 AND timeout_at > now()"
        ))
        .bind(token)
        .execute(self.db.pool())
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Deletes a lock, whether it exists or not. Unconditional: a caller
    /// that needs to distinguish "it existed and was active" from "it did
    /// not exist" uses [`Self::refresh`] before calling this (see
    /// `dav::lock::unlock`, which leverages `refresh` as a test-and-set
    /// instead of adding a fifth method).
    ///
    /// # Errors
    /// `Connection` if the deletion fails.
    pub async fn delete(&self, token: &str) -> Result<(), DbError> {
        sqlx::query("DELETE FROM dav_locks WHERE token = $1")
            .bind(token)
            .execute(self.db.pool())
            .await?;
        Ok(())
    }

    /// Is there an active (unexpired) lock on this path? Used by `LOCK`
    /// without `If:` to decide whether to create a new lock or respond
    /// `423 Locked` — an expired lock counts as absent.
    ///
    /// # Errors
    /// `Connection` if the query fails.
    pub async fn is_locked(&self, resource_path: &str) -> Result<bool, DbError> {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM dav_locks WHERE resource_path = $1 AND timeout_at > now())",
        )
        .bind(resource_path)
        .fetch_one(self.db.pool())
        .await?;
        Ok(exists)
    }
}
