use std::time::Duration;

use chrono::{DateTime, Utc};
use keeppix_domain::{AuthContext, SessionId, SessionToken, SystemRole, UserId};
use uuid::Uuid;

use crate::{Db, DbError};

/// Row of `GET /users/me/sessions`: one per family, i.e. per
/// device/login (see `SessionId`), not one per `sessions` row — rows
/// consumed by rotation are not an extra device.
pub struct SessionSummary {
    pub id: SessionId,
    pub device_label: Option<String>,
    /// When the family's active row was created: at login, or at the
    /// last rotation (`POST /auth/refresh`) if there was one. This is the
    /// cheapest available approximation of "last used" without a counter
    /// updated on every authenticated request (which `authenticate()`
    /// explicitly does not do: see `authenticate_does_not_slide_expiry`).
    pub last_seen_at: DateTime<Utc>,
    pub current: bool,
}

pub struct SessionRepo<'a> {
    db: &'a Db,
}

#[derive(sqlx::FromRow)]
struct AuthRow {
    user_id: uuid::Uuid,
    role: String,
}

impl AuthRow {
    fn into_domain(self) -> Result<AuthContext, DbError> {
        // Same taxonomy as `users.rs`: a value the code cannot interpret is
        // `Corrupted`, not a role silently downgraded to `User`. The CHECK
        // on the column makes this unreachable and the downgrade would
        // fail closed anyway, but two modules treating the same data
        // differently would make error triage unreliable.
        let role = match self.role.as_str() {
            "admin" => SystemRole::Admin,
            "user" => SystemRole::User,
            other => return Err(crate::row::corrupted("role", other)),
        };
        Ok(AuthContext::user(UserId::from_uuid(self.user_id), role))
    }
}

#[derive(sqlx::FromRow)]
struct RotateRow {
    id: uuid::Uuid,
    family_id: uuid::Uuid,
    user_id: uuid::Uuid,
    device_label: Option<String>,
    consumed_at: Option<chrono::DateTime<chrono::Utc>>,
    revoked_at: Option<chrono::DateTime<chrono::Utc>>,
    expires_at: chrono::DateTime<chrono::Utc>,
    db_now: chrono::DateTime<chrono::Utc>,
}

impl<'a> SessionRepo<'a> {
    #[must_use]
    pub const fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// Opens a new session family. The plaintext token is returned only
    /// once: only the digest remains in the database.
    ///
    /// # Errors
    /// `DbError::Connection` if the insert fails.
    pub async fn create(
        &self,
        user_id: UserId,
        ttl: Duration,
        user_agent: Option<&str>,
    ) -> Result<SessionToken, DbError> {
        let token = SessionToken::generate();
        let id = Uuid::now_v7();
        let device_label = device_label_from_user_agent(user_agent);

        sqlx::query(
            "INSERT INTO sessions \
                 (id, family_id, user_id, refresh_token_hash, device_label, expires_at) \
             VALUES ($1, $1, $2, $3, $4, now() + $5::interval)",
        )
        .bind(id)
        .bind(user_id.as_uuid())
        .bind(token.digest().as_slice())
        .bind(&device_label)
        .bind(interval(ttl))
        .execute(self.db.pool())
        .await?;

        Ok(token)
    }

    /// # Errors
    /// `DbError::NotFound` if the token is unknown, expired, consumed,
    /// revoked, or if the user is disabled.
    pub async fn authenticate(&self, token: &SessionToken) -> Result<AuthContext, DbError> {
        let row: Option<AuthRow> = sqlx::query_as(
            "SELECT u.id AS user_id, u.role \
               FROM sessions s JOIN users u ON u.id = s.user_id \
              WHERE s.refresh_token_hash = $1 \
                AND s.consumed_at IS NULL \
                AND s.revoked_at IS NULL \
                AND s.expires_at > now() \
                AND u.disabled_at IS NULL",
        )
        .bind(token.digest().as_slice())
        .fetch_optional(self.db.pool())
        .await?;

        row.ok_or(DbError::NotFound)?.into_domain()
    }

