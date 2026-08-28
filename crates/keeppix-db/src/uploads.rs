//! Resumable, tus-style upload sessions. See `keeppix_domain::upload`
//! for the domain types.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, Utc};
use keeppix_domain::{
    Actor, AssetId, AssetKind, AuthContext, CollisionOutcome, FolderId, ObjectRole, UploadOwner,
    UploadSession, UploadSessionId, UserId,
};
use uuid::Uuid;

use crate::folders::FolderRepo;
use crate::libraries::LibraryRepo;
use crate::permissions::PermissionRepo;
use crate::{Db, DbError};

/// Folder for upload temp files, inside the library root — same
/// filesystem as the final path, so the finalization `rename()` is atomic
/// even for a 2 GB file. **Must** stay excluded from the walker like
/// `.keeppix-trash` (`keeppix_media::walk::is_excluded_name`): a mismatch
/// here would produce a reindexing loop on the temp files themselves.
pub const UPLOAD_TMP_DIR_NAME: &str = ".keeppix-tmp";

/// Abandoned sessions expire after a week.
const SESSION_TTL_DAYS: i64 = 7;

/// Parameters to open a session. `AuthContext` decides who owns it: an
/// authenticated user, or — via `Actor::ShareLink` — a shared link with
/// `allow_upload`.
pub struct NewUploadSession {
    pub target_folder_id: FolderId,
    pub filename: String,
    pub expected_size: i64,
    /// blake3 of the whole file as declared by the client, if known in advance.
    pub expected_hash: Option<[u8; 32]>,
    pub client_mtime: Option<DateTime<Utc>>,
}

/// Outcome of finalization: the asset involved and how any name
/// collision was resolved.
pub struct FinalizeOutcome {
    pub asset_id: AssetId,
    /// Final name of the file on disk — may differ from the requested
    /// one if there was a rename due to a collision.
    pub filename: String,
    pub collision: CollisionOutcome,
}

#[derive(sqlx::FromRow)]
struct SessionRow {
    id: Uuid,
    user_id: Option<Uuid>,
    share_link_id: Option<Uuid>,
    target_folder_id: Uuid,
    filename: String,
    expected_size: i64,
    expected_hash: Option<Vec<u8>>,
    received_bytes: i64,
    temp_path: String,
    client_mtime: Option<DateTime<Utc>>,
    expires_at: DateTime<Utc>,
    created_at: DateTime<Utc>,
}

impl SessionRow {
    fn into_domain(self) -> Result<UploadSession, DbError> {
        let owner = match (self.user_id, self.share_link_id) {
            (Some(uid), None) => UploadOwner::User(UserId::from_uuid(uid)),
            (None, Some(link)) => UploadOwner::ShareLink(link),
            _ => {
                return Err(crate::row::corrupted(
                    "upload_sessions actor",
                    "neither or both of user_id/share_link_id are set, violating the one-actor check",
                ));
            }
        };
        let expected_hash = match self.expected_hash {
            None => None,
            Some(bytes) => Some(<[u8; 32]>::try_from(bytes.as_slice()).map_err(|_| {
                crate::row::corrupted("upload_sessions.expected_hash", "not 32 bytes")
            })?),
        };
        Ok(UploadSession {
            id: UploadSessionId::from_uuid(self.id),
            owner,
            target_folder_id: FolderId::from_uuid(self.target_folder_id),
            filename: self.filename,
            expected_size: self.expected_size,
            expected_hash,
            received_bytes: self.received_bytes,
            temp_path: PathBuf::from(self.temp_path),
            client_mtime: self.client_mtime,
            expires_at: self.expires_at,
            created_at: self.created_at,
        })
    }
}

const COLUMNS: &str = "id, user_id, share_link_id, target_folder_id, filename, expected_size, \
                       expected_hash, received_bytes, temp_path, client_mtime, expires_at, \
                       created_at";

pub struct UploadSessionRepo<'a> {
    db: &'a Db,
}

