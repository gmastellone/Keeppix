//! Sessioni di upload riprendibili in stile tus (spec §1). Vedi
//! `keeppix_domain::upload` per i tipi di dominio.

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

/// Cartella dei temporanei di upload, dentro la radice della libreria —
/// stesso filesystem del percorso finale, quindi il `rename()` di
/// finalizzazione è atomico anche per un file da 2 GB. **Deve** restare
/// esclusa dal walker come `.keeppix-trash`
/// (`keeppix_media::walk::is_excluded_name`): un disallineamento qui
/// produrrebbe un ciclo di reindicizzazione sui temporanei stessi.
pub const UPLOAD_TMP_DIR_NAME: &str = ".keeppix-tmp";

/// Le sessioni abbandonate scadono dopo una settimana (spec §1.3).
const SESSION_TTL_DAYS: i64 = 7;

/// Parametri per aprire una sessione. `AuthContext` decide chi ne è
/// proprietario: un utente autenticato, oppure — via `Actor::ShareLink` —
/// un link condiviso con `allow_upload`.
pub struct NewUploadSession {
    pub target_folder_id: FolderId,
    pub filename: String,
    pub expected_size: i64,
    /// blake3 del file intero dichiarato dal client, se noto in anticipo.
    pub expected_hash: Option<[u8; 32]>,
    pub client_mtime: Option<DateTime<Utc>>,
}

/// Esito della finalizzazione: l'asset coinvolto e come si è risolto un
/// eventuale scontro di nome.
pub struct FinalizeOutcome {
    pub asset_id: AssetId,
    /// Nome finale del file sul disco — può differire da quello richiesto
    /// se c'è stata una rinomina per collisione.
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

    /// Apre una sessione. Controlla, in ordine: il permesso di scrittura
    /// sulla cartella destinazione, poi lo spazio libero sul filesystem
    /// della libreria — mai scoperto a metà upload (spec §1.3).
    ///
    /// # Errors
    /// `Forbidden` se il chiamante non può scrivere nella cartella, o se un
    /// link condiviso non ha `allow_upload` sull'oggetto esatto.
    /// `InsufficientStorage` se lo spazio libero è sotto `expected_size`.
    /// `Conflict` se `expected_size` non è positivo. `Io` se `.keeppix-tmp/`
    /// non si può creare.
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

        // Deroga o cancello del link condiviso, prima di qualunque accesso a
        // disco: senza allow_upload sull'oggetto esatto non deve nemmeno
        // arrivare a occupare spazio in .keeppix-tmp/ (caso limite pinnato).
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

        // Un editor scrive, un viewer no — stessa porta di
        // `FolderRepo::move_subtree`. Un `Actor::ShareLink` è già stato
        // filtrato sopra: qui il controllo riguarda solo un utente.
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

    /// Sessione con l'offset che vale davvero (`HEAD`, spec §1.3: «la verità
    /// sta sempre sul server»). Una sessione scaduta viene ripulita subito —
    /// riga e temporaneo insieme — e restituita come `Gone`.
    ///
    /// # Errors
    /// `Forbidden` se il chiamante non è il proprietario — anche quando
    /// l'id non esiste, per non offrire un oracolo di esistenza. `NotFound`
    /// solo a un admin che chiede un id inesistente. `Gone` se la sessione è
    /// scaduta.
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

    /// Avanza l'offset dopo un chunk scritto con successo sul temporaneo.
    ///
    /// # Errors
    /// `Forbidden` se il chiamante non è il proprietario, o se la sessione è
    /// sparita nel frattempo (scaduta e ripulita da un'altra richiesta).
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

    /// Marca la sessione fallita: cancella la riga e il temporaneo insieme,
    /// mai l'uno senza l'altro. Usata quando l'hash end-to-end o la verifica
    /// di decodificabilità falliscono a file completo.
    ///
    /// # Errors
    /// `Forbidden` se il chiamante non è il proprietario.
    pub async fn fail(&self, ctx: &AuthContext, id: UploadSessionId) -> Result<(), DbError> {
        self.load_row(ctx, id).await?;
        self.delete_and_cleanup(id.as_uuid()).await
    }