    /// Rotates the token. If the one presented turns out to be **already
    /// consumed**, the only explanation is that a copy is in someone
    /// else's hands: the entire family is revoked and a fresh login is
    /// forced.
    ///
    /// # Errors
    /// `DbError::Forbidden` if reuse is detected; `DbError::NotFound` if
    /// the token does not exist or has expired.
    pub async fn rotate(
        &self,
        token: &SessionToken,
        ttl: Duration,
    ) -> Result<SessionToken, DbError> {
        let mut tx = self.db.pool().begin().await?;

        let row: Option<RotateRow> = sqlx::query_as(
            "SELECT s.id, s.family_id, s.user_id, s.device_label, s.consumed_at, s.revoked_at, \
                    s.expires_at, now() AS db_now \
               FROM sessions s \
               JOIN users u ON u.id = s.user_id \
              WHERE s.refresh_token_hash = $1 AND u.disabled_at IS NULL \
              FOR UPDATE OF s",
        )
        .bind(token.digest().as_slice())
        .fetch_optional(&mut *tx)
        .await?;
        let row = row.ok_or(DbError::NotFound)?;

        if row.consumed_at.is_some() {
            sqlx::query(
                "UPDATE sessions SET revoked_at = now() \
                  WHERE family_id = $1 AND revoked_at IS NULL",
            )
            .bind(row.family_id)
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            return Err(DbError::Forbidden);
        }

        if row.revoked_at.is_some() {
            return Err(DbError::NotFound);
        }

        if row.expires_at <= row.db_now {
            return Err(DbError::NotFound);
        }

        sqlx::query("UPDATE sessions SET consumed_at = now() WHERE id = $1")
            .bind(row.id)
            .execute(&mut *tx)
            .await?;

        let next = SessionToken::generate();
        sqlx::query(
            "INSERT INTO sessions \
                 (id, family_id, user_id, refresh_token_hash, parent_id, device_label, \
                  expires_at) \
             VALUES ($1, $2, $3, $4, $5, $6, now() + $7::interval)",
        )
        .bind(Uuid::now_v7())
        .bind(row.family_id)
        .bind(row.user_id)
        .bind(next.digest().as_slice())
        .bind(row.id)
        .bind(&row.device_label)
        .bind(interval(ttl))
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(next)
    }

    /// # Errors
    /// `DbError::Connection` if the update fails.
    pub async fn revoke(&self, token: &SessionToken) -> Result<(), DbError> {
        sqlx::query(
            "UPDATE sessions SET revoked_at = now() \
              WHERE refresh_token_hash = $1 AND revoked_at IS NULL",
        )
        .bind(token.digest().as_slice())
        .execute(self.db.pool())
        .await?;
        Ok(())
    }

    /// Revokes all of a user's sessions. Used when the user is disabled.
    ///
    /// # Errors
    /// `DbError::Connection`.
    pub async fn revoke_all_for_user(
        &self,
        user_id: keeppix_domain::UserId,
    ) -> Result<u64, DbError> {
        let result = sqlx::query(
            "UPDATE sessions SET revoked_at = now() \
              WHERE user_id = $1 AND revoked_at IS NULL",
        )
        .bind(user_id.as_uuid())
        .execute(self.db.pool())
        .await?;
        Ok(result.rows_affected())
    }

    /// Revokes all of the user's families except the current token's
    /// (password change: other sessions must drop).
    ///
    /// # Errors
    /// `DbError::Connection`.
    pub async fn revoke_other_families(
        &self,
        user_id: keeppix_domain::UserId,
        keep_token: &SessionToken,
    ) -> Result<u64, DbError> {
        let result = sqlx::query(
            "UPDATE sessions SET revoked_at = now() \
              WHERE user_id = $1 AND revoked_at IS NULL \
                AND family_id <> ( \
                  SELECT family_id FROM sessions \
                   WHERE refresh_token_hash = $2 LIMIT 1 \
                )",
        )
        .bind(user_id.as_uuid())
        .bind(keep_token.digest().as_slice())
        .execute(self.db.pool())
        .await?;
        Ok(result.rows_affected())
    }

    /// # Errors
    /// `DbError::Connection` if the deletion fails.
    pub async fn purge_expired(&self) -> Result<u64, DbError> {
        let result = sqlx::query("DELETE FROM sessions WHERE expires_at <= now()")
            .execute(self.db.pool())
            .await?;
        Ok(result.rows_affected())
    }

    /// Family of the presented token — i.e. the session id (`SessionId`)
    /// that `GET /users/me/sessions` must mark `current: true`.
    /// **Documented exception** to the invariant "every method that reads
    /// a user's data takes an `AuthContext`" for the same reason as
    /// `authenticate`: the token itself is already the credential, and is
    /// needed to build the comparison *before* the session list (which
    /// does take an `AuthContext`) is queried.
    ///
    /// # Errors
    /// `DbError::Connection` if the query fails. An unknown token is not
    /// an error: it returns `Ok(None)`, leaving it to the caller to
    /// decide whether that is an anomaly (the `Auth` extractor has
    /// already verified the token authenticates, so in practice `Some`
    /// always arrives).
    pub async fn family_of(&self, token: &SessionToken) -> Result<Option<SessionId>, DbError> {
        let family: Option<Uuid> =
            sqlx::query_scalar("SELECT family_id FROM sessions WHERE refresh_token_hash = $1")
                .bind(token.digest().as_slice())
                .fetch_optional(self.db.pool())
                .await?;
        Ok(family.map(SessionId::from_uuid))
    }