impl<'a> UploadSessionRepo<'a> {
    #[must_use]
    pub const fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// Opens a session. Checks, in order: write permission on the
    /// destination folder, then free space on the library's filesystem —
    /// never discovered midway through the upload.
    ///
    /// # Errors
    /// `Forbidden` if the caller cannot write to the folder, or if a
    /// shared link does not have `allow_upload` on the exact object.
    /// `InsufficientStorage` if free space is below `expected_size`.
    /// `Conflict` if `expected_size` is not positive. `Io` if
    /// `.keeppix-tmp/` cannot be created.
    pub async fn create(
        &self,
        ctx: &AuthContext,
        new: NewUploadSession,
    ) -> Result<UploadSession, DbError> {
        if new.expected_size <= 0 {
            return Err(DbError::Conflict(
                "expected_size must be positive".to_owned(),
            ));
        }

        // Shared-link exception or gate, before any disk access: without
        // allow_upload on the exact object, this must not even get to
        // occupy space in .keeppix-tmp/ (a pinned edge case).
        if let Actor::ShareLink {
            object_type,
            object_id,
            allow_upload,
            ..
        } = &ctx.actor
            && (!allow_upload
                || object_type != "folder"
                || *object_id != new.target_folder_id.as_uuid())
        {
            return Err(DbError::Forbidden);
        }

        let folder = FolderRepo::new(self.db)
            .find_by_id(ctx, new.target_folder_id)
            .await?;
        let library = LibraryRepo::new(self.db)
            .load_for_scan(folder.library_id)
            .await?;

        // An editor can write, a viewer cannot — same gate as
        // `FolderRepo::move_subtree`. An `Actor::ShareLink` has already
        // been filtered above: this check only concerns a user.
        if matches!(ctx.actor, Actor::User { .. })
            && !ctx.is_admin()
            && ctx.user_id() != Some(library.owner_id)
        {
            match PermissionRepo::new(self.db)
                .effective_role(ctx, new.target_folder_id)
                .await?
            {
                Some(ObjectRole::Editor) => {}
                _ => return Err(DbError::Forbidden),
            }
        }

        ensure_disk_space(&library.root_path, new.expected_size)?;

        let tmp_dir = library.root_path.join(UPLOAD_TMP_DIR_NAME);
        std::fs::create_dir_all(&tmp_dir)
            .map_err(|e| DbError::Io(format!("creating {}: {e}", tmp_dir.display())))?;

        let id = UploadSessionId::new();
        let temp_path = tmp_dir.join(format!("{id}_{}", new.filename));
        let expires_at = Utc::now() + Duration::days(SESSION_TTL_DAYS);

        let (user_id, share_link_id) = match &ctx.actor {
            Actor::User { id, .. } => (Some(id.as_uuid()), None),
            Actor::ShareLink { link_id, .. } => (None, Some(*link_id)),
        };

        let row: SessionRow = sqlx::query_as(&format!(
            "INSERT INTO upload_sessions \
                (id, user_id, share_link_id, target_folder_id, filename, expected_size, \
                 expected_hash, temp_path, client_mtime, expires_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) \
             RETURNING {COLUMNS}"
        ))
        .bind(id.as_uuid())
        .bind(user_id)
        .bind(share_link_id)
        .bind(new.target_folder_id.as_uuid())
        .bind(&new.filename)
        .bind(new.expected_size)
        .bind(new.expected_hash.map(|h| h.to_vec()))
        .bind(temp_path.to_string_lossy().into_owned())
        .bind(new.client_mtime)
        .bind(expires_at)
        .fetch_one(self.db.pool())
        .await?;

        row.into_domain()
    }

    /// Session with the offset that is actually true (`HEAD`: "the truth
    /// always lives on the server"). An expired session is cleaned up
    /// right away — row and temp file together — and returned as `Gone`.
    ///
    /// # Errors
    /// `Forbidden` if the caller is not the owner — even when the id does
    /// not exist, so as not to offer an existence oracle. `NotFound` only
    /// for an admin requesting a nonexistent id. `Gone` if the session
    /// has expired.
    pub async fn load_owned(
        &self,
        ctx: &AuthContext,
        id: UploadSessionId,
    ) -> Result<UploadSession, DbError> {
        let session = self.load_row(ctx, id).await?;
        if session.is_expired_at(Utc::now()) {
            self.delete_and_cleanup(id.as_uuid()).await?;
            return Err(DbError::Gone);
        }
        Ok(session)
    }