    /// Finalizza una sessione completa: risolve un'eventuale collisione di
    /// nome nella cartella target, poi muove il file dal temporaneo alla
    /// destinazione con `rename()` — un'operazione sul filesystem, che non
    /// può stare dentro la transazione SQL — e **solo dopo** crea la riga
    /// `assets` (o la salta se è un duplicato esatto) e cancella la
    /// sessione, nella stessa transazione.
    ///
    /// L'asimmetria è intenzionale: se il commit fallisce dopo un `rename()`
    /// riuscito, il file resta al posto giusto senza una riga `assets` — la
    /// prossima scansione della libreria lo scopre e lo indicizza come un
    /// file qualunque, esattamente come un asset che il walker ha appena
    /// trovato. L'ordine opposto (commit prima del `rename()`) lascerebbe
    /// invece un asset che punta a un file inesistente se il `rename()`
    /// fallisse dopo, con la sessione già cancellata e quindi senza più
    /// alcun riferimento al temporaneo da cui recuperare — un file deve
    /// **mai** toccare la cartella target prima di essere verificato
    /// end-to-end, ma una volta lì non deve mai restare orfano di verità nel
    /// database.
    ///
    /// # Errors
    /// Come [`Self::load_owned`] per il possesso della sessione. `Io` se il
    /// `rename()` finale fallisce — in tal caso il temporaneo resta al suo
    /// posto, nessuna riga viene toccata.
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
            // Stesso nome, stesso hash: duplicato esatto. Non si tocca mai
            // la cartella target — si rimuove solo il temporaneo, prima del
            // commit: se il commit fallisse dopo, la sessione resterebbe con
            // un `temp_path` non più esistente, tollerato ovunque venga
            // ripulito di nuovo (`remove_file_tolerant`), mai il rischio
            // opposto di un asset fantasma.
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
            // Stesso nome, hash diverso: mai una sovrascrittura silenziosa —
            // si salva con un suffisso numerico (caso limite pinnato).
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

        // Il `rename()` viene prima del commit, non dopo: è l'operazione sul
        // filesystem che rende vero ciò che la riga `assets` sta per
        // affermare. Se fallisce, si abbandona senza aver toccato il
        // database — il temporaneo resta al suo posto, la sessione è ancora
        // lì per un retry.
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

    /// Ripulisce le sessioni scadute: temporaneo e riga insieme, mai l'uno
    /// senza l'altro (spec §1.3, Task 2). Solo `expires_at` decide — una
    /// sessione ancora viva non viene toccata anche se `received_bytes` è
    /// fermo da ore (una connessione lenta non è un upload abbandonato).
    /// Tollera un temporaneo già sparito (es. un crash che ha già svuotato
    /// la directory): la riga va cancellata comunque.
    ///
    /// # Errors
    /// `DbError::Connection` se la query fallisce. `DbError::Io` se un
    /// temporaneo non si può rimuovere per un motivo diverso da `ENOENT`
    /// (in tal caso le righe già cancellate nella stessa chiamata resta
    /// senza il proprio file: la scansione successiva della libreria
    /// tratterebbe il file come un asset scoperto, non un rischio peggiore
    /// di una sessione mai scaduta).
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

/// Stesso suffisso descritto dalla spec (`IMG_1234_1.ARW`): un
/// sottotrattino, non il trattino di `unique_filename` in `share.rs`, che
/// serve a un flusso diverso (upload ospite senza concetto di collisione di
/// contenuto).
///
/// `pub(crate)`: riusata anche da `AssetRepo::ingest_direct` (`WebDAV PUT`,
/// Task 7 Fase 5), che risolve la stessa collisione senza passare da una
/// sessione di upload.
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

/// `pub(crate)`: riusata anche da `AssetRepo::ingest_direct` (Task 7).
pub(crate) fn remove_file_tolerant(path: &Path) -> Result<(), DbError> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(DbError::Io(format!("removing {}: {e}", path.display()))),
    }
}

/// `pub(crate)`: riusata anche da `AssetRepo::ingest_direct` (Task 7), stessa
/// collisione `(folder_id, filename)`.
pub(crate) fn map_unique_violation(err: sqlx::Error) -> DbError {
    if let sqlx::Error::Database(ref db_err) = err
        && db_err.code().as_deref() == Some("23505")
    {
        return DbError::Conflict("asset already exists in this folder".into());
    }
    DbError::Connection(err)
}

/// `pub`: riusata da `keeppix_api::dav::write::put` (`WebDAV PUT`, Task 7
/// Fase 5, fix round) per lo stesso controllo prima di scrivere il body su
/// disco — a differenza della sessione `tus` qui non c'è una riga da
/// rifiutare "alla creazione", quindi il controllo va ripetuto punto per
/// punto nel modulo `WebDAV`. `pub(crate)` non basterebbe: il chiamante è in
/// un altro crate del workspace, e questa funzione non contiene SQL, quindi
/// esportarla non viola l'invariante "nessun SQL fuori da `keeppix-db`" né
/// il divieto di dipendenza `keeppix-media` ↔ `keeppix-db` (ledger Task 7).
///
/// Spazio libero e totale sul volume che contiene `root`.
///
/// # Errors
/// `Io` se `statvfs` fallisce.
pub fn disk_usage(root: &Path) -> Result<(u64, u64), DbError> {
    let (free, total) = statvfs_bytes(root)?;
    Ok((free, total))
}

/// # Errors
/// `InsufficientStorage` se lo spazio libero su `root` è sotto
/// `expected_size`. `Io` se `statvfs` fallisce.
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
    // SAFETY: `c_path` è un `CString` valido e nul-terminated per la durata
    // della chiamata; `stat` è zero-inizializzato, un pattern di bit valido
    // per una struct di soli interi, e `statvfs` lo riempie per intero
    // prima di restituire successo.
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