    /// List of the authenticated user's active sessions, one per family:
    /// `consumed_at IS NULL AND revoked_at IS NULL AND expires_at >
    /// now()` selects exactly the live row of each family, because
    /// rotation (`rotate`) always marks `consumed_at` on the superseded
    /// row before inserting a new one — no `GROUP BY` needed.
    ///
    /// # Errors
    /// `DbError::Forbidden` if the caller is not an authenticated user.
    pub async fn list_active(
        &self,
        ctx: &AuthContext,
        current: SessionId,
    ) -> Result<Vec<SessionSummary>, DbError> {
        let user_id = ctx.user_id().ok_or(DbError::Forbidden)?;
        let rows: Vec<ActiveRow> = sqlx::query_as(
            "SELECT family_id, device_label, created_at, (family_id = $2) AS current \
               FROM sessions \
              WHERE user_id = $1 AND consumed_at IS NULL AND revoked_at IS NULL \
                AND expires_at > now() \
              ORDER BY created_at DESC",
        )
        .bind(user_id.as_uuid())
        .bind(current.as_uuid())
        .fetch_all(self.db.pool())
        .await?;
        Ok(rows.into_iter().map(ActiveRow::into_domain).collect())
    }

    /// Revokes a session family (`GET .../sessions` lists it as one row,
    /// not as its rotations). Owner only, never another user: an id
    /// belonging to someone else responds `Forbidden`, never `NotFound` —
    /// same rule as `AppPasswordRepo::revoke`, and for the same reason
    /// (existence oracle). **Does not** check whether `family` is the
    /// current family: that check (400, not 403/404 — it is not an
    /// ownership problem) lives in the HTTP handler, which already has
    /// the current token at hand and does not need to query the database
    /// for it.
    ///
    /// # Errors
    /// `DbError::Forbidden` if the caller does not own the family, or if
    /// the family does not exist.
    pub async fn revoke_family(&self, ctx: &AuthContext, family: SessionId) -> Result<(), DbError> {
        let user_id = ctx.user_id().ok_or(DbError::Forbidden)?;

        let owner: Option<Uuid> =
            sqlx::query_scalar("SELECT user_id FROM sessions WHERE family_id = $1 LIMIT 1")
                .bind(family.as_uuid())
                .fetch_optional(self.db.pool())
                .await?;
        let Some(owner) = owner else {
            return Err(DbError::Forbidden);
        };
        if owner != user_id.as_uuid() {
            return Err(DbError::Forbidden);
        }

        sqlx::query(
            "UPDATE sessions SET revoked_at = now() \
              WHERE family_id = $1 AND revoked_at IS NULL",
        )
        .bind(family.as_uuid())
        .execute(self.db.pool())
        .await?;
        Ok(())
    }
}

#[derive(sqlx::FromRow)]
struct ActiveRow {
    family_id: Uuid,
    device_label: Option<String>,
    created_at: DateTime<Utc>,
    current: bool,
}

impl ActiveRow {
    fn into_domain(self) -> SessionSummary {
        SessionSummary {
            id: SessionId::from_uuid(self.family_id),
            device_label: self.device_label,
            last_seen_at: self.created_at,
            current: self.current,
        }
    }
}

/// Postgres does not accept a Rust `Duration`: an interval is passed as
/// **fractional** seconds. `as_secs()` used to truncate: a 500ms TTL
/// became `"0 seconds"`, i.e. a token born already expired with no error
/// at all — silent and insidious, and the first test that wants to
/// observe an expiry without waiting a full second would run right into
/// it. `as_secs_f64()` goes well beyond `interval`'s microsecond
/// precision, and a zero TTL still yields `"0 seconds"`, i.e. an
/// already-expired token: a property the existing tests rely on.
fn interval(ttl: Duration) -> String {
    format!("{} seconds", ttl.as_secs_f64())
}