    /// Advances the offset after a chunk was successfully written to the temp file.
    ///
    /// # Errors
    /// `Forbidden` if the caller is not the owner, or if the session has
    /// vanished in the meantime (expired and cleaned up by another request).
    pub async fn advance(
        &self,
        ctx: &AuthContext,
        id: UploadSessionId,
        received_bytes: i64,
    ) -> Result<(), DbError> {
        let affected = if ctx.is_admin() {
            sqlx::query("UPDATE upload_sessions SET received_bytes = $2 WHERE id = $1")
                .bind(id.as_uuid())
                .bind(received_bytes)
                .execute(self.db.pool())
                .await?
                .rows_affected()
        } else {
            match &ctx.actor {
                Actor::User { id: uid, .. } => sqlx::query(
                    "UPDATE upload_sessions SET received_bytes = $2 \
                      WHERE id = $1 AND user_id = $3",
                )
                .bind(id.as_uuid())
                .bind(received_bytes)
                .bind(uid.as_uuid())
                .execute(self.db.pool())
                .await?
                .rows_affected(),
                Actor::ShareLink { link_id, .. } => sqlx::query(
                    "UPDATE upload_sessions SET received_bytes = $2 \
                      WHERE id = $1 AND share_link_id = $3",
                )
                .bind(id.as_uuid())
                .bind(received_bytes)
                .bind(link_id)
                .execute(self.db.pool())
                .await?
                .rows_affected(),
            }
        };
        if affected == 0 {
            return Err(DbError::Forbidden);
        }
        Ok(())
    }

    /// Marks the session as failed: deletes the row and the temp file
    /// together, never one without the other. Used when the end-to-end
    /// hash or the decodability check fails on the complete file.
    ///
    /// # Errors
    /// `Forbidden` if the caller is not the owner.
    pub async fn fail(&self, ctx: &AuthContext, id: UploadSessionId) -> Result<(), DbError> {
        self.load_row(ctx, id).await?;
        self.delete_and_cleanup(id.as_uuid()).await
    }

    /// Finalizes a complete session: resolves any name collision in the
    /// target folder, then moves the file from the temp location to the
    /// destination with `rename()` — a filesystem operation, which cannot
    /// live inside the SQL transaction — and **only afterward** creates
    /// the `assets` row (or skips it if it is an exact duplicate) and
    /// deletes the session, in the same transaction.
    ///
    /// The asymmetry is intentional: if the commit fails after a
    /// successful `rename()`, the file is left in the right place
    /// without an `assets` row — the next library scan discovers it and
    /// indexes it like any other file, exactly like an asset the walker
    /// just found. The opposite order (commit before `rename()`) would
    /// instead leave an asset pointing at a nonexistent file if the
    /// `rename()` later failed, with the session already deleted and
    /// therefore no reference left to the temp file to recover from — a
    /// file must **never** touch the target folder before being verified
    /// end to end, but once it is there it must never be left orphaned
    /// of truth in the database.
    ///
    /// # Errors
    /// Same as [`Self::load_owned`] for session ownership. `Io` if the
    /// final `rename()` fails — in that case the temp file stays put, no
    /// row is touched.
    pub async fn finalize(
        &self,
        ctx: &AuthContext,
        id: UploadSessionId,
        kind: AssetKind,
        content_hash: [u8; 32],
    ) -> Result<FinalizeOutcome, DbError> {
        let session = self.load_row(ctx, id).await?;
        let folder_dir = FolderRepo::new(self.db)
            .absolute_path(ctx, session.target_folder_id)
            .await?;
        let mtime = session.client_mtime.unwrap_or_else(Utc::now);

        let mut tx = self.db.pool().begin().await?;
        let existing: Option<(Uuid, Option<Vec<u8>>)> = sqlx::query_as(
            "SELECT id, content_hash FROM assets WHERE folder_id = $1 AND filename = $2",
        )
        .bind(session.target_folder_id.as_uuid())
        .bind(&session.filename)
        .fetch_optional(&mut *tx)
        .await?;

        if let Some((existing_id, existing_hash)) = &existing
            && existing_hash.as_deref() == Some(content_hash.as_slice())
        {
            // Same name, same hash: exact duplicate. The target folder is
            // never touched — only the temp file is removed, before the
            // commit: if the commit later failed, the session would be
            // left with a `temp_path` that no longer exists, tolerated
            // wherever it gets cleaned up again (`remove_file_tolerant`),
            // never the opposite risk of a phantom asset.
            remove_file_tolerant(&session.temp_path)?;
            sqlx::query("DELETE FROM upload_sessions WHERE id = $1")
                .bind(id.as_uuid())
                .execute(&mut *tx)
                .await?;
            tx.commit().await?;
            return Ok(FinalizeOutcome {
                asset_id: AssetId::from_uuid(*existing_id),
                filename: session.filename.clone(),
                collision: CollisionOutcome::SkippedDuplicate {
                    existing_asset_id: AssetId::from_uuid(*existing_id),
                },
            });
        }

        let (final_name, outcome) = if existing.is_some() {
            // Same name, different hash: never a silent overwrite — it is
            // saved with a numeric suffix (a pinned edge case).
            let taken: Vec<String> =
                sqlx::query_scalar("SELECT filename FROM assets WHERE folder_id = $1")
                    .bind(session.target_folder_id.as_uuid())
                    .fetch_all(&mut *tx)
                    .await?;
            let unique = unique_suffixed_name(&session.filename, &taken);
            (unique.clone(), CollisionOutcome::RenamedTo(unique))
        } else {
            (session.filename.clone(), CollisionOutcome::Created)
        };

        // `rename()` happens before the commit, not after: it is the
        // filesystem operation that makes true what the `assets` row is
        // about to assert. If it fails, we bail out without having
        // touched the database — the temp file stays put, the session is
        // still there for a retry.
        let target_path = folder_dir.join(&final_name);
        std::fs::rename(&session.temp_path, &target_path).map_err(|e| {
            DbError::Io(format!(
                "moving {} to {}: {e}",
                session.temp_path.display(),
                target_path.display()
            ))
        })?;

        let asset_id = AssetId::new();
        sqlx::query(
            "INSERT INTO assets (id, folder_id, filename, content_hash, size_bytes, mtime, kind) \
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(asset_id.as_uuid())
        .bind(session.target_folder_id.as_uuid())
        .bind(&final_name)
        .bind(content_hash.as_slice())
        .bind(session.expected_size)
        .bind(mtime)
        .bind(kind_str(kind))
        .execute(&mut *tx)
        .await
        .map_err(map_unique_violation)?;

        sqlx::query("DELETE FROM upload_sessions WHERE id = $1")
            .bind(id.as_uuid())
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;

        Ok(FinalizeOutcome {
            asset_id,
            filename: final_name,
            collision: outcome,
        })
    }

    /// Cleans up expired sessions: temp file and row together, never one
    /// without the other. Only `expires_at` decides — a session that is
    /// still alive is not touched even if `received_bytes` has been
    /// stalled for hours (a slow connection is not an abandoned upload).
    /// Tolerates a temp file that has already vanished (e.g. a crash that
    /// already emptied the directory): the row is deleted regardless.
    ///
    /// # Errors
    /// `DbError::Connection` if the query fails. `DbError::Io` if a temp
    /// file cannot be removed for a reason other than `ENOENT` (in that
    /// case rows already deleted in the same call are left without their
    /// file: the next library scan would treat the file as a newly
    /// discovered asset, no worse a risk than a session that never expired).
    pub async fn delete_expired(&self, now: DateTime<Utc>) -> Result<u64, DbError> {
        let rows: Vec<(String,)> =
            sqlx::query_as("DELETE FROM upload_sessions WHERE expires_at < $1 RETURNING temp_path")
                .bind(now)
                .fetch_all(self.db.pool())
                .await?;
        for (temp_path,) in &rows {
            remove_file_tolerant(Path::new(temp_path))?;
        }
        Ok(u64::try_from(rows.len()).unwrap_or(u64::MAX))
    }

    async fn load_row(
        &self,
        ctx: &AuthContext,
        id: UploadSessionId,
    ) -> Result<UploadSession, DbError> {
        let row: Option<SessionRow> = sqlx::query_as(&format!(
            "SELECT {COLUMNS} FROM upload_sessions WHERE id = $1"
        ))
        .bind(id.as_uuid())
        .fetch_optional(self.db.pool())
        .await?;
        let Some(row) = row else {
            return Err(if ctx.is_admin() {
                DbError::NotFound
            } else {
                DbError::Forbidden
            });
        };
        let session = row.into_domain()?;
        if !ctx.is_admin() && !owns(ctx, &session) {
            return Err(DbError::Forbidden);
        }
        Ok(session)
    }

    async fn delete_and_cleanup(&self, id: Uuid) -> Result<(), DbError> {
        let row: Option<(String,)> =
            sqlx::query_as("DELETE FROM upload_sessions WHERE id = $1 RETURNING temp_path")
                .bind(id)
                .fetch_optional(self.db.pool())
                .await?;
        if let Some((temp_path,)) = row {
            remove_file_tolerant(Path::new(&temp_path))?;
        }
        Ok(())
    }
}

fn owns(ctx: &AuthContext, session: &UploadSession) -> bool {
    match (&ctx.actor, session.owner) {
        (Actor::User { id, .. }, UploadOwner::User(owner)) => *id == owner,
        (Actor::ShareLink { link_id, .. }, UploadOwner::ShareLink(owner)) => *link_id == owner,
        _ => false,
    }
}

const fn kind_str(kind: AssetKind) -> &'static str {
    match kind {
        AssetKind::Image => "image",
        AssetKind::RawImage => "raw_image",
        AssetKind::Video => "video",
        AssetKind::Unknown => "unknown",
    }
}