/// Short label ("Chrome on macOS") derived from a `User-Agent`, to show
/// in `GET /users/me/sessions` *which* device holds a session without
/// storing its full value (the entire `User-Agent` is more personal data
/// than necessary). `None` only when the header itself is missing; a
/// present but unrecognized header produces "Unknown device", not
/// `None` — the user still has a device, we simply do not know its name.
///
/// The order of checks matters: the most common browser/OS substrings
/// contain each other in a real `User-Agent` (Edge and Chrome both
/// contain `Safari/`; Chrome contains `Safari/`; an iPhone claims "like
/// Mac OS X"), so the first check that matches must be the most specific.
fn device_label_from_user_agent(user_agent: Option<&str>) -> Option<String> {
    let ua = user_agent?;

    let os = if ua.contains("iPhone") || ua.contains("iPad") || ua.contains("iPod") {
        Some("iOS")
    } else if ua.contains("Android") {
        Some("Android")
    } else if ua.contains("Windows") {
        Some("Windows")
    } else if ua.contains("Mac OS X") {
        Some("macOS")
    } else if ua.contains("Linux") {
        Some("Linux")
    } else {
        None
    };

    let browser = if ua.contains("Edg/") || ua.contains("Edge/") {
        Some("Edge")
    } else if ua.contains("OPR/") || ua.contains("Opera") {
        Some("Opera")
    } else if ua.contains("Firefox/") {
        Some("Firefox")
    } else if ua.contains("Chrome/") || ua.contains("CriOS/") {
        Some("Chrome")
    } else if ua.contains("Safari/") {
        Some("Safari")
    } else {
        None
    };

    Some(match (browser, os) {
        (Some(browser), Some(os)) => format!("{browser} on {os}"),
        (Some(browser), None) => browser.to_owned(),
        (None, Some(os)) => os.to_owned(),
        (None, None) => "Unknown device".to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::{device_label_from_user_agent, interval};
    use std::time::Duration;

    #[test]
    fn sub_second_ttls_survive_the_round_trip() {
        assert_eq!(interval(Duration::from_millis(500)), "0.5 seconds");
        assert_eq!(interval(Duration::from_micros(1500)), "0.0015 seconds");
        // A zero TTL must stay zero: the immediate-expiry tests rely on it.
        assert_eq!(interval(Duration::ZERO), "0 seconds");
        // Integer values do not gain decimals: `2592000 seconds`, not
        // `2592000.0 seconds`.
        assert_eq!(interval(Duration::from_secs(2_592_000)), "2592000 seconds");
    }

    const CHROME_MACOS: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) \
        AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36";
    const FIREFOX_WINDOWS: &str =
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:128.0) Gecko/20100101 Firefox/128.0";
    const SAFARI_IOS: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 17_5_1 like Mac OS X) \
        AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.5 Mobile/15E148 Safari/604.1";
    const CHROME_ANDROID: &str = "Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 \
        (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36";
    const EDGE_WINDOWS: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
        (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36 Edg/126.0.0.0";

    #[test]
    fn known_browser_and_os_combinations_are_labelled() {
        assert_eq!(
            device_label_from_user_agent(Some(CHROME_MACOS)),
            Some("Chrome on macOS".to_owned())
        );
        assert_eq!(
            device_label_from_user_agent(Some(FIREFOX_WINDOWS)),
            Some("Firefox on Windows".to_owned())
        );
        assert_eq!(
            device_label_from_user_agent(Some(SAFARI_IOS)),
            Some("Safari on iOS".to_owned())
        );
        assert_eq!(
            device_label_from_user_agent(Some(CHROME_ANDROID)),
            Some("Chrome on Android".to_owned())
        );
        // Edge carries both `Chrome/` and `Safari/` in its own string:
        // `Edg/` must win, otherwise every Edge would be labelled "Chrome".
        assert_eq!(
            device_label_from_user_agent(Some(EDGE_WINDOWS)),
            Some("Edge on Windows".to_owned())
        );
    }

    #[test]
    fn a_missing_or_unrecognised_user_agent_is_none() {
        assert_eq!(device_label_from_user_agent(None), None);
        assert_eq!(
            device_label_from_user_agent(Some("curl/8.7.1")),
            Some("Unknown device".to_owned())
        );
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn the_full_user_agent_never_survives_into_the_label() {
        // The property the migration declares: the label must never
        // contain the full original string (nor a specific version
        // number, which would identify the device more than a label
        // should).
        let label = device_label_from_user_agent(Some(CHROME_MACOS)).unwrap();
        assert!(!label.contains("126.0.0.0"));
        assert!(!label.contains("AppleWebKit"));
        assert!(label.len() < CHROME_MACOS.len());
    }
}