/// Suffix style (`IMG_1234_1.ARW`): an underscore, not the hyphen used by
/// `unique_filename` in `share.rs`, which serves a different flow (guest
/// upload with no concept of content collision).
///
/// `pub(crate)`: also reused by `AssetRepo::ingest_direct` (`WebDAV PUT`),
/// which resolves the same collision without going through an upload session.
pub(crate) fn unique_suffixed_name(desired: &str, taken: &[String]) -> String {
    if !taken.iter().any(|name| name == desired) {
        return desired.to_owned();
    }
    let (stem, ext) = match desired.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => (stem, format!(".{ext}")),
        _ => (desired, String::new()),
    };
    for n in 1..10_000 {
        let candidate = format!("{stem}_{n}{ext}");
        if !taken.iter().any(|name| name == &candidate) {
            return candidate;
        }
    }
    format!("{stem}_{}{ext}", Uuid::now_v7())
}

/// `pub(crate)`: also reused by `AssetRepo::ingest_direct`.
pub(crate) fn remove_file_tolerant(path: &Path) -> Result<(), DbError> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(DbError::Io(format!("removing {}: {e}", path.display()))),
    }
}

/// `pub(crate)`: also reused by `AssetRepo::ingest_direct`, same
/// `(folder_id, filename)` collision.
pub(crate) fn map_unique_violation(err: sqlx::Error) -> DbError {
    if let sqlx::Error::Database(ref db_err) = err
        && db_err.code().as_deref() == Some("23505")
    {
        return DbError::Conflict("asset already exists in this folder".into());
    }
    DbError::Connection(err)
}

/// `pub`: reused by `keeppix_api::dav::write::put` (`WebDAV PUT`) for the
/// same check before writing the body to disk — unlike the `tus` session,
/// there is no row to reject "at creation" here, so the check must be
/// repeated on the spot in the `WebDAV` module. `pub(crate)` would not be
/// enough: the caller is in another crate of the workspace, and this
/// function contains no SQL, so exporting it does not violate the
/// invariant "no SQL outside `keeppix-db`" nor the
/// `keeppix-media` <-> `keeppix-db` dependency ban.
///
/// Free and total space on the volume that contains `root`.
///
/// # Errors
/// `Io` if `statvfs` fails.
pub fn disk_usage(root: &Path) -> Result<(u64, u64), DbError> {
    let (free, total) = statvfs_bytes(root)?;
    Ok((free, total))
}

/// # Errors
/// `InsufficientStorage` if the free space on `root` is below
/// `expected_size`. `Io` if `statvfs` fails.
pub fn ensure_disk_space(root: &Path, expected_size: i64) -> Result<(), DbError> {
    let needed = u64::try_from(expected_size).unwrap_or(u64::MAX);
    let (available, _) = statvfs_bytes(root)?;
    if available < needed {
        return Err(DbError::InsufficientStorage);
    }
    Ok(())
}

#[cfg(unix)]
fn statvfs_bytes(root: &Path) -> Result<(u64, u64), DbError> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt as _;

    let c_path = CString::new(root.as_os_str().as_bytes())
        .map_err(|_| DbError::Io("library root path contains a NUL byte".to_owned()))?;
    // SAFETY: `c_path` is a valid, nul-terminated `CString` for the
    // duration of the call; `stat` is zero-initialized, a valid bit
    // pattern for an all-integer struct, and `statvfs` fills it in
    // entirely before returning success.
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    let ret = unsafe { libc::statvfs(c_path.as_ptr(), &raw mut stat) };
    if ret != 0 {
        return Err(DbError::Io(format!(
            "statvfs failed for {}: {}",
            root.display(),
            std::io::Error::last_os_error()
        )));
    }
    // `f_bavail` / `f_blocks` / `f_frsize` widths differ by platform (`u32` vs
    // `u64`). On Linux all are already `u64`, so the casts look redundant to
    // clippy; keep them so macOS (and other libc layouts) still compile.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::unnecessary_cast
    )]
    {
        let frsize = stat.f_frsize as u64;
        let free = (stat.f_bavail as u64).saturating_mul(frsize);
        let total = (stat.f_blocks as u64).saturating_mul(frsize);
        Ok((free, total))
    }
}

#[cfg(not(unix))]
fn statvfs_bytes(_root: &Path) -> Result<(u64, u64), DbError> {
    Ok((u64::MAX, u64::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;
    use keeppix_domain::SystemRole;

    #[test]
    fn a_user_owns_only_their_own_session() {
        let owner = UserId::new();
        let ctx = AuthContext::user(owner, SystemRole::User);
        let session = sample_session(UploadOwner::User(owner));
        assert!(owns(&ctx, &session));

        let stranger_ctx = AuthContext::user(UserId::new(), SystemRole::User);
        assert!(!owns(&stranger_ctx, &session));
    }

    #[test]
    fn a_share_link_owns_only_the_session_it_opened() {
        let link_id = Uuid::now_v7();
        let ctx = AuthContext::share_link(
            link_id,
            keeppix_domain::ShareLinkParams {
                object_type: "folder".to_owned(),
                object_id: Uuid::now_v7(),
                allow_download: true,
                allow_original: false,
                hide_metadata: true,
                allow_upload: true,
                upload_quota_bytes: None,
            },
        );
        let session = sample_session(UploadOwner::ShareLink(link_id));
        assert!(owns(&ctx, &session));

        let other_link = AuthContext::share_link(
            Uuid::now_v7(),
            keeppix_domain::ShareLinkParams {
                object_type: "folder".to_owned(),
                object_id: Uuid::now_v7(),
                allow_download: true,
                allow_original: false,
                hide_metadata: true,
                allow_upload: true,
                upload_quota_bytes: None,
            },
        );
        assert!(!owns(&other_link, &session));
    }

    #[test]
    fn unique_suffixed_name_uses_an_underscore_not_a_hyphen() {
        let taken = vec!["IMG_1234.ARW".to_owned()];
        assert_eq!(
            unique_suffixed_name("IMG_1234.ARW", &taken),
            "IMG_1234_1.ARW"
        );
    }

    #[test]
    fn unique_suffixed_name_skips_taken_suffixes() {
        let taken = vec!["foto.jpg".to_owned(), "foto_1.jpg".to_owned()];
        assert_eq!(unique_suffixed_name("foto.jpg", &taken), "foto_2.jpg");
    }

    #[test]
    fn unique_suffixed_name_passes_through_when_free() {
        let taken: Vec<String> = vec![];
        assert_eq!(unique_suffixed_name("foto.jpg", &taken), "foto.jpg");
    }

    #[allow(clippy::unwrap_used)]
    fn sample_session(owner: UploadOwner) -> UploadSession {
        UploadSession {
            id: UploadSessionId::new(),
            owner,
            target_folder_id: FolderId::new(),
            filename: "foto.jpg".to_owned(),
            expected_size: 10,
            expected_hash: None,
            received_bytes: 0,
            temp_path: PathBuf::from("/tmp/x"),
            client_mtime: None,
            expires_at: Utc::now(),
            created_at: Utc::now(),
        }
    }
}
